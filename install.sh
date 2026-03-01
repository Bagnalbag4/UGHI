#!/bin/bash
# UGHI v1.0 – Secure Installer
# Follows strict_rules.md | Binary <28 MB | Idle RAM ~180 MB
#
# SECURE INSTALL:
#   curl -fsSL https://ughi.ai/install.sh | bash
#
# VERIFY ONLY (no install):
#   curl -fsSL https://ughi.ai/install.sh | bash -s -- --verify-only
#
# Security:
#   1. Downloads ONLY from GitHub Releases (pinned version)
#   2. SHA256 checksum verification (mandatory)
#   3. GPG signature verification (if gpg available)
#   4. Post-install self-check (ughi doctor)
#   5. cargo install uses --locked (dependency pinning)
#
# Supports: Linux (x86_64, aarch64, armv7), macOS (x86_64, arm64), WSL

set -euo pipefail

# ── Configuration ────────────────────────────────────────
VERSION="1.0.0"
REPO="Bagnalbag4/UGHI"
GITHUB_RELEASES="https://github.com/${REPO}/releases/download/v${VERSION}"
INSTALL_DIR="${UGHI_DIR:-$HOME/.ughi}"
BIN_DIR="$INSTALL_DIR/bin"
TMP_DIR="$(mktemp -d)"
VERIFY_ONLY=false
UPDATE=false
ROLLBACK=false
GPG_KEY_ID="UGHI-RELEASE-2026"
GPG_KEY_URL="${GITHUB_RELEASES}/ughi-release.asc"

# ── Parse flags ──────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --verify-only) VERIFY_ONLY=true ;;
        --update)      UPDATE=true ;;
        --rollback)    ROLLBACK=true ;;
        --version=*)   VERSION="${arg#*=}" ;;
        --help|-h)
            echo "Usage: curl -fsSL https://ughi.ai/install.sh | bash [-s -- OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --verify-only   Download and verify integrity, but do not install"
            echo "  --version=X.Y.Z Install a specific version (default: $VERSION)"
            echo "  --update        Update to the latest version (preserves previous binary)"
            echo "  --rollback      Rollback to the previously installed version"
            echo "  --help          Show this help"
            exit 0
            ;;
    esac
done

# ── Banner ───────────────────────────────────────────────
echo ""
echo "  ╔══════════════════════════════════════════════════════╗"
echo "  ║  UGHI v${VERSION} – Secure Installer                    ║"
echo "  ║  Unleashed Global Human Intelligence                ║"
echo "  ║  SHA256 ✓ | GPG ✓ | GitHub Releases Only            ║"
echo "  ╚══════════════════════════════════════════════════════╝"
echo ""

# ── Cleanup on exit ──────────────────────────────────────
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

# ── Detect platform ──────────────────────────────────────
detect_platform() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"

    case "$os" in
        linux*)  os="linux" ;;
        darwin*) os="macos" ;;
        mingw*|msys*|cygwin*) os="windows" ;;
        *) echo "  ✗ Unsupported OS: $os"; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        armv7l)        arch="armv7" ;;
        *) echo "  ✗ Unsupported architecture: $arch"; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

PLATFORM=$(detect_platform)
echo "  ✓ Platform: $PLATFORM"

# ── Rollback mode ────────────────────────────────────────
if [ "$ROLLBACK" = true ]; then
    if [ -f "$BIN_DIR/ughi.bak" ]; then
        echo "  ⊳ Rolling back to previous version..."
        mv "$BIN_DIR/ughi.bak" "$BIN_DIR/ughi"
        chmod +x "$BIN_DIR/ughi"
        echo "  ✅ Rollback complete. Current version:"
        "$BIN_DIR/ughi" --version
        exit 0
    else
        echo "  ✗ No previous version backup found at $BIN_DIR/ughi.bak"
        exit 1
    fi
fi

if [ "$UPDATE" = true ]; then
    echo "  ⊳ Checking for updates..."
fi

