# How to Sync Code Between Windows and Linux

## 📤 Step 1: Push Changes from Windows to GitHub

```powershell
# In PowerShell on Windows (D:\Github\Netual)
cd D:\Github\Netual

# Check what changed
git status

# Add all changes
git add .

# Commit with message
git commit -m "Fix server TUN device implementation for Linux compatibility"

# Push to GitHub
git push origin main
```

If you haven't set up the GitHub repo yet:

```powershell
# First time only - create repo on GitHub, then:
git init
git add .
git commit -m "Initial commit - Netual VPN with channel bonding"
git branch -M main
git remote add origin https://github.com/YOUR_USERNAME/Netual.git
git push -u origin main
```

---

## 📥 Step 2: Pull Changes on Linux Server

```bash
# SSH into your Linux server
ssh username@your-server-ip

# If repo doesn't exist yet, clone it
cd ~
git clone https://github.com/YOUR_USERNAME/Netual.git
cd Netual/server

# If repo already exists, pull latest changes
cd ~/Netual
git pull origin main
cd server
```

---

## 🔨 Step 3: Build on Linux

```bash
# Make sure you're in the server directory
cd ~/Netual/server

# Build release version (optimized)
cargo build --release

# Or build debug version (faster compile, for testing)
cargo build

# Check for errors
cargo check
```

---

## 🚀 Step 4: Run or Restart Server

```bash
# If running manually
sudo pkill netual-server  # Stop old version
sudo ./target/release/netual-server

# If using systemd service
sudo systemctl restart netual-vpn
sudo systemctl status netual-vpn

# View logs
sudo journalctl -u netual-vpn -f
```

---

## 🔄 Quick Reference: Full Workflow

### On Windows (after making changes):
```powershell
cd D:\Github\Netual
git add .
git commit -m "Description of changes"
git push origin main
```

### On Linux (to get latest):
```bash
cd ~/Netual
git pull origin main
cd server
cargo build --release
sudo systemctl restart netual-vpn
```

---

## 🐛 Troubleshooting

### "Permission denied" when pulling
```bash
# Make sure you own the directory
sudo chown -R $USER:$USER ~/Netual
```

### "Authentication failed" when pushing from Windows
```powershell
# Use GitHub personal access token instead of password
# Create token at: https://github.com/settings/tokens
# Use token as password when prompted
```

### Build fails on Linux with "permission denied"
```bash
# Clean and rebuild
cd ~/Netual/server
cargo clean
cargo build --release
```

### Changes not showing up after pull
```bash
# Check you're on the right branch
git branch
git status

# Force pull (careful - overwrites local changes!)
git fetch origin
git reset --hard origin/main
```

---

## 📝 Git Best Practices

### Before making changes on Windows:
```powershell
# Always pull latest first
git pull origin main
```

### After making changes:
```powershell
# Stage specific files
git add server/src/main.rs
git add server/Cargo.toml

# Or stage everything
git add .

# Commit with descriptive message
git commit -m "Fix TUN device async wrapper for tun 0.6 compatibility"

# Push to GitHub
git push origin main
```

### View commit history:
```bash
git log --oneline -10  # Last 10 commits
git log --graph --all  # Pretty graph view
```

---

## ⚡ Pro Tips

**Use .gitignore** - Already included, but important files ignored:
- `target/` - Rust build artifacts (huge, don't commit!)
- `*.apk` - Built Android apps
- `*.so` - Native libraries
- `Cargo.lock` - Let it be generated per system

**Compile times:**
- First build on Linux: 5-10 minutes (downloads dependencies)
- Subsequent builds: 30 seconds - 2 minutes
- Use `cargo build` (debug) for faster iteration during development
- Use `cargo build --release` for production deployment

**Multiple machines:**
- Always `git pull` before starting work
- Commit often with clear messages
- Push frequently to keep in sync

---

## 🎯 Current Status Check

On **Windows** right now, you should:

```powershell
cd D:\Github\Netual
git status
```

Then commit and push the latest server fixes!
