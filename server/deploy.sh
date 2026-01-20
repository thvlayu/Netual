#!/bin/bash
# Netual Server Deployment Script for VPS
# Run this on your VPS (Ubuntu/Debian)

set -e

echo "🚀 Netual Server Deployment Script"
echo "===================================="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "⚠️  Please run as root (or use sudo)"
    exit 1
fi

# Install Rust if not present
if ! command -v cargo &> /dev/null; then
    echo "📦 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    echo "✅ Rust installed"
else
    echo "✅ Rust already installed"
fi

# Create directory
INSTALL_DIR="/opt/netual-server"
echo "📁 Creating directory: $INSTALL_DIR"
mkdir -p $INSTALL_DIR

# Copy files (assumes you've already uploaded them)
if [ -f "Cargo.toml" ]; then
    echo "📋 Copying server files..."
    cp -r * $INSTALL_DIR/
    cd $INSTALL_DIR
else
    echo "⚠️  Run this script from the server directory (where Cargo.toml is)"
    echo "   Or manually upload files to $INSTALL_DIR"
    exit 1
fi

# Build release version
echo "🔨 Building release version (this may take a few minutes)..."
cargo build --release

# Configure firewall
echo "🔥 Configuring firewall..."
ufw allow 9998/tcp comment 'Netual Control'
ufw allow 9999/udp comment 'Netual Tunnel'
ufw --force enable

# Enable IP forwarding
echo "🌐 Enabling IP forwarding..."
sysctl -w net.ipv4.ip_forward=1
echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf

# Configure iptables for NAT
echo "🔧 Configuring NAT..."
INTERFACE=$(ip route | grep default | awk '{print $5}' | head -n1)
iptables -t nat -A POSTROUTING -o $INTERFACE -j MASQUERADE
iptables-save > /etc/iptables/rules.v4 || true

# Create systemd service
echo "⚙️  Creating systemd service..."
cat > /etc/systemd/system/netual.service << EOF
[Unit]
Description=Netual VPN Server
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/target/release/netual-server
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Enable and start service
echo "🎬 Starting service..."
systemctl daemon-reload
systemctl enable netual
systemctl start netual

# Check status
echo ""
echo "✅ Deployment complete!"
echo ""
echo "📊 Service Status:"
systemctl status netual --no-pager | head -n 15

echo ""
echo "📡 Server Info:"
echo "   Control Port: 9998 (TCP)"
echo "   Tunnel Port:  9999 (UDP)"
echo "   Public IP:    $(curl -s ifconfig.me)"
echo ""
echo "📋 Useful Commands:"
echo "   View logs:      sudo journalctl -u netual -f"
echo "   Restart:        sudo systemctl restart netual"
echo "   Stop:           sudo systemctl stop netual"
echo "   Status:         sudo systemctl status netual"
echo ""
echo "🎉 Server is ready! Use the public IP above in your Android app."
