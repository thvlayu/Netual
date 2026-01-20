# Netual VPN - WiFi + Mobile Data Combiner

A bare-bones VPN that combines WiFi and mobile data connections for better reliability. Routes traffic through your VPS server using both networks simultaneously.

## 🎯 What It Does

- Combines WiFi + Mobile Data at packet level
- Routes traffic through your VPS (relay server)
- Simple round-robin distribution between connections
- No fancy UI, just functional

## 📋 Prerequisites

**VPS Server:**
- Ubuntu/Debian Linux
- Rust installed
- Public IP address
- Ports 9998 and 9999 open

**Android Device:**
- Android 5.0+ (API 21+)
- Both WiFi and Mobile Data enabled
- Command-line tools (no Android Studio needed)

**Your Computer (for building):**
- Java JDK 17+
- Android SDK command-line tools

---

## 🚀 Setup Instructions

### Method 1: Build with GitHub Actions (RECOMMENDED - No local space needed!)

**This builds the APK in the cloud for FREE:**

1. **Push code to GitHub:**
   ```bash
   cd d:\Github\Netual
   git init
   git add .
   git commit -m "Initial commit"
   git remote add origin https://github.com/YOUR_USERNAME/Netual.git
   git push -u origin main
   ```

2. **GitHub automatically builds APK:**
   - Go to your repository on GitHub
   - Click "Actions" tab
   - Wait for build to complete (~5 minutes)

3. **Download APK:**
   - Click on the completed workflow
   - Scroll to "Artifacts" section
   - Download "netual-vpn-debug"
   - Extract ZIP to get `app-debug.apk`

4. **Install on phone:**
   ```bash
   adb install app-debug.apk
   ```

**That's it! Zero space used on your laptop.**

---

### Method 2: Build Locally (If you have space)

SSH into your VPS:

```bash
# Install Rust if not already installed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone/upload the server code
cd /opt
# Upload the 'server' directory to /opt/netual-server

cd /opt/netual-server

# Build release version
cargo build --release

# Allow traffic on required ports
sudo ufw allow 9998/tcp
sudo ufw allow 9999/udp

# Run the server
sudo ./target/release/netual-server
```

**Server will start listening on:**
- TCP 9998 (control/registration)
- UDP 9999 (tunnel packets)

**Make it run on startup (systemd):**

```bash
sudo nano /etc/systemd/system/netual.service
```

Add:
```ini
[Unit]
Description=Netual VPN Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/netual-server
ExecStart=/opt/netual-server/target/release/netual-server
Restart=always

[Install]
WantedBy=multi-user.target
```

Then:
```bash
sudo systemctl enable netual
sudo systemctl start netual
sudo systemctl status netual
```

---

---

### 1. Deploy Server on VPS

**On Windows:**

```powershell
# Install Java JDK 17+ if not installed
# Download from: https://adoptium.net/

# Download Android command-line tools
# https://developer.android.com/studio#command-line-tools-only

# Extract to C:\Android\cmdline-tools\latest

# Set environment variables
$env:ANDROID_HOME = "C:\Android"
$env:PATH += ";C:\Android\cmdline-tools\latest\bin;C:\Android\platform-tools"

# Install required SDK components
sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0"

# Accept licenses
sdkmanager --licenses

# Navigate to android directory
cd d:\Github\Netual\android

# Build debug APK
.\gradlew assembleDebug

# APK will be at: app\build\outputs\apk\debug\app-debug.apk
```

**On Linux/Mac:**

```bash
# Install Java JDK 17+
# Ubuntu: sudo apt install openjdk-17-jdk
# Mac: brew install openjdk@17

# Download Android command-line tools
# Extract to ~/Android/cmdline-tools/latest

export ANDROID_HOME=~/Android
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0"
sdkmanager --licenses

cd android
./gradlew assembleDebug

# APK at: app/build/outputs/apk/debug/app-debug.apk
```

---

### 3. Install on Android

```bash
# Enable USB debugging on your phone:
# Settings > About Phone > Tap "Build Number" 7 times
# Settings > Developer Options > Enable USB Debugging

# Connect phone via USB

# Install APK
adb install app/build/outputs/apk/debug/app-debug.apk

# Check logs (helpful for debugging)
adb logcat | grep NetualVPN
```

