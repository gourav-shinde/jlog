#!/usr/bin/env bash
set -e

INSTALL_DIR="/opt/jlog"
BINARY_NAME="jlog"
REPO="gourav-shinde/jlog"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[jlog]${NC} $1"; }
warn()  { echo -e "${YELLOW}[jlog]${NC} $1"; }
error() { echo -e "${RED}[jlog]${NC} $1"; exit 1; }

if [ "$EUID" -ne 0 ]; then
    error "Please run as root: sudo ./install.sh"
fi

BINARY_PATH=""

# --- Try downloading a pre-built binary from GitHub Releases ---
try_download() {
    if ! command -v curl &>/dev/null && ! command -v wget &>/dev/null; then
        warn "curl/wget not found — skipping download"
        return 1
    fi

    local api_url="https://api.github.com/repos/$REPO/releases/latest"
    local release_json

    info "Checking GitHub releases..."
    if command -v curl &>/dev/null; then
        release_json=$(curl -fsSL "$api_url" 2>/dev/null) || { warn "Could not reach GitHub"; return 1; }
    else
        release_json=$(wget -qO- "$api_url" 2>/dev/null) || { warn "Could not reach GitHub"; return 1; }
    fi

    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  arch="x86_64" ;;
        aarch64) arch="aarch64" ;;
        *) warn "No pre-built binary for arch $arch"; return 1 ;;
    esac

    local asset_name="jlog-linux-$arch"
    local download_url
    download_url=$(echo "$release_json" | grep -o "\"browser_download_url\": *\"[^\"]*$asset_name\"" | grep -o 'https://[^"]*') || true

    if [ -z "$download_url" ]; then
        warn "No pre-built binary found in latest release"
        return 1
    fi

    local tmp
    tmp=$(mktemp)
    info "Downloading $asset_name..."
    if command -v curl &>/dev/null; then
        curl -fsSL "$download_url" -o "$tmp"
    else
        wget -qO "$tmp" "$download_url"
    fi

    chmod +x "$tmp"
    BINARY_PATH="$tmp"
    info "Downloaded pre-built binary"
}

# --- Fall back to building from source ---
build_from_source() {
    if ! command -v cargo &>/dev/null; then
        error "No pre-built binary available and cargo not found.\nInstall Rust from https://rustup.rs or download a binary from:\nhttps://github.com/$REPO/releases"
    fi

    info "Building from source (release)..."
    sudo -u "${SUDO_USER:-$USER}" bash -c "cd '$REPO_DIR' && cargo build --release"

    BINARY_PATH="$REPO_DIR/target/release/$BINARY_NAME"
    if [ ! -f "$BINARY_PATH" ]; then
        error "Build failed — binary not found at $BINARY_PATH"
    fi
    info "Built from source"
}

try_download || build_from_source

# --- Install ---
info "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME.bin"
chmod +x "$INSTALL_DIR/$BINARY_NAME.bin"

# Wrapper script to suppress benign WSL2/X11 teardown noise
cat > "$INSTALL_DIR/$BINARY_NAME" <<'WRAPPER'
#!/usr/bin/env bash
# Filter out benign WSL2/WSLg X11 escape code messages printed on window close.
exec "$INSTALL_DIR_PLACEHOLDER/jlog.bin" "$@" 2> >(grep -v "Dropped Escape call" >&2)
WRAPPER
sed -i "s|INSTALL_DIR_PLACEHOLDER|$INSTALL_DIR|g" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

# --- Update shell rc ---
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

# Symlink for immediate use
ln -sf "$INSTALL_DIR/$BINARY_NAME" /usr/local/bin/$BINARY_NAME

echo ""
echo -e "  Binary   : ${GREEN}$INSTALL_DIR/$BINARY_NAME.bin${NC}"
echo -e "  Wrapper  : ${GREEN}$INSTALL_DIR/$BINARY_NAME${NC} (filters WSL2 X11 noise)"
echo -e "  Symlink  : ${GREEN}/usr/local/bin/$BINARY_NAME${NC}"
echo -e "  Shell RC : ${GREEN}$SHELL_RC${NC}"
echo ""
info "Done! Run 'jlog -h' to get started. Restart your shell or: source $SHELL_RC"
