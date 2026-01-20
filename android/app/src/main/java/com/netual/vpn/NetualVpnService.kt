package com.netual.vpn

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.*
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.nio.ByteBuffer
import java.nio.channels.DatagramChannel

class NetualVpnService : VpnService() {

    companion object {
        const val ACTION_CONNECT = "com.netual.vpn.CONNECT"
        const val ACTION_DISCONNECT = "com.netual.vpn.DISCONNECT"
        private const val TAG = "NetualVPN"
        private const val NOTIFICATION_ID = 1
        private const val CHANNEL_ID = "NetualVPN"
    }

    private var vpnInterface: ParcelFileDescriptor? = null
    private var isRunning = false
    private var serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    
    private var serverIp: String = ""
    private val serverPort = 9999
    private val controlPort = 9998
    
    private var sessionId: Int = 0
    private var packetSeq: Int = 0
    
    private var wifiSocket: DatagramChannel? = null
    private var mobileSocket: DatagramChannel? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_CONNECT -> {
                serverIp = intent.getStringExtra("server_ip") ?: ""
                if (serverIp.isNotEmpty()) {
                    startVpn()
                }
            }
            ACTION_DISCONNECT -> {
                stopVpn()
            }
        }
        return START_STICKY
    }

    private fun startVpn() {
        if (isRunning) return

        Log.i(TAG, "Starting VPN...")
        showNotification("Connecting...")

        serviceScope.launch {
            try {
                // Register with server
                sessionId = registerWithServer()
                if (sessionId == 0) {
                    Log.e(TAG, "Failed to register with server")
                    showNotification("Connection failed")
                    return@launch
                }

                Log.i(TAG, "Registered with session ID: $sessionId")

                // Create VPN interface
                val builder = Builder()
                    .setSession("Netual VPN")
                    .addAddress("10.0.0.2", 24)
                    .addRoute("0.0.0.0", 0)
                    .addDnsServer("8.8.8.8")
                    .setMtu(1500)

                vpnInterface = builder.establish()
                isRunning = true

                showNotification("Connected")
                Log.i(TAG, "VPN interface established")

                // Setup network sockets
                setupNetworkSockets()

                // Start packet forwarding
                startPacketForwarding()

            } catch (e: Exception) {
                Log.e(TAG, "Error starting VPN", e)
                showNotification("Error: ${e.message}")
                stopVpn()
            }
        }
    }

    private suspend fun registerWithServer(): Int = withContext(Dispatchers.IO) {
        try {
            val socket = java.net.Socket()
            socket.connect(InetSocketAddress(serverIp, controlPort), 5000)
            
            val output = socket.getOutputStream()
            output.write("REGISTER\n".toByteArray())
            output.flush()

            val input = socket.getInputStream()
            val buffer = ByteArray(1024)
            val n = input.read(buffer)
            val response = String(buffer, 0, n)
            
            socket.close()

            if (response.startsWith("SESSION_ID:")) {
                response.substringAfter(":").trim().toIntOrNull() ?: 0
            } else {
                0
            }
        } catch (e: Exception) {
            Log.e(TAG, "Registration failed", e)
            0
        }
    }

    private fun setupNetworkSockets() {
        try {
            val connectivityManager = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            val networks = connectivityManager.allNetworks

            for (network in networks) {
                val caps = connectivityManager.getNetworkCapabilities(network)
                if (caps == null) continue
                
                when {
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> {
                        wifiSocket = DatagramChannel.open()
                        wifiSocket?.socket()?.bind(null)
                        connectivityManager.bindProcessToNetwork(network)
                        wifiSocket?.connect(InetSocketAddress(serverIp, serverPort))
                        connectivityManager.bindProcessToNetwork(null)
                        Log.i(TAG, "WiFi socket created")
                    }
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> {
                        mobileSocket = DatagramChannel.open()
                        mobileSocket?.socket()?.bind(null)
                        connectivityManager.bindProcessToNetwork(network)
                        mobileSocket?.connect(InetSocketAddress(serverIp, serverPort))
                        connectivityManager.bindProcessToNetwork(null)
                        Log.i(TAG, "Mobile socket created")
                    }
                }
            }

            if (wifiSocket == null && mobileSocket == null) {
                throw Exception("No network sockets available")
            }

        } catch (e: Exception) {
            Log.e(TAG, "Error setting up sockets", e)
            throw e
        }
    }

    private fun startPacketForwarding() {
        val vpnFd = vpnInterface ?: return

        // Read from VPN interface and send to server
        serviceScope.launch {
            val inputStream = FileInputStream(vpnFd.fileDescriptor)
            val buffer = ByteArray(32767)

            while (isRunning) {
                try {
                    val length = inputStream.read(buffer)
                    if (length > 0) {
                        sendPacketToServer(buffer, length)
                    }
                } catch (e: Exception) {
                    if (isRunning) {
                        Log.e(TAG, "Error reading from VPN", e)
                    }
                    // Exit the loop on error
                    return@launch
                }
            }
        }

        // Receive from server and write to VPN interface
        serviceScope.launch {
            receiveFromServer()
        }
    }

    private fun sendPacketToServer(data: ByteArray, length: Int) {
        try {
            // Build packet with header: session_id(4) + packet_seq(4) + payload
            val packet = ByteBuffer.allocate(8 + length)
            packet.putInt(sessionId)
            packet.putInt(packetSeq++)
            packet.put(data, 0, length)
            packet.flip()

            // Try to send on BOTH connections for redundancy (Speedify-style bonding)
            // This ensures packet delivery even if one connection is slow
            var wifiSent = false
            var mobileSent = false
            
            // Send on WiFi
            if (wifiSocket != null) {
                try {
                    val packetCopy = packet.duplicate()
                    wifiSocket?.write(packetCopy)
                    wifiSent = true
                } catch (e: Exception) {
                    Log.d(TAG, "WiFi send failed: ${e.message}")
                }
            }
            
            // Send on Mobile
            if (mobileSocket != null) {
                try {
                    packet.rewind()
                    mobileSocket?.write(packet)
                    mobileSent = true
                } catch (e: Exception) {
                    Log.d(TAG, "Mobile send failed: ${e.message}")
                }
            }

            if (wifiSent || mobileSent) {
                val via = when {
                    wifiSent && mobileSent -> "BOTH"
                    wifiSent -> "WiFi"
                    mobileSent -> "Mobile"
                    else -> "NONE"
                }
                Log.d(TAG, "Sent packet $packetSeq via $via ($length bytes)")
            } else {
                Log.w(TAG, "Failed to send packet $packetSeq - no connections available")
            }

        } catch (e: Exception) {
            Log.e(TAG, "Error sending packet", e)
        }
    }

    private suspend fun receiveFromServer() = withContext(Dispatchers.IO) {
        val vpnFd = vpnInterface ?: return@withContext
        val outputStream = FileOutputStream(vpnFd.fileDescriptor)
        
        // Launch separate receivers for both connections (parallel receiving)
        val wifiJob = launch {
            receiveFromSocket(wifiSocket, "WiFi", outputStream)
        }
        
        val mobileJob = launch {
            receiveFromSocket(mobileSocket, "Mobile", outputStream)
        }
        
        // Wait for both to complete (when service stops)
        wifiJob.join()
        mobileJob.join()
    }
    
    private suspend fun receiveFromSocket(
        socket: DatagramChannel?,
        name: String,
        outputStream: FileOutputStream
    ) = withContext(Dispatchers.IO) {
        if (socket == null) return@withContext
        
        val buffer = ByteBuffer.allocate(32767)
        val seenPackets = mutableSetOf<Int>() // Deduplication
        
        try {
            socket.configureBlocking(false)
            
            while (isRunning) {
                try {
                    buffer.clear()
                    val length = socket.read(buffer)
                    
                    if (length > 8) {
                        buffer.flip()
                        val receivedSessionId = buffer.getInt()
                        val receivedSeq = buffer.getInt()
                        
                        if (receivedSessionId == sessionId) {
                            // Deduplicate packets (server sends on both paths)
                            val isDuplicate = synchronized(seenPackets) {
                                if (seenPackets.contains(receivedSeq)) {
                                    true
                                } else {
                                    seenPackets.add(receivedSeq)
                                    
                                    // Keep set size manageable
                                    if (seenPackets.size > 1000) {
                                        seenPackets.clear()
                                    }
                                    false
                                }
                            }
                            
                            if (!isDuplicate) {
                                val payload = ByteArray(length - 8)
                                buffer.get(payload)
                                
                                outputStream.write(payload)
                                Log.d(TAG, "[$name] Received packet $receivedSeq: ${length - 8} bytes")
                            } else {
                                Log.d(TAG, "[$name] Duplicate packet $receivedSeq ignored")
                            }
                        }
                    } else if (length == 0) {
                        // Non-blocking read with no data, sleep briefly
                        delay(1)
                    }
                    
                } catch (e: Exception) {
                    if (isRunning) {
                        Log.d(TAG, "[$name] Receive error: ${e.message}")
                        delay(10)
                    }
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "[$name] Socket error", e)
        }
    }

    private fun stopVpn() {
        Log.i(TAG, "Stopping VPN...")
        isRunning = false
        
        serviceScope.cancel()
        serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())
        
        wifiSocket?.close()
        mobileSocket?.close()
        wifiSocket = null
        mobileSocket = null
        
        vpnInterface?.close()
        vpnInterface = null
        
        stopForeground(true)
        stopSelf()
    }

    private fun showNotification(message: String) {
        val notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Netual VPN",
                NotificationManager.IMPORTANCE_LOW
            )
            notificationManager.createNotificationChannel(channel)
        }

        val intent = Intent(this, MainActivity::class.java)
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Netual VPN")
            .setContentText(message)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .build()

        startForeground(NOTIFICATION_ID, notification)
    }

    override fun onDestroy() {
        super.onDestroy()
        stopVpn()
    }
}
