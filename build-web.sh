#!/usr/bin/env bash
# Builds the WebAssembly version of the app into ./dist
#
#   ./build-web.sh            # release build
#   ./build-web.sh --dev      # faster debug build
#   ./build-web.sh --serve    # build, then serve on http://localhost:8080
set -euo pipefail
cd "$(dirname "$0")"

PROFILE=release
CARGO_FLAGS=(--release)
SERVE=false
for arg in "$@"; do
    case "$arg" in
        --dev) PROFILE=debug; CARGO_FLAGS=() ;;
        --serve) SERVE=true ;;
        *) echo "unknown argument: $arg" >&2; exit 1 ;;
    esac
done

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

# The wasm-bindgen CLI must be exactly the same version as the wasm-bindgen crate
# in Cargo.lock, so install a matching copy into ./.tools (not globally).
WB_VERSION=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
WB=.tools/bin/wasm-bindgen
if [[ ! -x $WB ]] || [[ "$($WB --version)" != "wasm-bindgen $WB_VERSION" ]]; then
    echo "Installing wasm-bindgen-cli $WB_VERSION into .tools/ ..."
    cargo install wasm-bindgen-cli --version "$WB_VERSION" --locked --root .tools
fi

cargo build --target wasm32-unknown-unknown ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}

rm -rf dist
mkdir -p dist
"$WB" "target/wasm32-unknown-unknown/$PROFILE/egui-test.wasm" \
    --out-dir dist --out-name egui_test --target web --no-typescript
cp index.html dist/

echo "Built dist/ ($(du -h dist/egui_test_bg.wasm | cut -f1) wasm)"

if $SERVE; then
    echo "Serving on http://localhost:8080"
    python3 -m http.server 8080 --directory dist
fi