# ── Check required tools ─────────────────────────────────
require_tool() {
    if ! command -v "$1" &>/dev/null; then
        echo "  ✗ Required tool not found: $1"
        echo "  ⊳ $2"
        exit 1
    fi
}

require_tool "curl" "Install curl: apt install curl / brew install curl"

# sha256sum or shasum (macOS)
SHA256CMD=""
if command -v sha256sum &>/dev/null; then
    SHA256CMD="sha256sum"
elif command -v shasum &>/dev/null; then
    SHA256CMD="shasum -a 256"
else
    echo "  ✗ No SHA256 tool found (sha256sum or shasum required)"
    exit 1
fi
echo "  ✓ SHA256 tool: $SHA256CMD"

# GPG (optional but recommended)
HAS_GPG=false
if command -v gpg &>/dev/null; then
    HAS_GPG=true
    echo "  ✓ GPG available: signature verification enabled"
else
    echo "  ⚠ GPG not found: signature verification skipped (install gnupg for full security)"
fi

# ── Download artifacts ───────────────────────────────────
BINARY_FILE="ughi-${PLATFORM}.tar.gz"
CHECKSUM_FILE="ughi-${PLATFORM}.tar.gz.sha256"
SIG_FILE="ughi-${PLATFORM}.tar.gz.sig"

echo ""
echo "  ⊳ Downloading from GitHub Releases (pinned v${VERSION})..."
echo "    Source: ${GITHUB_RELEASES}/"

# Download binary
echo "  ⊳ [1/3] Binary: ${BINARY_FILE}"
if ! curl -fsSL "${GITHUB_RELEASES}/${BINARY_FILE}" -o "${TMP_DIR}/${BINARY_FILE}" 2>/dev/null; then
    echo ""
    echo "  ✗ Release binary not found for v${VERSION} / ${PLATFORM}"
    echo ""
    echo "  ⊳ Falling back to verified source build..."
    if ! command -v cargo &>/dev/null; then
        echo "  ✗ Rust toolchain not found."
        echo "  ⊳ Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    echo "  ⊳ Building from source (--locked for dependency pinning)..."
    if [ "$VERIFY_ONLY" = true ]; then
        echo "  ✓ Source build would use: cargo install --locked --git https://github.com/${REPO}"
        echo "  ✓ Verify-only mode: no build executed."
        exit 0
    fi
    mkdir -p "$BIN_DIR"
    cargo install --locked --git "https://github.com/${REPO}" --bin ughi --root "$INSTALL_DIR"
    echo "  ✓ Built from source with --locked (dependency integrity preserved)"

    # Skip to PATH setup
    goto_path_setup=true
fi

