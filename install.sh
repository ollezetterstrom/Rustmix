#!/bin/sh
set -e

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64) TARGET="x86_64-apple-darwin" ;;
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

if [ -z "$INSTALL_DIR" ]; then
  if [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="$HOME/.local/bin"
  fi
fi

mkdir -p "$INSTALL_DIR"

URL="https://github.com/ollezetterstrom/Rustmix/releases/latest/download/rustmix-${TARGET}.tar.gz"
echo "==> Downloading rustmix for ${TARGET}..."
curl -fsSL "$URL" | tar -xz -C "$INSTALL_DIR" rustmix
chmod +x "$INSTALL_DIR/rustmix"

echo "==> rustmix installed successfully to $INSTALL_DIR/rustmix"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Notice: $INSTALL_DIR is not in your PATH."
    echo "Add it by adding this line to your ~/.bashrc or ~/.zshrc:"
    echo "    export PATH=\"\$PATH:$INSTALL_DIR\""
    echo ""
    ;;
esac
