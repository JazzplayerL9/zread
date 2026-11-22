#!/usr/bin/env bash
set -euo pipefail

PKG="zread"
VER="0.1.0"
REV="1"
ARCH="arm64"
OUT="${PKG}_${VER}-${REV}_${ARCH}.deb"
BIN="target/aarch64-unknown-linux-musl/release/zread"

cargo build --release --target aarch64-unknown-linux-musl --offline

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/data/usr/bin" \
         "$STAGE/data/usr/share/APPLaunch/applications" \
         "$STAGE/data/usr/share/APPLaunch/share/images" \
         "$STAGE/control"

install -m 755 "$BIN" "$STAGE/data/usr/bin/zread"
install -m 644 zread.desktop "$STAGE/data/usr/share/APPLaunch/applications/zread.desktop"
install -m 644 assets/images/zread_icon.png "$STAGE/data/usr/share/APPLaunch/share/images/zread.png"

INSTALLED_KB=$(du -sk "$STAGE/data" | awk '{print $1}')

cat > "$STAGE/control/control" <<EOF
Package: ${PKG}
Version: ${VER}-${REV}
Section: utils
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_KB}
Depends: curl
Maintainer: JazzplayerL9 <https://github.com/JazzplayerL9/zread>
Description: Lightweight TUI e-reader for CardputerZero
 Distraction-free Rust TUI e-reader for EPUB, Markdown, and TXT
 with Table of Contents, bookmarks, quick notes, search, RSVP,
 and Standard Ebooks / Project Gutenberg / Internet Archive download.
EOF

printf "2.0\n" > "$STAGE/debian-binary"
tar -czf "$STAGE/control.tar.gz" -C "$STAGE/control" .
tar -czf "$STAGE/data.tar.gz" -C "$STAGE/data" --owner=0 --group=0 .

rm -f "$OUT"
ar r "$OUT" "$STAGE/debian-binary" "$STAGE/control.tar.gz" "$STAGE/data.tar.gz"
echo "Built $OUT"
