#!/bin/bash
set -euo pipefail

BINARY_NAME="sclean"
INSTALL_DIR="${PREFIX:-/usr/local}/bin"
COMPLETIONS_DIR="${PREFIX:-/usr/local}/share/bash-completion/completions"
CONFIG_DIR="${HOME}/.config/${BINARY_NAME}"

echo "Uninstalling ${BINARY_NAME}..."

# Check write permission
if [ ! -w "$(dirname "${INSTALL_DIR}")" ]; then
    USE_SUDO=true
else
    USE_SUDO=false
fi

# Remove binary
if [ -f "${INSTALL_DIR}/${BINARY_NAME}" ]; then
    if [ "$USE_SUDO" = true ]; then
        sudo rm -f "${INSTALL_DIR}/${BINARY_NAME}"
    else
        rm -f "${INSTALL_DIR}/${BINARY_NAME}"
    fi
    echo "Removed ${INSTALL_DIR}/${BINARY_NAME}"
else
    echo "Binary not found at ${INSTALL_DIR}/${BINARY_NAME}"
fi

# Remove shell completions
if [ -f "${COMPLETIONS_DIR}/${BINARY_NAME}" ]; then
    if [ "$USE_SUDO" = true ]; then
        sudo rm -f "${COMPLETIONS_DIR}/${BINARY_NAME}"
    else
        rm -f "${COMPLETIONS_DIR}/${BINARY_NAME}"
    fi
    echo "Removed shell completions"
fi

# Ask about config
if [ -d "${CONFIG_DIR}" ]; then
    echo ""
    read -rp "Remove config directory (${CONFIG_DIR})? [y/N] " response
    if [[ "${response}" =~ ^[Yy] ]]; then
        rm -rf "${CONFIG_DIR}"
        echo "Removed ${CONFIG_DIR}"
    else
        echo "Config directory preserved"
    fi
fi

echo ""
echo "Uninstalled successfully"
