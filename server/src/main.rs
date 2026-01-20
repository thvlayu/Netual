use anyhow::Result;
use bytes::{Buf, BufMut, BytesMut};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::RwLock;
use tokio::time::interval;
use tun::platform::Device;

const BUFFER_SIZE: usize = 65536;
const SESSION_TIMEOUT: Duration = Duration::from_secs(120);
const PACKET_HEADER_SIZE: usize = 8; // session_id(4) + packet_seq(4)

/// Represents a client session with multiple connections (WiFi + Mobile)
#[derive(Debug)]
struct ClientSession {
    session_id: u32,
    connections: HashMap<SocketAddr, ConnectionInfo>,
    packet_buffer: HashMap<u32, Vec<u8>>, // packet_seq -> packet data (for dedup)
    next_expected_seq: u32,
    last_activity: Instant,
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

    info!("🚀 Netual VPN Server starting...");

    // Create TUN device for routing packets to internet
    let tun_device = create_tun_device()?;
    let tun_fd = tun_device.as_raw_fd();
    let tun_device = Arc::new(tokio::io::unix::AsyncFd::new(tun_device)?);
    info!("✅ TUN device created and configured (fd: {})", tun_fd);

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

    // Spawn TUN reader - reads from internet and sends back to clients
    let tun_reader = tun_device.clone();
    let tunnel_sock_reader = tunnel_socket.clone();
    let sessions_reader = sessions.clone();
    tokio::spawn(handle_tun_to_client(tun_reader, tunnel_sock_reader, sessions_reader));

    // Main tunnel packet handler - reads from clients and writes to TUN
    handle_client_to_tun(tunnel_socket, sessions, tun_device).await?;

    Ok(())
}

/// Create and configure TUN device
fn create_tun_device() -> Result<Device> {
    let mut config = tun::Configuration::default();
    config
        .name("netual0")
        .address((10, 0, 0, 1))
        .netmask((255, 255, 255, 0))
        .up();

    #[cfg(target_os = "linux")]
    config.platform(|config| {
        config.packet_information(false);
    });

    let dev = tun::create(&config)?;
    
    info!("🌐 TUN device 'netual0' created with IP 10.0.0.1/24");
    info!("   Run on Linux: sudo iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -j MASQUERADE");
    info!("   Run on Linux: sudo sysctl -w net.ipv4.ip_forward=1");
    
    Ok(dev)
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

/// Main handler: Client -> TUN (client sends packet, we write to TUN/internet)
async fn handle_client_to_tun(
    socket: Arc<UdpSocket>,
    sessions: Sessions,
    tun_device: Arc<tokio::io::unix::AsyncFd<Device>>,
) -> Result<()> {
    let mut buf = vec![0u8; BUFFER_SIZE];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((n, src_addr)) => {
                let data = &buf[..n];
                let sessions_clone = sessions.clone();
                let tun_clone = tun_device.clone();
                let data_owned = data.to_vec();

                tokio::spawn(async move {
                    if let Err(e) = process_client_packet(
                        data_owned,
                        src_addr,
                        sessions_clone,
                        tun_clone,
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

/// Process packet from client and write to TUN device
async fn process_client_packet(
    data: Vec<u8>,
    src_addr: SocketAddr,
    sessions: Sessions,
    tun_device: Arc<tokio::io::unix::AsyncFd<Device>>,
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

    // Update connection info (track both WiFi and Mobile)
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

    // Deduplicate packets (client sends on BOTH WiFi and Mobile)
    if session.packet_buffer.contains_key(&packet_seq) {
        debug!("🔄 Duplicate packet {} ignored (already processed)", packet_seq);
        return Ok(());
    }

    // Store packet for deduplication
    session.packet_buffer.insert(packet_seq, payload.to_vec());
    
    // Keep buffer size manageable
    if session.packet_buffer.len() > 100 {
        let min_seq = packet_seq.saturating_sub(100);
        session.packet_buffer.retain(|seq, _| *seq > min_seq);
    }

    // Write IP packet to TUN device (forwards to internet via Linux routing)
    if payload.len() > 20 {
        drop(sessions); // Release lock before async write
        
        let tun_clone = tun_device.clone();
        let payload_vec = payload.to_vec();
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let mut guard = tun_clone.get_ref();
            if let Err(e) = guard.write_all(&payload_vec) {
                debug!("Failed to write to TUN: {}", e);
            } else {
                debug!("✅ Wrote packet seq {} to TUN device ({} bytes)", packet_seq, payload_vec.len());
            }
        });
    }

    Ok(())
}

/// TUN -> Client handler: Reads from TUN and sends back to clients
async fn handle_tun_to_client(
    tun_device: Arc<tokio::io::unix::AsyncFd<Device>>,
    tunnel_socket: Arc<UdpSocket>,
    sessions: Sessions,
) {
    let mut response_seq_map: HashMap<u32, u32> = HashMap::new();
    
    loop {
        let mut buf = vec![0u8; BUFFER_SIZE];
        let tun_clone = tun_device.clone();
        
        let n = match tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let mut guard = tun_clone.get_ref();
            guard.read(&mut buf).map(|n| (n, buf))
        }).await {
            Ok(Ok((n, buf))) => {
                if n > 0 {
                if n < 20 {
                    continue; // Too small for IP packet
                }
                
                let ip_packet = &buf[..n];
                
                // Extract destination IP from IP header (bytes 16-19 for IPv4)
                // Simplified: we need to identify which session this response belongs to
                // For now, use source IP (bytes 12-15) to match to client's VPN IP
                
                let dest_ip = if n >= 20 && (ip_packet[0] >> 4) == 4 {
                    // IPv4: destination IP is at bytes 16-19
                    [ip_packet[16], ip_packet[17], ip_packet[18], ip_packet[19]]
                } else {
                    continue; // Skip non-IPv4 for now
                };
                
                // Match dest IP 10.0.0.X to session
                // Client uses 10.0.0.2, so we need to find session by tracking
                // For simplicity, broadcast to all active sessions (or implement proper routing)
                
                let sessions_read = sessions.read().await;
                
                for (session_id, session) in sessions_read.iter() {
                    // Get or init response sequence for this session
                    let response_seq = response_seq_map.entry(*session_id).or_insert(0);
                    
                    // Build packet with header
                    let mut packet = BytesMut::with_capacity(PACKET_HEADER_SIZE + n);
                    packet.put_u32(*session_id);
                    packet.put_u32(*response_seq);
                    *response_seq = response_seq.wrapping_add(1);
                    packet.put_slice(ip_packet);
                    
                    // Send to ALL active client connections (WiFi + Mobile)
                    for (client_addr, conn_info) in &session.connections {
                        if conn_info.last_seen.elapsed().as_secs() < 10 {
                            if let Err(e) = tunnel_socket.send_to(&packet, client_addr).await {
                                debug!("Failed to send to {}: {}", client_addr, e);
                            } else {
                                debug!("📨 Sent {} bytes to {}", n, client_addr);
                            }
                        }
                    }
                }
                buf
                } else {
                    continue;
                }
            }
            Ok(Err(e)) => {
                error!("TUN read error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                error!("TUN task error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
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
                    "  Session {}: {} connections, {} packets buffered",
                    sid,
                    session.connections.len(),
                    session.packet_buffer.len()
                );
            }
        }
    }
}
