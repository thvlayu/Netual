use anyhow::Result;
use bytes::{Buf, BufMut, BytesMut};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::RwLock;
use tokio::time::interval;

const BUFFER_SIZE: usize = 65536;
const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const PACKET_HEADER_SIZE: usize = 8; // session_id(4) + packet_seq(4)

/// Represents a client session with multiple connections (WiFi + Mobile)
#[derive(Debug)]
struct ClientSession {
    session_id: u32,
    connections: HashMap<SocketAddr, ConnectionInfo>,
    packet_buffer: HashMap<u32, Vec<u8>>, // packet_seq -> packet data
    next_expected_seq: u32,
    last_activity: Instant,
    internet_socket: Option<Arc<UdpSocket>>, // For forwarding to internet
}

#[derive(Debug, Clone)]
struct ConnectionInfo {
    last_seen: Instant,
    packets_received: u64,
}

type Sessions = Arc<RwLock<HashMap<u32, ClientSession>>>;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("🚀 Netual Server starting...");

    let sessions: Sessions = Arc::new(RwLock::new(HashMap::new()));

    // UDP listener for receiving tunneled packets from Android clients
    let tunnel_socket = Arc::new(UdpSocket::bind("0.0.0.0:9999").await?);
    info!("📡 Tunnel server listening on UDP 0.0.0.0:9999");

    // Control listener for initial handshake/registration
    let control_listener = TcpListener::bind("0.0.0.0:9998").await?;
    info!("🔌 Control server listening on TCP 0.0.0.0:9998");

    // Spawn session cleanup task
    tokio::spawn(cleanup_sessions(sessions.clone()));

    // Spawn control connection handler
    let control_sessions = sessions.clone();
    tokio::spawn(async move {
        loop {
            match control_listener.accept().await {
                Ok((stream, addr)) => {
                    info!("📲 New control connection from {}", addr);
                    let sessions = control_sessions.clone();
                    tokio::spawn(handle_control_connection(stream, addr, sessions));
                }
                Err(e) => error!("Control accept error: {}", e),
            }
        }
    });

    // Main tunnel packet handler
    handle_tunnel_packets(tunnel_socket, sessions).await?;

    Ok(())
}

/// Handle control connection for client registration
async fn handle_control_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    sessions: Sessions,
) -> Result<()> {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;

    if n < 4 {
        warn!("Invalid control message from {}", addr);
        return Ok(());
    }

    let msg = String::from_utf8_lossy(&buf[..n]);
    info!("Control message from {}: {}", addr, msg.trim());

    // Simple protocol: "REGISTER"
    if msg.starts_with("REGISTER") {
        let session_id = rand::random::<u32>();
        
        // Create new session
        let mut sessions = sessions.write().await;
        sessions.insert(
            session_id,
            ClientSession {
                session_id,
                connections: HashMap::new(),
                packet_buffer: HashMap::new(),
                next_expected_seq: 0,
                last_activity: Instant::now(),
                internet_socket: None,
            },
        );

        info!("✅ Created session {} for {}", session_id, addr);

        // Send session ID back
        let response = format!("SESSION_ID:{}\n", session_id);
        stream.write_all(response.as_bytes()).await?;
        stream.flush().await?;
    }

    Ok(())
}

/// Main packet handler - receives packets from Android client and forwards to internet
async fn handle_tunnel_packets(socket: Arc<UdpSocket>, sessions: Sessions) -> Result<()> {
    let mut buf = vec![0u8; BUFFER_SIZE];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((n, src_addr)) => {
                let data = &buf[..n];
                let sessions_clone = sessions.clone();
                let socket_clone = socket.clone();
                let data_owned = data.to_vec();

                tokio::spawn(async move {
                    if let Err(e) = process_tunnel_packet(
                        data_owned,
                        src_addr,
                        sessions_clone,
                        socket_clone,
                    )
                    .await
                    {
                        debug!("Error processing packet from {}: {}", src_addr, e);
                    }
                });
            }
            Err(e) => {
                error!("UDP receive error: {}", e);
            }
        }
    }
}

