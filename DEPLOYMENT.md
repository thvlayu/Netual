# Netual VPN - Deployment Guide

## ⚡ Channel Bonding Technology

Netual is a **Speedify-style VPN** that uses **channel bonding** to combine WiFi and mobile data for:
- ✅ **Faster speeds** - Uses both connections simultaneously
- ✅ **Better reliability** - Automatic failover if one connection drops
- ✅ **Seamless handover** - Switch between networks without disconnection

### How It Works

```
Android App                    VPN Server                  Internet
-----------                    ----------                  --------
   WiFi    ─┐                     ┌──> TUN Device ──>  Google.com
             ├──> [Bonding]  ─────┤                     YouTube.com
   Mobile  ─┘                     └──> Routes to        Websites
                                       real internet
```

1. **Android sends every packet on BOTH WiFi and Mobile**
2. **Server deduplicates** and forwards to internet via TUN device
3. **Server sends responses on BOTH connections** back to Android
4. **Android deduplicates** responses and delivers to apps

Result: **Faster downloads, lower latency, seamless failover!**

---

## 🔧 Server Setup (Must Run on Linux)

### Why Linux Only?
The TUN device requires Linux kernel support. Windows/macOS don't work with this implementation.

### Requirements
- Ubuntu 20.04+ or Debian 11+ (or any modern Linux)
- Root access (for TUN device and iptables)
- Rust 1.70+ installed

### Installation

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 2. Clone repository
cd /opt
git clone https://github.com/yourusername/Netual.git
cd Netual/server

# 3. Build server
cargo build --release

# 4. Configure Linux networking
# Enable IP forwarding
sudo sysctl -w net.ipv4.ip_forward=1
sudo sh -c 'echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf'

# Setup NAT/masquerading for VPN traffic
sudo iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -j MASQUERADE
sudo iptables -A FORWARD -s 10.0.0.0/24 -j ACCEPT

# Save iptables rules
sudo apt-get install iptables-persistent
sudo netfilter-persistent save

# 5. Run server (requires root for TUN device)
sudo ./target/release/netual-server
```

### Server Ports
- **TCP 9998** - Control connection (registration)
- **UDP 9999** - Data tunnel (actual VPN traffic)

### Firewall Configuration

```bash
# UFW (Ubuntu firewall)
sudo ufw allow 9998/tcp
sudo ufw allow 9999/udp
sudo ufw enable

# Or iptables directly
sudo iptables -A INPUT -p tcp --dport 9998 -j ACCEPT
sudo iptables -A INPUT -p udp --dport 9999 -j ACCEPT
```

### Running as System Service

Create `/etc/systemd/system/netual-vpn.service`:

```ini
[Unit]
Description=Netual VPN Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/Netual/server
ExecStart=/opt/Netual/server/target/release/netual-server
Restart=always
RestartSec=10
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

Then:
```bash
sudo systemctl daemon-reload
sudo systemctl enable netual-vpn
sudo systemctl start netual-vpn
sudo systemctl status netual-vpn
```

---

## 📱 Android App Setup

### Building the APK

```bash
cd android
gradle assembleDebug

# APK will be at:
# android/app/build/outputs/apk/debug/app-debug.apk
```

### Installing on Device

```bash
adb install app/build/outputs/apk/debug/app-debug.apk
```

Or use GitHub Actions - it builds automatically on every push!

### Using the App

1. **Open Netual VPN** app
2. **Enter your server IP** (e.g., `your-vps-ip.com` or `123.45.67.89`)
3. **Click Connect**
4. **Grant VPN permission** when Android asks
5. **App will connect using BOTH WiFi and Mobile!**

### Checking Connection

```bash
# On server, watch logs
sudo journalctl -u netual-vpn -f

# You should see:
# ✅ Created session XXXXX
# 📦 Packet from X.X.X.X:YYYY (WiFi connection)
# 📦 Packet from X.X.X.X:ZZZZ (Mobile connection)
# 📊 Active sessions: 1
#   Session XXXXX: 2 connections  <-- This means bonding works!
```

---

## 🐛 Troubleshooting

### App Crashes on Connect

