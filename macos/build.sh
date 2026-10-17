#!/usr/bin/env bash
set -euo pipefail
if [ "$(uname -s)" != "Darwin" ]; then
  echo "macos/build.sh must be run on a Mac (Apple clang + SDK)." >&2
  echo "Windows cannot compile the macOS app: sqlite/eframe need -arch and AppKit." >&2
  exit 1
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/dist/macos"
APP="$OUT/ModDB.app"
cd "$ROOT"

ARCH="$(uname -m)"
if [ "$ARCH" = "arm64" ]; then
  TARGET="aarch64-apple-darwin"
else
  TARGET="x86_64-apple-darwin"
fi

rustup target add "$TARGET"
cargo build --release --target "$TARGET"
BIN="$ROOT/target/$TARGET/release/ModDB"
if [ ! -f "$BIN" ]; then
  echo "missing binary: $BIN" >&2
  exit 1
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/macos/Info.plist" "$APP/Contents/Info.plist"
cp "$BIN" "$APP/Contents/MacOS/ModDB"
chmod +x "$APP/Contents/MacOS/ModDB"

PNG="$ROOT/assets/icon.png"
if [ -f "$PNG" ] && command -v sips >/dev/null && command -v iconutil >/dev/null; then
  ICONSET="$OUT/icon.iconset"
  rm -rf "$ICONSET"
  mkdir -p "$ICONSET"
  sips -z 16 16 "$PNG" --out "$ICONSET/icon_16x16.png" >/dev/null
  sips -z 32 32 "$PNG" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
  sips -z 32 32 "$PNG" --out "$ICONSET/icon_32x32.png" >/dev/null
  sips -z 64 64 "$PNG" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
  sips -z 128 128 "$PNG" --out "$ICONSET/icon_128x128.png" >/dev/null
  sips -z 256 256 "$PNG" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
  sips -z 256 256 "$PNG" --out "$ICONSET/icon_256x256.png" >/dev/null
  sips -z 512 512 "$PNG" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
  sips -z 512 512 "$PNG" --out "$ICONSET/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$PNG" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"
  rm -rf "$ICONSET"
elif [ -f "$PNG" ]; then
  cp "$PNG" "$APP/Contents/Resources/icon.png"
fi

echo "$APP"