/// Process individual tunnel packet
async fn process_tunnel_packet(
    data: Vec<u8>,
    src_addr: SocketAddr,
    sessions: Sessions,
    tunnel_socket: Arc<UdpSocket>,
) -> Result<()> {
    if data.len() < PACKET_HEADER_SIZE {
        return Ok(()); // Too small
    }

    let mut cursor = &data[..];
    let session_id = cursor.get_u32();
    let packet_seq = cursor.get_u32();
    let payload = &data[PACKET_HEADER_SIZE..];

    debug!(
        "📦 Packet from {}: session={}, seq={}, size={}",
        src_addr,
        session_id,
        packet_seq,
        payload.len()
    );

    let mut sessions = sessions.write().await;
    let session = sessions.get_mut(&session_id);

    if session.is_none() {
        warn!("Unknown session {} from {}", session_id, src_addr);
        return Ok(());
    }

    let session = session.unwrap();

    // Update connection info
    session
        .connections
        .entry(src_addr)
        .and_modify(|info| {
            info.last_seen = Instant::now();
            info.packets_received += 1;
        })
        .or_insert(ConnectionInfo {
            last_seen: Instant::now(),
            packets_received: 1,
        });

    session.last_activity = Instant::now();

    // Deduplicate packets (client sends on both WiFi and Mobile)
    if session.packet_buffer.contains_key(&packet_seq) {
        debug!("🔄 Duplicate packet {} ignored (already processed)", packet_seq);
        return Ok(());
    }

    // Store packet for deduplication
    session.packet_buffer.insert(packet_seq, payload.to_vec());
    
    // Keep buffer size manageable
    if session.packet_buffer.len() > 100 {
        // Remove old packets
        let min_seq = packet_seq.saturating_sub(100);
        session.packet_buffer.retain(|seq, _| *seq > min_seq);
    }

    // Create internet socket if not exists (for forwarding)
    if session.internet_socket.is_none() {
        let internet_sock = UdpSocket::bind("0.0.0.0:0").await?;
        session.internet_socket = Some(Arc::new(internet_sock));
        
        // Spawn response handler for ALL registered client connections
        let internet_sock = session.internet_socket.as_ref().unwrap().clone();
        let tunnel_sock = tunnel_socket.clone();
        let sess_id = session_id;
        let sessions_clone = sessions.clone();
        
        tokio::spawn(handle_internet_responses(
            internet_sock,
            tunnel_sock,
            sess_id,
            sessions_clone,
        ));
        
        info!("🌐 Created internet socket for session {}", session_id);
    }

    // Forward packet to internet immediately (accept out-of-order)
    if let Some(ref internet_sock) = session.internet_socket {
        if payload.len() > 20 {
            match forward_packet_to_internet(internet_sock, payload).await {
                Ok(_) => {
                    debug!("✅ Forwarded packet seq {} to internet", packet_seq);
                    if packet_seq >= session.next_expected_seq {
                        session.next_expected_seq = packet_seq + 1;
                    }
                }
                Err(e) => {
                    debug!("Failed to forward packet: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Forward packet to actual internet destination
async fn forward_packet_to_internet(socket: &UdpSocket, payload: &[u8]) -> Result<()> {
    // This is a simplified version
    // Real implementation needs to parse IP packet header and extract dest IP/port
    // For now, just forward to Google DNS as example
    socket.send_to(payload, "8.8.8.8:53").await?;
    Ok(())
}

/// Handle responses from internet back to Android client
async fn handle_internet_responses(
    internet_socket: Arc<UdpSocket>,
    tunnel_socket: Arc<UdpSocket>,
    session_id: u32,
    sessions: Sessions,
) {
    let mut buf = vec![0u8; BUFFER_SIZE];
    let mut response_seq = 0u32;
    
    loop {
        match internet_socket.recv_from(&mut buf).await {
            Ok((n, _src)) => {
                let response = &buf[..n];
                
                // Build response packet with header
                let mut packet = BytesMut::with_capacity(PACKET_HEADER_SIZE + n);
                packet.put_u32(session_id);
                packet.put_u32(response_seq);
                response_seq = response_seq.wrapping_add(1);
                packet.put_slice(response);
                
                // Send to ALL client connections (WiFi + Mobile) for redundancy
                let sessions_read = sessions.read().await;
                if let Some(session) = sessions_read.get(&session_id) {
                    for (client_addr, conn_info) in &session.connections {
                        // Only send to recently active connections
                        if conn_info.last_seen.elapsed().as_secs() < 10 {
                            if let Err(e) = tunnel_socket.send_to(&packet, client_addr).await {
                                debug!("Failed to send response to {}: {}", client_addr, e);
                            } else {
                                debug!("📨 Sent response to {}: {} bytes", client_addr, n);
                            }
                        }
                    }
                } else {
                    debug!("Session {} no longer exists", session_id);
                    break;
                }
            }
            Err(e) => {
                debug!("Internet socket recv error: {}", e);
                break;
            }
        }
    }
}

/// Cleanup expired sessions
async fn cleanup_sessions(sessions: Sessions) {
    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        let mut sessions = sessions.write().await;
        let now = Instant::now();

        sessions.retain(|session_id, session| {
            let should_keep = now.duration_since(session.last_activity) < SESSION_TIMEOUT;
            
            if !should_keep {
                info!("🗑️ Removing expired session {}", session_id);
            }
            
            should_keep
        });

        let active_count = sessions.len();
        if active_count > 0 {
            info!("📊 Active sessions: {}", active_count);
            for (sid, session) in sessions.iter() {
                info!(
                    "  Session {}: {} connections",
                    sid,
                    session.connections.len()
                );
            }
        }
    }
}
