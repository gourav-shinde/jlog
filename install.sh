#!/usr/bin/env bash
set -e

INSTALL_DIR="/opt/jlog"
BINARY_NAME="jlog"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()    { echo -e "${GREEN}[jlog]${NC} $1"; }
warn()    { echo -e "${YELLOW}[jlog]${NC} $1"; }
error()   { echo -e "${RED}[jlog]${NC} $1"; exit 1; }

# Check for root
if [ "$EUID" -ne 0 ]; then
    error "Please run as root: sudo ./install.sh"
fi

# Check for cargo
if ! command -v cargo &>/dev/null; then
    error "cargo not found. Install Rust from https://rustup.rs"
fi

info "Building $BINARY_NAME (release)..."
sudo -u "${SUDO_USER:-$USER}" bash -c "cd '$REPO_DIR' && cargo build --release"

BINARY_PATH="$REPO_DIR/target/release/$BINARY_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    error "Build failed — binary not found at $BINARY_PATH"
fi

info "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

# Detect the real user's shell config
REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(eval echo "~$REAL_USER")

detect_shell_rc() {
    local shell
    shell=$(getent passwd "$REAL_USER" | cut -d: -f7)
    case "$shell" in
        */zsh)  echo "$REAL_HOME/.zshrc" ;;
        */fish) echo "$REAL_HOME/.config/fish/config.fish" ;;
        *)      echo "$REAL_HOME/.bashrc" ;;
    esac
}

SHELL_RC=$(detect_shell_rc)
EXPORT_LINE="export PATH=\"$INSTALL_DIR:\$PATH\""

if grep -qF "$INSTALL_DIR" "$SHELL_RC" 2>/dev/null; then
    warn "$INSTALL_DIR already in $SHELL_RC — skipping PATH update"
else
    info "Adding $INSTALL_DIR to PATH in $SHELL_RC..."
    echo "" >> "$SHELL_RC"
    echo "# jlog" >> "$SHELL_RC"
    echo "$EXPORT_LINE" >> "$SHELL_RC"
fi

# Also symlink to /usr/local/bin so it works immediately in the current session
ln -sf "$INSTALL_DIR/$BINARY_NAME" /usr/local/bin/$BINARY_NAME

info "Done! jlog $("$INSTALL_DIR/$BINARY_NAME" --help 2>&1 | head -1 || true)"
echo ""
echo -e "  Binary   : ${GREEN}$INSTALL_DIR/$BINARY_NAME${NC}"
echo -e "  Symlink  : ${GREEN}/usr/local/bin/$BINARY_NAME${NC}"
echo -e "  Shell RC : ${GREEN}$SHELL_RC${NC}"
echo ""
info "Run 'jlog -h' to get started. Restart your shell or run: source $SHELL_RC"
