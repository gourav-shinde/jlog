#!/usr/bin/env bash
set -e

INSTALL_DIR="/opt/jlog"
BINARY_NAME="jlog"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[jlog]${NC} $1"; }
warn()  { echo -e "${YELLOW}[jlog]${NC} $1"; }
error() { echo -e "${RED}[jlog]${NC} $1"; exit 1; }

if [ "$EUID" -ne 0 ]; then
    error "Please run as root: sudo ./uninstall.sh"
fi

# Remove install dir
if [ -d "$INSTALL_DIR" ]; then
    info "Removing $INSTALL_DIR..."
    rm -rf "$INSTALL_DIR"
else
    warn "$INSTALL_DIR not found — skipping"
fi

# Remove symlink
if [ -L "/usr/local/bin/$BINARY_NAME" ]; then
    info "Removing /usr/local/bin/$BINARY_NAME symlink..."
    rm -f "/usr/local/bin/$BINARY_NAME"
else
    warn "/usr/local/bin/$BINARY_NAME symlink not found — skipping"
fi

# Remove PATH export from shell rc
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

if [ -f "$SHELL_RC" ] && grep -qF "$INSTALL_DIR" "$SHELL_RC"; then
    info "Removing PATH entry from $SHELL_RC..."
    # Remove the jlog comment line and the export line
    sed -i "/# jlog/d" "$SHELL_RC"
    sed -i "\|$INSTALL_DIR|d" "$SHELL_RC"
else
    warn "No PATH entry found in $SHELL_RC — skipping"
fi

info "jlog uninstalled successfully."
echo ""
echo -e "  Config and profiles in ${YELLOW}~/.config/jlog/${NC} were left intact."
echo -e "  Remove manually if no longer needed: ${YELLOW}rm -rf ~/.config/jlog${NC}"