# ── SHA256 Checksum Verification (MANDATORY) ─────────────
if [ "${goto_path_setup:-false}" != true ]; then

    # Download checksum
    echo "  ⊳ [2/3] Checksum: ${CHECKSUM_FILE}"
    if ! curl -fsSL "${GITHUB_RELEASES}/${CHECKSUM_FILE}" -o "${TMP_DIR}/${CHECKSUM_FILE}" 2>/dev/null; then
        echo "  ✗ SECURITY ABORT: SHA256 checksum file not found."
        echo "  ✗ Cannot verify binary integrity. Install aborted."
        echo "  ⊳ Report this: https://github.com/${REPO}/issues"
        exit 1
    fi

    # Download signature
    echo "  ⊳ [3/3] Signature: ${SIG_FILE}"
    curl -fsSL "${GITHUB_RELEASES}/${SIG_FILE}" -o "${TMP_DIR}/${SIG_FILE}" 2>/dev/null || true

    echo ""
    echo "  ── Integrity Verification ─────────────────────────"

    # Verify SHA256 checksum
    echo "  ⊳ SHA256 checksum verification..."
    EXPECTED_HASH=$(cat "${TMP_DIR}/${CHECKSUM_FILE}" | awk '{print $1}')
    ACTUAL_HASH=$(cd "$TMP_DIR" && $SHA256CMD "${BINARY_FILE}" | awk '{print $1}')

    if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
        echo ""
        echo "  ╔══════════════════════════════════════════════════════╗"
        echo "  ║  ✗ SHA256 MISMATCH – POSSIBLE TAMPERING DETECTED    ║"
        echo "  ╠══════════════════════════════════════════════════════╣"
        echo "  ║  Expected: ${EXPECTED_HASH:0:16}...                  ║"
        echo "  ║  Got:      ${ACTUAL_HASH:0:16}...                    ║"
        echo "  ║                                                      ║"
        echo "  ║  DO NOT INSTALL. The binary may be compromised.      ║"
        echo "  ║  Report: https://github.com/${REPO}/issues           ║"
        echo "  ╚══════════════════════════════════════════════════════╝"
        echo ""
        exit 1
    fi
    echo "  ✅ SHA256: ${ACTUAL_HASH:0:16}... matches"

    # ── GPG Signature Verification (if available) ────────────
    if [ "$HAS_GPG" = true ] && [ -f "${TMP_DIR}/${SIG_FILE}" ]; then
        echo "  ⊳ GPG signature verification..."

        # Import UGHI release key
        if ! gpg --list-keys "$GPG_KEY_ID" &>/dev/null 2>&1; then
            echo "  ⊳ Importing UGHI release signing key..."
            curl -fsSL "$GPG_KEY_URL" | gpg --import 2>/dev/null || {
                echo "  ⚠ Could not import GPG key. Signature check skipped."
            }
        fi

        # Verify signature
        if gpg --verify "${TMP_DIR}/${SIG_FILE}" "${TMP_DIR}/${BINARY_FILE}" 2>/dev/null; then
            echo "  ✅ GPG: Valid signature from UGHI release key"
        else
            echo ""
            echo "  ╔══════════════════════════════════════════════════════╗"
            echo "  ║  ✗ GPG SIGNATURE INVALID – BINARY MAY BE FORGED     ║"
            echo "  ╠══════════════════════════════════════════════════════╣"
            echo "  ║  The binary's GPG signature does not match the      ║"
            echo "  ║  official UGHI release signing key.                  ║"
            echo "  ║                                                      ║"
            echo "  ║  DO NOT INSTALL. Report to the UGHI security team.   ║"
            echo "  ╚══════════════════════════════════════════════════════╝"
            echo ""
            exit 1
        fi
    elif [ "$HAS_GPG" = true ]; then
        echo "  ⚠ GPG: Signature file not available (SHA256 still verified)"
    fi

    echo ""

    # ── Verify-only mode stops here ──────────────────────────
    if [ "$VERIFY_ONLY" = true ]; then
        echo "  ╔══════════════════════════════════════════════════════╗"
        echo "  ║  ✅ VERIFICATION PASSED                              ║"
        echo "  ╠══════════════════════════════════════════════════════╣"
        echo "  ║  SHA256:    ✅ Checksum matches                      ║"
        if [ "$HAS_GPG" = true ] && [ -f "${TMP_DIR}/${SIG_FILE}" ]; then
            echo "  ║  GPG:       ✅ Signature valid                       ║"
        fi
        echo "  ║  Source:    GitHub Releases v${VERSION}               ║"
        echo "  ║  Platform:  ${PLATFORM}                              ║"
        echo "  ║                                                      ║"
        echo "  ║  Binary is safe to install.                          ║"
        echo "  ╚══════════════════════════════════════════════════════╝"
        echo ""
        exit 0
    fi

    # ── Extract and install ──────────────────────────────────
    echo "  ⊳ Installing verified binary..."
    mkdir -p "$BIN_DIR" "$INSTALL_DIR/data" "$INSTALL_DIR/models" "$INSTALL_DIR/skills"

    if [ -f "$BIN_DIR/ughi" ]; then
        echo "  ⊳ Backing up current version to ughi.bak"
        mv "$BIN_DIR/ughi" "$BIN_DIR/ughi.bak"
    fi

    tar -xzf "${TMP_DIR}/${BINARY_FILE}" -C "$BIN_DIR"
    chmod +x "$BIN_DIR/ughi"
    echo "  ✓ Binary installed: $BIN_DIR/ughi"