**Symptom:** App requests VPN permission but crashes immediately

**Likely Causes:**
1. Server not reachable
2. Firewall blocking ports 9998/9999
3. Server not running with root privileges (needs TUN access)

**Fix:**
```bash
# Check server is running
sudo systemctl status netual-vpn

# Check firewall
sudo ufw status
sudo iptables -L

# Test connectivity from phone
# Use app like "Network Tools" to ping server IP
```

### Only One Connection Shows on Server

**Symptom:** Server logs show only 1 connection instead of 2

**Likely Causes:**
1. Phone doesn't have both WiFi and Mobile data enabled
2. One network isn't working properly
3. Android's network routing is blocking

**Fix:**
```bash
# On Android:
# - Enable WiFi
# - Enable Mobile Data
# - Disable "WiFi only" mode
# - Check both connections work in browser
```

### No Internet Access While Connected

**Symptom:** VPN connects but can't browse internet

**Likely Causes:**
1. Server NAT/masquerading not configured
2. IP forwarding disabled
3. iptables rules missing

**Fix:**
```bash
# On server, reapply networking config
sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -j MASQUERADE
sudo iptables -A FORWARD -s 10.0.0.0/24 -j ACCEPT
```

### Slow Speed or High Latency

**Symptom:** VPN works but slower than expected

**Likely Causes:**
1. Server has poor internet connection
2. High RTT to server location
3. Server CPU overloaded

**Solutions:**
- Use a VPS with better connectivity (e.g., near your location)
- Use a VPS with more CPU/RAM
- Check server logs for errors

---

## 📊 Performance Testing

### Test Bonding is Working

```bash
# On phone, while connected:
# 1. Start downloading a large file
# 2. Disable WiFi - download continues!
# 3. Re-enable WiFi - speed increases!
# 4. Disable Mobile - download continues!

# This proves channel bonding works!
```

### Measure Speed Improvement

```bash
# Test 1: Normal connection (no VPN)
# Use speedtest.net app

# Test 2: WiFi only (disable mobile data)
# Connect to VPN, use speedtest.net

# Test 3: Mobile only (disable WiFi)
# Connect to VPN, use speedtest.net

# Test 4: Both networks (BONDING!)
# Enable both WiFi and Mobile, connect VPN
# Speed should be close to WiFi + Mobile combined!
```

---

## 🔒 Security Notes

**⚠️ This is a basic implementation for testing channel bonding technology.**

For production use, you should add:
- ✅ Encryption (currently packets are unencrypted)
- ✅ Authentication (currently anyone can register)
- ✅ Rate limiting
- ✅ DDoS protection
- ✅ Logging and monitoring

---

## 🚀 Next Steps

1. **Deploy server** on a Linux VPS
2. **Build and install** Android app
3. **Test bonding** by using both WiFi and Mobile
4. **Monitor performance** and logs
5. **Report issues** on GitHub

---

## 📝 Architecture Details

### Packet Flow: Upload (Phone -> Internet)

```
1. App captures IP packet (e.g., HTTP request to google.com)
2. Adds header: [SessionID(4) + SeqNum(4) + IPPacket]
3. Sends on WiFi socket to server:9999
4. Sends on Mobile socket to server:9999 (SAME packet!)
5. Server receives both, deduplicates by SeqNum
6. Server writes IP packet to TUN device
7. Linux routes packet to google.com
```

### Packet Flow: Download (Internet -> Phone)

```
1. Server TUN device receives response from google.com
2. Reads IP packet from TUN
3. Adds header: [SessionID(4) + SeqNum(4) + IPPacket]
4. Sends to BOTH client addresses (WiFi + Mobile)
5. Android receives on both sockets
6. Deduplicates by SeqNum
7. Writes to VPN interface
8. App receives response
```

### Why This Is Fast

- **Redundancy:** Both networks carry same data, whichever arrives first wins
- **Failover:** If WiFi packet lost, Mobile packet still gets through
- **Load balancing:** OS can split TCP connections across both networks
- **Seamless:** Switching networks doesn't break connections

---

## 📄 License

MIT License - Feel free to use, modify, and distribute!
