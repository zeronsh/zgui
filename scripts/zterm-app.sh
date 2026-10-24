#!/bin/sh
# Build the terminal example as zterm.app (the dist profile), its interface
# compiled in (terminal-bundle), and install it: ./scripts/zterm-app.sh [destination]
# (default /Applications). Needs Zig 0.16 for Ghostty (see
# crates/zgx-ghostty-sys).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dest=${1:-/Applications}
cd "$root"
cargo build --profile dist -p zgx-desktop --features ghostty,terminal-bundle --example terminal

app="$dest/zterm.app"
staging=$(mktemp -d)/zterm.app
mkdir -p "$staging/Contents/MacOS" "$staging/Contents/Resources"
cp target/dist/examples/terminal "$staging/Contents/MacOS/zterm"
if [ -f scripts/zterm.icns ]; then
  cp scripts/zterm.icns "$staging/Contents/Resources/zterm.icns"
fi
cat > "$staging/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>zterm</string>
  <key>CFBundleDisplayName</key><string>zterm</string>
  <key>CFBundleIdentifier</key><string>sh.zeron.zterm</string>
  <key>CFBundleExecutable</key><string>zterm</string>
  <key>CFBundleIconFile</key><string>zterm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST
# Signed with a stable identity, macOS keeps what the user allowed it (its
# shells reading ~/Documents, say) across rebuilds; an ad-hoc signature
# changes every build, so each install would be a new app to it and asking
# again, or refusing. ZTERM_SIGN_IDENTITY picks one; else the first valid
# Apple Development identity, else ad-hoc.
identity=${ZTERM_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null |
  grep '"Apple Development' | grep -v CSSMERR | head -1 | awk '{print $2}')}
if [ -n "$identity" ] && codesign --force --deep --sign "$identity" "$staging" >/dev/null 2>&1; then
  echo "signed with $identity"
else
  codesign --force --deep --sign - "$staging" >/dev/null 2>&1 || true
  echo "signed ad hoc: macOS asks again for its permissions after each install"
fi
rm -rf "$app"
mv "$staging" "$app"
echo "installed $app"
