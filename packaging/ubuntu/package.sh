#!/usr/bin/env bash
# 在 Debian/Ubuntu 内把一个已编译好的 lexicon-dict 打包成 .deb 和便携 tar.gz。
# 由 packaging/ubuntu/build.sh 在 Ubuntu 容器内调用，也可在 Debian/Ubuntu 原生运行。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)}"
case "$(uname -m)" in
    x86_64) DEFAULT_ARCH=amd64 ;;
    aarch64|arm64) DEFAULT_ARCH=arm64 ;;
    *) DEFAULT_ARCH=amd64 ;;
esac
ARCH="${ARCH:-$DEFAULT_ARCH}"
OUT="${OUT:-$ROOT/dist}"
BIN="${1:-$ROOT/target/release/lexicon-dict}"

[ -x "$BIN" ] || { echo "未找到可执行文件：$BIN" >&2; exit 1; }
command -v dpkg-deb >/dev/null || { echo "需要 dpkg-deb（Debian/Ubuntu）" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

# ---- deb 根目录 ---------------------------------------------------------
PKG="lexicon-dict_${VERSION}_${ARCH}"
PKGROOT="$WORK/$PKG"
install -Dm755 "$BIN" "$PKGROOT/usr/bin/lexicon-dict"
install -Dm644 "$ROOT/packaging/io.github.lexicon.dict.desktop" \
    "$PKGROOT/usr/share/applications/io.github.lexicon.dict.desktop"
install -Dm644 "$ROOT/packaging/io.github.lexicon.dict.svg" \
    "$PKGROOT/usr/share/icons/hicolor/scalable/apps/io.github.lexicon.dict.svg"
install -Dm644 "$ROOT/crates/lexicon-app/assets/icon.png" \
    "$PKGROOT/usr/share/icons/hicolor/256x256/apps/io.github.lexicon.dict.png"
install -Dm644 "$ROOT/packaging/debian/copyright" \
    "$PKGROOT/usr/share/doc/lexicon-dict/copyright"
if [ -f "$ROOT/README.md" ]; then
    gzip -9 -n -c "$ROOT/README.md" > "$PKGROOT/usr/share/doc/lexicon-dict/README.md.gz"
fi
INSTALLED_SIZE="$(du -sk "$PKGROOT" | cut -f1)"
install -d "$PKGROOT/DEBIAN"

cat > "$PKGROOT/DEBIAN/control" <<EOF
Package: lexicon-dict
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.35), libgcc-s1, libx11-6, libxcursor1, libxrandr2, libxi6, libxinerama1, libxxf86vm1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, libwayland-egl1, libgl1, libegl1, hicolor-icon-theme
Recommends: fonts-noto-cjk, fonts-cantarell
Maintainer: lexicon-dict contributors <noreply@example.invalid>
Homepage: https://example.invalid/lexicon-dict
Description: offline dictionary desktop app
 Single-binary offline dictionary reader for .ld2 and StarDict,
 with in-process query and rendering. No daemon, no browser engine and no
 external UI runtime.
 .
 Runs on Debian 12+/Ubuntu 22.04+ (amd64) and Windows 11.
EOF
chmod 0644 "$PKGROOT/DEBIAN/control"
install -Dm755 "$ROOT/packaging/debian/postinst" "$PKGROOT/DEBIAN/postinst"
dpkg-deb --build --root-owner-group "$PKGROOT" "$OUT/$PKG.deb"
echo "    $OUT/$PKG.deb"

# ---- 便携 tar.gz（同一份 Ubuntu 兼容二进制） ---------------------------
STAGE="lexicon-dict-${VERSION}-ubuntu22.04-${ARCH}"
STAGEROOT="$WORK/$STAGE"
install -Dm755 "$BIN" "$STAGEROOT/lexicon-dict"
install -Dm644 "$ROOT/packaging/io.github.lexicon.dict.desktop" "$STAGEROOT/io.github.lexicon.dict.desktop"
install -Dm644 "$ROOT/packaging/io.github.lexicon.dict.svg" "$STAGEROOT/io.github.lexicon.dict.svg"
install -Dm644 "$ROOT/packaging/README.txt" "$STAGEROOT/README.txt"
install -Dm755 "$ROOT/packaging/install.sh" "$STAGEROOT/install.sh"
install -Dm755 "$ROOT/packaging/uninstall.sh" "$STAGEROOT/uninstall.sh"
tar -C "$WORK" --owner=0 --group=0 --numeric-owner -czf "$OUT/$STAGE.tar.gz" "$STAGE"
echo "    $OUT/$STAGE.tar.gz"

( cd "$OUT" && sha256sum "$PKG.deb" "$STAGE.tar.gz" > SHA256SUMS.ubuntu )
echo "    $OUT/SHA256SUMS.ubuntu"