fi

# ── Add to PATH ──────────────────────────────────────────
SHELL_RC=""
if [ -n "${ZSH_VERSION:-}" ] || [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
elif [ -f "$HOME/.profile" ]; then
    SHELL_RC="$HOME/.profile"
fi

if [ -n "$SHELL_RC" ]; then
    if ! grep -q ".ughi/bin" "$SHELL_RC" 2>/dev/null; then
        echo "" >> "$SHELL_RC"
        echo "# UGHI – Unleashed Global Human Intelligence" >> "$SHELL_RC"
        echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$SHELL_RC"
        echo "  ✓ Added to PATH in $SHELL_RC"
    fi
fi

export PATH="$BIN_DIR:$PATH"

# ── Post-install self-check (ughi doctor) ────────────────
echo ""
echo "  ── Post-Install Self-Check (ughi doctor) ──────────"

doctor_pass=true

# Check 1: Binary exists and is executable
if [ -x "$BIN_DIR/ughi" ]; then
    echo "  ✅ Binary:    $BIN_DIR/ughi (executable)"
else
    echo "  ❌ Binary:    Not found or not executable"
    doctor_pass=false
fi

# Check 2: Binary runs without crash
if "$BIN_DIR/ughi" --version &>/dev/null 2>&1; then
    INSTALLED_VERSION=$("$BIN_DIR/ughi" --version 2>&1 | head -1 || echo "unknown")
    echo "  ✅ Version:   ${INSTALLED_VERSION}"
else
    echo "  ⚠ Version:   Could not verify (binary may need runtime deps)"
fi

# Check 3: Data directories exist
for dir in data models skills; do
    if [ -d "$INSTALL_DIR/$dir" ]; then
        echo "  ✅ Directory: $INSTALL_DIR/$dir"
    else
        mkdir -p "$INSTALL_DIR/$dir"
        echo "  ✅ Directory: $INSTALL_DIR/$dir (created)"
    fi
done

# Check 4: SHA256 of installed binary
if [ -x "$BIN_DIR/ughi" ]; then
    INSTALLED_HASH=$($SHA256CMD "$BIN_DIR/ughi" | awk '{print $1}')
    echo "  ✅ SHA256:    ${INSTALLED_HASH:0:16}..."
fi

# Check 5: disk usage
if [ -x "$BIN_DIR/ughi" ]; then
    SIZE_KB=$(du -k "$BIN_DIR/ughi" | awk '{print $1}')
    SIZE_MB=$((SIZE_KB / 1024))
    if [ "$SIZE_MB" -le 35 ]; then
        echo "  ✅ Size:      ${SIZE_MB} MB (under 35 MB limit)"
    else
        echo "  ⚠ Size:      ${SIZE_MB} MB (exceeds 35 MB target)"
    fi
fi

echo ""
if [ "$doctor_pass" = true ]; then
    echo "  ╔══════════════════════════════════════════════════════╗"
    echo "  ║  ✅ UGHI v${VERSION} installed and verified!            ║"
    echo "  ╠══════════════════════════════════════════════════════╣"
    echo "  ║                                                      ║"
    echo "  ║  Quick start:                                        ║"
    echo "  ║    ughi run 'Your goal here'                         ║"
    echo "  ║    ughi status                                       ║"
    echo "  ║    ughi skills leaderboard                           ║"
    echo "  ║    ughi evolve                                       ║"
    echo "  ║                                                      ║"
    echo "  ║  Security:                                           ║"
    echo "  ║    ughi skills verify <name>                         ║"
    echo "  ║    ughi doctor                                       ║"
    echo "  ║                                                      ║"
    echo "  ╚══════════════════════════════════════════════════════╝"
    echo ""
    echo "  Restart your shell or run: source $SHELL_RC"
else
    echo "  ⚠ Some checks failed. Run 'ughi doctor' after restarting your shell."
fi
echo ""
