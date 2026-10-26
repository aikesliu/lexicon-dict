#!/usr/bin/env bash
# 产出：Linux tar.gz + RPM；Windows 免安装 zip；SHA256SUMS。
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export RUSTC_WRAPPER="$ROOT/scripts/rustc-remap.sh"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="$ROOT/dist"
rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> 构建 Linux release"
cargo build --release -p lexicon-app
BIN="$ROOT/target/release/lexicon-dict"

LIN="lexicon-dict-$VERSION-linux-x86_64"
STAGE="$DIST/$LIN"
mkdir -p "$STAGE"
install -m 0755 "$BIN" "$STAGE/lexicon-dict"
install -m 0644 packaging/io.github.lexicon.dict.desktop "$STAGE/io.github.lexicon.dict.desktop"
install -m 0644 packaging/io.github.lexicon.dict.svg "$STAGE/io.github.lexicon.dict.svg"
install -m 0644 packaging/README.txt "$STAGE/README.txt"
install -m 0755 packaging/install.sh "$STAGE/install.sh"
install -m 0755 packaging/uninstall.sh "$STAGE/uninstall.sh"
tar -C "$DIST" --owner=0 --group=0 --numeric-owner -czf "$DIST/$LIN.tar.gz" "$LIN"
echo "    $DIST/$LIN.tar.gz"

if command -v rpmbuild >/dev/null 2>&1; then
  echo "==> 构建 RPM"
  TOP="$DIST/rpmbuild"
  mkdir -p "$TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
  cp "$BIN" "$TOP/SOURCES/lexicon-dict"
  cp packaging/fedora/io.github.lexicon.dict.desktop "$TOP/SOURCES/"
  cp packaging/fedora/io.github.lexicon.dict.svg "$TOP/SOURCES/"
  sed "s/@VERSION@/$VERSION/" packaging/fedora/lexicon-dict.spec.in > "$TOP/SPECS/lexicon-dict.spec"
  rpmbuild -bb "$TOP/SPECS/lexicon-dict.spec" --define "_topdir $TOP" >/dev/null
  cp "$TOP"/RPMS/x86_64/*.rpm "$DIST/"
  rm -rf "$TOP"
  echo "    $(ls "$DIST"/*.rpm)"
fi

echo "==> 构建 Windows release"
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  cargo build --release --target x86_64-pc-windows-gnu -p lexicon-app
  WIN="lexicon-dict-$VERSION-windows-x86_64"
  WSTAGE="$DIST/$WIN"
  mkdir -p "$WSTAGE"
  install -m 0755 "$ROOT/target/x86_64-pc-windows-gnu/release/lexicon-dict.exe" "$WSTAGE/lexicon-dict.exe"
  install -m 0644 packaging/windows/README.txt "$WSTAGE/README.txt"
  : > "$WSTAGE/portable"
  ( cd "$DIST" && zip -qrX "$WIN.zip" "$WIN" )
  echo "    $DIST/$WIN.zip"
else
  echo "    （跳过 Windows：未找到 mingw 工具链）"
fi

( cd "$DIST" && sha256sum *.zip *.tar.gz *.rpm 2>/dev/null > SHA256SUMS ) || true
echo "==> 完成："
ls -lh "$DIST" | tail -n +2
