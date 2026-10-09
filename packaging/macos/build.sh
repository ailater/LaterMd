#!/bin/bash
# Build a development app without installing to /Applications or publishing.
set -euo pipefail

usage() {
  echo "Usage: $0 [--release] [--universal] [--dmg] [--open]"
  echo 'Default: debug build for this Mac. --universal builds arm64 + x86_64.'
}
profile=debug
universal=false
make_dmg=false
launch=false
for arg in "$@"; do
  case "$arg" in
    --release) profile=release ;;
    --universal) universal=true ;;
    --dmg) make_dmg=true ;;
    --open) launch=true ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
if [[ $(uname -s) != Darwin ]]; then
  echo 'This build script requires macOS and Xcode Command Line Tools.' >&2
  exit 1
fi
repo_dir=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo_dir"
for tool in cargo rustc python3 iconutil codesign plutil; do
  command -v "$tool" >/dev/null || { echo "Missing tool: $tool" >&2; exit 1; }
done
xcrun --find clang >/dev/null
# Same deployment baseline as Info.plist, for both Mach-O slices.
export MACOSX_DEPLOYMENT_TARGET=14.0
host=$(rustc -vV | sed -n 's/^host: //p')
case "$host" in
  aarch64-apple-darwin|x86_64-apple-darwin) ;;
  *) echo "Unsupported host: $host" >&2; exit 1 ;;
esac

# Cargo metadata respects CARGO_TARGET_DIR and workspace version inheritance.
metadata=$(cargo metadata --locked --no-deps --format-version 1)
target_dir=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<< "$metadata")
version=$(python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "latermd-app"))' <<< "$metadata")
build_args=(build --locked -p latermd-app --bin latermd)
if [[ "$profile" == release ]]; then
  build_args+=(--release)
fi
if $universal; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  targets=(aarch64-apple-darwin x86_64-apple-darwin)
  architecture=universal2
else
  targets=("$host")
  architecture=$host
fi
for target in "${targets[@]}"; do
  cargo "${build_args[@]}" --target "$target"
done
output_dir="$target_dir/macos/$profile/$architecture"
mkdir -p "$output_dir"
binary="$target_dir/$host/$profile/latermd"
if $universal; then
  binary="$output_dir/latermd-universal"
  lipo -create "$target_dir/aarch64-apple-darwin/$profile/latermd" \
    "$target_dir/x86_64-apple-darwin/$profile/latermd" -output "$binary"
  lipo "$binary" -verify_arch arm64
  lipo "$binary" -verify_arch x86_64
fi
app="$output_dir/LaterMD.app"
bash "$repo_dir/packaging/macos/bundle.sh" "$binary" "$app" "$version"
if $make_dmg; then
  dmg="$output_dir/latermd-v$version-$architecture.dmg"
  hdiutil create -volname LaterMD -srcfolder "$app" -ov -format UDZO "$dmg"
  hdiutil verify "$dmg"
  echo "DMG: $dmg"
fi
if $launch; then
  open -n "$app"
fi
