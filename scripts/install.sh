#!/bin/sh
set -e

# Lumen Installation Script
# This script downloads and installs the latest Lumen binaries.

REPO="alliecatowo/lumen"
GITHUB_URL="https://github.com/$REPO"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

printf "${BLUE}==>${NC} ${GREEN}Detecting system...${NC}\n"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
FORMAT="tar.gz"

case "$OS" in
  linux)
    PLATFORM="linux"
    ;;
  darwin)
    PLATFORM="macos"
    ;;
  mingw*|msys*|cygwin*)
    PLATFORM="windows"
    FORMAT="zip"
    ;;
  *)
    printf "${RED}Error: Unsupported OS $OS${NC}\n"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)
    ARCH_NAME="x64"
    ;;
  arm64|aarch64)
    ARCH_NAME="arm64"
    ;;
  *)
    printf "${RED}Error: Unsupported architecture $ARCH${NC}\n"
    exit 1
    ;;
esac

# Handle Linux MUSL if needed (default to GNU)
if [ "$PLATFORM" = "linux" ] && [ "$ARCH_NAME" = "x64" ]; then
    if ldd --version 2>&1 | grep -q "musl"; then
        ARCH_NAME="x64-musl"
    fi
fi

ASSET_NAME="lumen-$PLATFORM-$ARCH_NAME.$FORMAT"

printf "${BLUE}==>${NC} ${GREEN}Fetching latest release...${NC}\n"

# Get latest tag from GitHub API
TAG=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$TAG" ]; then
    printf "${RED}Error: Could not determine latest version.${NC}\n"
    exit 1
fi

DOWNLOAD_URL="$GITHUB_URL/releases/download/$TAG/$ASSET_NAME"

printf "${BLUE}==>${NC} ${GREEN}Downloading Lumen $TAG for $PLATFORM-$ARCH_NAME...${NC}\n"

TMP_DIR=$(mktemp -d)
curl -L -o "$TMP_DIR/$ASSET_NAME" "$DOWNLOAD_URL"

printf "${BLUE}==>${NC} ${GREEN}Installing...${NC}\n"

INSTALL_DIR="/usr/local/bin"
if [ ! -w "$INSTALL_DIR" ]; then
    printf "${BLUE}==>${NC} ${BLUE}Note: $INSTALL_DIR is not writable. Attempting to install to ~/.lumen/bin...${NC}\n"
    INSTALL_DIR="$HOME/.lumen/bin"
    mkdir -p "$INSTALL_DIR"
    
    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        printf "${RED}Warning: $INSTALL_DIR is not in your PATH.${NC}\n"
        printf "Please add 'export PATH=\"\$PATH:$INSTALL_DIR\"' to your shell profile.\n"
    fi
fi

cd "$TMP_DIR"
if [ "$FORMAT" = "tar.gz" ]; then
    tar -xzf "$ASSET_NAME"
else
    unzip "$ASSET_NAME"
fi

mv lumen lumen-lsp "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/lumen" "$INSTALL_DIR/lumen-lsp"

rm -rf "$TMP_DIR"

printf "\n${GREEN}Lumen $TAG installed successfully!${NC}\n"
printf "Run '${BLUE}lumen --version${NC}' to get started.\n"
