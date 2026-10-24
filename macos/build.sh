#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "macos/build.sh must be run on a Mac (Apple clang + SDK)." >&2
  echo "Windows and Linux cannot compile the macOS app: sqlite/ring need -arch and the Apple SDK." >&2
  echo "Use the GitHub 'macOS build' workflow instead, or run this on a Mac." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/dist/macos"
APP="$OUT/ModDB.app"
cd "$ROOT"

VERSION="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)"
if [ -z "$VERSION" ]; then
  echo "cannot read version from Cargo.toml" >&2
  exit 1
fi

# Universal (Apple Silicon + Intel) by default; UNIVERSAL=0 builds this Mac only.
if [ "${UNIVERSAL:-1}" = "1" ]; then
  TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
  SUFFIX="universal"
else
  case "$(uname -m)" in
    arm64) TARGETS=(aarch64-apple-darwin) ;;
    x86_64) TARGETS=(x86_64-apple-darwin) ;;
    *)
      echo "unsupported CPU: $(uname -m)" >&2
      exit 1
      ;;
  esac
  SUFFIX="$(uname -m)"
fi

BINS=()
for TARGET in "${TARGETS[@]}"; do
  rustup target add "$TARGET"
  cargo build --release --target "$TARGET"
  BIN="$ROOT/target/$TARGET/release/ModDB"
  if [ ! -f "$BIN" ]; then
    echo "missing binary: $BIN" >&2
    exit 1
  fi
  BINS+=("$BIN")
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/macos/Info.plist" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"

if [ "${#BINS[@]}" -gt 1 ]; then
  lipo -create "${BINS[@]}" -output "$APP/Contents/MacOS/ModDB"
else
  cp "${BINS[0]}" "$APP/Contents/MacOS/ModDB"
fi
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

# Apple Silicon refuses to launch unsigned code; ad-hoc signing keeps local/direct downloads working.
codesign --force --deep --sign - "$APP" >/dev/null
codesign --verify --deep --strict "$APP"

ZIP="$OUT/MCDB-$VERSION-macos-$SUFFIX.zip"
rm -f "$ZIP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

echo "$APP"
echo "$ZIP"
