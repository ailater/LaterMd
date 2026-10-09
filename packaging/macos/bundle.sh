#!/bin/bash
# Shared by local builds and the universal2 release workflow.
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "Usage: $0 <latermd-binary> <output.app> <version>" >&2
  exit 2
fi
if [[ $(uname -s) != Darwin ]]; then
  echo 'Building an app bundle requires macOS.' >&2
  exit 1
fi

binary=$1
app=$2
version=$3
repo_dir=$(cd "$(dirname "$0")/../.." && pwd)
if [[ ! -x "$binary" || "$app" != *.app || -L "$app" ]]; then
  echo 'Expected an executable binary and a non-symlink .app destination.' >&2
  exit 1
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$ ]]; then
  echo "Invalid version: $version" >&2
  exit 1
fi
# codesign can sign plain scripts too; require a native Mach-O input first.
lipo -archs "$binary" >/dev/null

# Only replace our generated bundle, never an unrelated destination directory.
if [[ -e "$app" ]]; then
  bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")
  if [[ "$bundle_id" != cn.ailater.LaterMD ]]; then
    echo "Refusing to replace a different application: $app" >&2
    exit 1
  fi
fi
mkdir -p "$(dirname "$app")"
stage=$(mktemp -d "$(dirname "$app")/.latermd-bundle.XXXXXX")
trap 'rm -rf "$stage"' EXIT
staged_app="$stage/LaterMD.app"
mkdir -p "$staged_app/Contents/MacOS" "$staged_app/Contents/Resources"
install -m 755 "$binary" "$staged_app/Contents/MacOS/latermd"
sed "s/__VERSION__/$version/g" "$repo_dir/packaging/macos/Info.plist" \
  > "$staged_app/Contents/Info.plist"
iconutil -c icns "$repo_dir/assets/logo/deliverables/macOS/AppIcon.iconset" \
  -o "$staged_app/Contents/Resources/AppIcon.icns"
plutil -lint "$staged_app/Contents/Info.plist"
# Ad-hoc signing does not notarize the app or bypass Gatekeeper.
codesign --force --sign - "$staged_app"
codesign --verify --deep --strict "$staged_app"
if [[ -e "$app" ]]; then
  rm -rf "$app"
fi
mv "$staged_app" "$app"
echo "App bundle: $app"
