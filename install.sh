#!/bin/bash
set -euo pipefail

BINARY_NAME="sclean"
INSTALL_DIR="${PREFIX:-/usr/local}/bin"
REPO="https://github.com/mohdismailmatasin/simple-cleaner-rs.git"
TMPDIR=$(mktemp -d)

trap 'rm -rf "$TMPDIR"' EXIT

echo "Installing ${BINARY_NAME}..."

# Clone repo
echo "Cloning repository..."
git clone --depth 1 "$REPO" "$TMPDIR"
cd "$TMPDIR"

# Check for cargo
if ! command -v cargo &>/dev/null; then
    echo "Error: cargo not found. Please install Rust: https://rustup.rs/"
    exit 1
fi

# Build binary
echo "Building binary..."
cargo build --release

SOURCE_BINARY="./target/release/${BINARY_NAME}"
if [ ! -f "${SOURCE_BINARY}" ]; then
    echo "Error: Binary not found after build!"
    exit 1
fi

# Install binary
mkdir -p "${INSTALL_DIR}"
cp "${SOURCE_BINARY}" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

# Generate shell completions
COMPLETIONS_DIR="${PREFIX:-/usr/local}/share/bash-completion/completions"
if command -v "${INSTALL_DIR}/${BINARY_NAME}" &>/dev/null; then
    mkdir -p "${COMPLETIONS_DIR}"
    "${INSTALL_DIR}/${BINARY_NAME}" --completions bash > "${COMPLETIONS_DIR}/${BINARY_NAME}" 2>/dev/null || true
fi

echo ""
echo "Installed successfully to ${INSTALL_DIR}/${BINARY_NAME}"
echo "Run with: ${BINARY_NAME}"
echo "Preview mode: ${BINARY_NAME} --preview"
