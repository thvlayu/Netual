# Quick Start - GitHub Actions Build (No Local Space Needed!)

## Step 1: Push to GitHub

```bash
cd d:\Github\Netual

# Initialize git if not already done
git init

# Add all files
git add .

# Commit
git commit -m "Initial Netual VPN"

# Create GitHub repo at https://github.com/new
# Name it: Netual

# Add remote and push
git remote add origin https://github.com/YOUR_USERNAME/Netual.git
git branch -M main
git push -u origin main
```

## Step 2: Wait for Build

1. Go to: `https://github.com/YOUR_USERNAME/Netual/actions`
2. You'll see "Build Android APK" workflow running
3. Wait ~5 minutes for it to complete (green checkmark ✅)

## Step 3: Download APK

1. Click on the completed workflow run
2. Scroll down to **Artifacts** section
3. Click **netual-vpn-debug** to download
4. Extract the ZIP file
5. You'll get `app-debug.apk`

## Step 4: Install on Phone

**Option A: With ADB**
```bash
adb install app-debug.apk
```

**Option B: Direct Transfer**
1. Copy APK to your phone (USB/cloud/email)
2. On phone: Open the APK file
3. Allow "Install from unknown sources" if prompted
4. Tap Install

## Step 5: Deploy Server

SSH to your VPS:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Upload server files to VPS (use SCP/SFTP)
# Then:
cd /opt/netual-server
cargo build --release

# Open firewall
sudo ufw allow 9998/tcp
sudo ufw allow 9999/udp

# Run server
sudo ./target/release/netual-server
```

## Step 6: Connect

1. Open Netual VPN app
2. Enter VPS IP (e.g., `203.0.113.45`)
3. Enable **both WiFi AND Mobile Data** on phone
4. Tap **Connect**
5. Grant VPN permission
6. Browse websites!

---

## Verify It's Working

**On VPS:**
```bash
sudo journalctl -u netual -f
```

You should see:
- "Created session X for Y"
- "WiFi socket created"
- "Mobile socket created"
- "Sent packet X via BOTH"

**On Phone:**
```bash
adb logcat | grep NetualVPN
```

You should see both connections active.

---

## Troubleshooting

**Build failed on GitHub?**
- Check Actions tab for error logs
- Make sure all files are committed

**Can't connect?**
- Verify VPS IP is correct
- Check firewall: `sudo ufw status`
- Ensure server is running: `ps aux | grep netual`

**Only one connection working?**
- Check if both WiFi and Mobile Data are truly enabled
- Some Android versions may restrict this
- Check logs: `adb logcat | grep NetualVPN`

---

## What's Next?

Once working, you can improve:
1. Add encryption (WireGuard)
2. Make server auto-start (systemd)
3. Add connection monitoring UI
4. Optimize battery usage

But the core bonding functionality is working NOW! 🚀
