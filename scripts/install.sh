#!/bin/bash
set -euo pipefail

# Peri Install Script
# Usage: curl -fsSL https://raw.githubusercontent.com/konghayao/peri/main/scripts/install.sh | bash
#
# Options:
#   PERI_INSTALL_VERSION   Specific version tag (e.g. agent-v1.17), empty = latest
#   PERI_INSTALL_DIR       Install directory (default: $HOME/.peri)
#   GITHUB_PROXY           GitHub download proxy prefix (replaces https://github.com in download URL)
#   PERI_NO_PATH_HINT      Set to 1 to skip PATH hint
#
# Example:
#   PERI_INSTALL_VERSION=agent-v1.17 bash install.sh
#   GITHUB_PROXY=https://ghproxy.com/https://github.com curl ... | bash

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step()    { echo -e "${CYAN}[STEP]${NC}  $*"; }

# --- Platform Detection ---
detect_platform() {
    local os arch platform

    case "$(uname -s)" in
        Darwin)  os="macos" ;;
        Linux)   os="linux" ;;
        *)       error "Unsupported OS: $(uname -s)"; exit 1 ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        riscv64)       arch="riscv64" ;;
        *)             error "Unsupported arch: $(uname -m)"; exit 1 ;;
    esac

    platform="${os}-${arch}"
    info "Detected platform: ${platform}"
    echo "${platform}"
}

# --- Download with optional proxy ---
get_download_url() {
    local url="$1"
    local proxy="${GITHUB_PROXY:-}"
    if [[ -n "${proxy}" ]]; then
        echo "${url/https:\/\/github.com/${proxy}}"
    else
        echo "${url}"
    fi
}

# --- Main ---
main() {
    INSTALL_DIR="${PERI_INSTALL_DIR:-${HOME}/.peri}"
    GITHUB_API="https://api.github.com/repos/konghayao/peri"

    echo ""
    info "Peri Agent Installer"
    info "-------------------------------"

    PLATFORM=$(detect_platform)

    # Fetch release info
    if [[ -n "${PERI_INSTALL_VERSION:-}" ]]; then
        VERSION_TAG="${PERI_INSTALL_VERSION}"
        step "Fetching release: ${VERSION_TAG}..."
        RELEASE_JSON=$(curl -fsSL "${GITHUB_API}/releases/tags/${VERSION_TAG}" 2>/dev/null) || {
            error "Failed to fetch release '${VERSION_TAG}'. Does this tag exist?"
            exit 1
        }
    else
        step "Fetching latest agent release..."
        RELEASES_JSON=$(curl -fsSL "${GITHUB_API}/releases?per_page=30" 2>/dev/null) || {
            error "Failed to fetch releases from GitHub."
            exit 1
        }
        # Find latest agent-* tag
        VERSION_TAG=$(echo "${RELEASES_JSON}" | grep -oE '"tag_name": *"agent-[^"]+"' | head -1 | cut -d'"' -f4)
        if [[ -z "${VERSION_TAG}" ]]; then
            error "No agent release found."
            exit 1
        fi

        # Fetch the specific release for asset list
        RELEASE_JSON=$(curl -fsSL "${GITHUB_API}/releases/tags/${VERSION_TAG}" 2>/dev/null) || {
            error "Failed to fetch release '${VERSION_TAG}'."
            exit 1
        }
    fi

    info "Found release: ${VERSION_TAG}"

    # Find matching asset
    # asset names: agent-tui-{platform} (e.g. agent-tui-macos-aarch64, agent-tui-windows-x86_64.exe)
    ASSET_NAME="agent-tui-${PLATFORM}"

    DOWNLOAD_URL=$(echo "${RELEASE_JSON}" | grep -oE "\"browser_download_url\": *\"[^\"]*${ASSET_NAME}[^\"]*\"" | head -1 | cut -d'"' -f4)

    if [[ -z "${DOWNLOAD_URL}" ]]; then
        error "No binary found for platform '${PLATFORM}'."
        echo ""
        echo "Available assets:"
        echo "${RELEASE_JSON}" | grep -oE '"browser_download_url": *"[^"]+"' | sed 's/"browser_download_url": *"/  - /;s/"//'
        exit 1
    fi

    info "Binary: ${ASSET_NAME}"

    # Create install directory
    VERSION_DIR="${INSTALL_DIR}/${VERSION_TAG}"
    mkdir -p "${VERSION_DIR}"

    TARGET="${VERSION_DIR}/agent"

    # Download
    FINAL_URL=$(get_download_url "${DOWNLOAD_URL}")
    if [[ "${FINAL_URL}" != "${DOWNLOAD_URL}" ]]; then
        info "Using proxy: ${FINAL_URL}"
    fi

    step "Downloading..."
    curl -fSL --progress-bar "${FINAL_URL}" -o "${TARGET}" || {
        error "Download failed."
        exit 1
    }

    # Make executable
    chmod +x "${TARGET}"
    info "Installed to: ${TARGET}"

    # Create symlink for convenience
    LINK="${INSTALL_DIR}/peri"
    rm -f "${LINK}"
    ln -sf "${TARGET}" "${LINK}"

    # Write current version
    echo "${VERSION_TAG}" > "${INSTALL_DIR}/current-version.txt"

    # --- PATH Check ---
    if [[ "${PERI_NO_PATH_HINT:-}" != "1" ]]; then
        BIN_LINK="${INSTALL_DIR}/peri"
        SHELL_PROFILE=""
        case "${SHELL:-}" in
            */zsh)  SHELL_PROFILE="${HOME}/.zshrc" ;;
            */bash) SHELL_PROFILE="${HOME}/.bashrc" ;;
            */fish) SHELL_PROFILE="${HOME}/.config/fish/config.fish" ;;
        esac

        if [[ -n "${SHELL_PROFILE}" ]] && ! grep -qF "${INSTALL_DIR}" "${SHELL_PROFILE}" 2>/dev/null; then
            echo ""
            warn "The install directory is not in your PATH."
            echo ""
            echo "  To add it now, run:"
            echo ""
            if [[ "${SHELL}" == */fish ]]; then
                echo "    echo 'set -gx PATH ${INSTALL_DIR} \$PATH' >> ${SHELL_PROFILE}"
            else
                echo "    echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ${SHELL_PROFILE}"
            fi
            echo "    source ${SHELL_PROFILE}"
            echo ""
        elif [[ -z "${SHELL_PROFILE}" ]]; then
            echo ""
            warn "Unknown shell. Add this directory to your PATH manually:"
            echo "    ${INSTALL_DIR}"
            echo ""
        fi
    fi

    echo ""
    info "Installation complete! Version: ${VERSION_TAG}"
    echo ""

    if command -v "${BIN_LINK}" &>/dev/null || [[ -x "${BIN_LINK}" ]]; then
        info "Run 'peri' to start."
    else
        info "Run: ${BIN_LINK}"
    fi
    echo ""
}

main