---

### 4. Configure and Connect

1. Open Netual VPN app on your phone
2. Enter your VPS IP address (e.g., `203.0.113.45`)
3. Make sure BOTH WiFi and Mobile Data are enabled
4. Tap "Connect"
5. Grant VPN permission when prompted

**Check if it's working:**
- On phone: Browse websites
- On VPS: `sudo journalctl -u netual -f` (see logs)

---

## 🎯 How It Works - Speedify-Style Channel Bonding

```
Your Phone
├─ WiFi Connection
│  └─ Socket 1 → VPS:9999 (UDP)
│     └─ Sends EVERY packet
│
├─ Mobile Data
│  └─ Socket 2 → VPS:9999 (UDP)
│     └─ Sends EVERY packet (duplicate)
│
└─ VPN Interface (tun0)
   └─ Intercepts ALL traffic

VPS Server
├─ Receives SAME packet from BOTH connections
├─ Deduplicates (keeps first arrival, drops duplicate)
├─ Forwards to internet
└─ Sends response to BOTH connections (redundancy)
```

**Key Benefits:**
- **Redundancy**: If WiFi packet is delayed, Mobile delivers it
- **Speed**: Whichever connection is faster delivers first
- **Reliability**: If one connection fails, other keeps working
- **Seamless**: Automatic failover, no interruption

**Unlike simple round-robin:**
- ❌ Round-robin: WiFi packet 1, Mobile packet 2, WiFi packet 3...
- ✅ Channel bonding: BOTH send packet 1, BOTH send packet 2, BOTH send packet 3...
- Server uses whichever arrives first!

This is how Speedify achieves "seamless" bonding.

**Packet Format:**
```
[Session ID: 4 bytes][Packet Seq: 4 bytes][Payload: IP packet]
```

---

## 🐛 Troubleshooting

**"Connection failed"**
- Check VPS IP is correct
- Verify firewall allows ports 9998, 9999
- Check server is running: `systemctl status netual`

**"No internet after connecting"**
- Make sure both WiFi AND Mobile Data are enabled
- Check phone Settings > Network > ensure both active

**Only one connection working**
- Android might not route both simultaneously
- Try disabling battery optimization for Netual app
- Check logs: `adb logcat | grep NetualVPN`

**Server crashes/stops**
- Check logs: `sudo journalctl -u netual -n 100`
- Might need to configure iptables for forwarding:
  ```bash
  sudo sysctl -w net.ipv4.ip_forward=1
  sudo iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
  ```

---

## ⚠️ Current Limitations

This is a **functional implementation** with Speedify-style bonding:

1. **✅ Channel Bonding** - Sends packets on both connections for redundancy
2. **✅ Deduplication** - Server removes duplicate packets
3. **✅ Auto-failover** - If one connection dies, other continues
4. **No encryption** - Packets are in plaintext (add WireGuard later)
5. **Basic IP forwarding** - Simplified packet forwarding (works for most traffic)
6. **No DNS optimization** - Uses basic DNS forwarding

**Production improvements for later:**
- Add proper encryption (ChaCha20-Poly1305)
- Full IP packet parsing and NAT
- Latency-based intelligent path selection
- Connection quality monitoring
- Better battery optimization

---

## 📊 Testing

**Speed test:**
```bash
# On phone with VPN connected
# Run speed test app
# Compare with/without VPN
```

**Verify both connections used:**
```bash
# On VPS server, watch logs
sudo journalctl -u netual -f

# You should see messages about both connections receiving packets
```

---

## 🔒 Security Notes

- This version has **NO ENCRYPTION**
- Your ISP can see all traffic
- Only use on trusted networks or add encryption layer
- Consider wrapping in WireGuard for production

---

## 📝 Next Steps (Future Improvements)

1. Add WireGuard encryption
2. Implement packet reordering queue
3. Smart load balancing (latency-based)
4. Connection quality metrics
5. Automatic failover
6. Better battery optimization
7. Connection status UI

---

## 🤝 Contributing

This is a minimal implementation. Feel free to:
- Add encryption
- Improve packet ordering
- Add better error handling
- Optimize performance

---

## License

MIT - Use at your own risk. No warranties.
