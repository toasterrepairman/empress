#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/.."
cd "$PROJECT_ROOT"

APP_ID="com.github.toasterrepair.empress"
BINARY_NAME="empress"
MANIFEST="${APP_ID}.json"

clean_vendor() {
    echo "Cleaning vendored crates..."
    rm -rf vendor vendor.tar.gz
    echo "Vendor directory and archive removed."
}

clean_build_artifacts() {
    echo "Cleaning build artifacts..."
    rm -rf build-dir /tmp/flatpak-repo .flatpak-builder
    echo "Build artifacts removed."
}

clean_all() {
    clean_vendor
    clean_build_artifacts
    rm -f "${BINARY_NAME}.flatpak"
    echo "Full cleanup complete."
}

case "${1:-build}" in
    clean)
        clean_all
        exit 0
        ;;
    clean-vendor)
        clean_vendor
        exit 0
        ;;
    clean-build)
        clean_build_artifacts
        exit 0
        ;;
esac

echo "=== Flatpak Build for ${APP_ID} ==="
echo ""

if [ ! -f "Cargo.lock" ]; then
    echo "Error: Cargo.lock not found. Run 'cargo generate-lockfile' first."
    exit 1
fi

echo "[1/4] Cleaning stale vendor artifacts..."
clean_vendor

echo "[2/4] Vendoring Rust dependencies..."
cargo vendor vendor
tar czf vendor.tar.gz vendor
echo "Vendored $(find vendor -maxdepth 1 -mindepth 1 -type d | wc -l) crates."

echo "[3/4] Building Flatpak..."
flatpak-builder --force-clean --repo=/tmp/flatpak-repo build-dir "$MANIFEST"

echo "[4/4] Creating Flatpak bundle..."
flatpak build-bundle /tmp/flatpak-repo "${BINARY_NAME}.flatpak" "$APP_ID"

echo ""
echo "=== Build complete ==="
echo "Bundle: ${BINARY_NAME}.flatpak"
echo "Size: $(du -h "${BINARY_NAME}.flatpak" | cut -f1)"
echo ""
echo "Run '$0 clean' to remove all artifacts, or '$0 clean-vendor' to free vendor space."
