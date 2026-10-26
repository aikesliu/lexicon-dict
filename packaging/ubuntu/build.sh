#!/usr/bin/env bash
# 在 Ubuntu 22.04 容器里构建二进制并产出 .deb + 便携 tar.gz。
# 用 22.04 作为基线，二进制可在 Ubuntu 22.04+ / Debian 12+ 上运行（amd64）。
#
#   ./packaging/ubuntu/build.sh
#   ENGINE=docker ./packaging/ubuntu/build.sh
#   ./packaging/ubuntu/build.sh --native   # 当前系统是 Debian/Ubuntu 时直接构建
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="$ROOT/dist"
mkdir -p "$DIST"

if [ "${1:-}" = "--native" ]; then
    echo "==> 原生构建 Ubuntu/Debian 包"
    export RUSTC_WRAPPER="$ROOT/scripts/rustc-remap.sh"
    cargo build --release -p lexicon-app
    OUT="$DIST" VERSION="$VERSION" packaging/ubuntu/package.sh "$ROOT/target/release/lexicon-dict"
    exit 0
fi

ENGINE="${ENGINE:-}"
if [ -z "$ENGINE" ]; then
    for e in podman docker; do
        if command -v "$e" >/dev/null 2>&1; then ENGINE="$e"; break; fi
    done
fi
[ -n "$ENGINE" ] || { echo "需要 podman 或 docker，或在 Ubuntu/Debian 上用 --native" >&2; exit 1; }

IMAGE="${IMAGE:-docker.io/library/ubuntu:22.04}"
OUT="$DIST/ubuntu-build"
case "$(uname -m)" in
    x86_64) ARCH=amd64 ;;
    aarch64|arm64) ARCH=arm64 ;;
    *) echo "暂不支持架构：$(uname -m)" >&2; exit 1 ;;
esac
TOOLCHAIN="${TOOLCHAIN:-$(dirname "$(dirname "$(rustup which cargo)")")}"
[ -x "$TOOLCHAIN/bin/cargo" ] || { echo "找不到 Rust 工具链：$TOOLCHAIN" >&2; exit 1; }
rm -rf "$OUT"
mkdir -p "$OUT"

# 复用宿主 cargo 缓存，避免每次干净容器都从 crates.io 重新下载。
# 若宿主没有缓存，退回到容器内联网下载。
HOST_CARGO="${CARGO_HOME:-$HOME/.cargo}"
CACHE_ARGS=()
CARGO_ENV=()
if [ -d "$HOST_CARGO/registry" ]; then
    CACHE_ARGS=(-v "$HOST_CARGO:/cargo")
    CARGO_ENV=(-e CARGO_HOME=/cargo -e CARGO_NET_OFFLINE="${CARGO_OFFLINE:-1}" -e RUSTC_WRAPPER=/src/scripts/rustc-remap.sh)
fi

echo "==> 在 $IMAGE 中构建（$ENGINE）"
"$ENGINE" run --rm --security-opt label=disable \
    -v "$ROOT:/src:ro" \
    -v "$OUT:/out:Z" \
    -v "$TOOLCHAIN:/toolchain:ro" \
    "${CACHE_ARGS[@]}" \
    "${CARGO_ENV[@]}" \
    -e VERSION="$VERSION" \
    -e ARCH="$ARCH" \
    "$IMAGE" bash -euxo pipefail -c '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        apt-get install -y --no-install-recommends \
            ca-certificates curl build-essential pkg-config \
            libx11-dev libxkbcommon-dev libwayland-dev \
            libgl1-mesa-dev libegl1-mesa-dev libfontconfig1-dev libfreetype-dev
        # 直接使用宿主机的 Rust 工具链（避免容器内再次下载 200MB）。
        export PATH=/toolchain/bin:$PATH
        export RUSTC=/toolchain/bin/rustc
        if [ -n "${CARGO_HOME:-}" ]; then
            export CARGO_HOME
        else
            export CARGO_HOME=/tmp/cargo
            export CARGO_NET_RETRY=5
        fi
        mkdir -p /work
        tar -C /src --exclude=./target --exclude=./dist --exclude=./.git -cf - . | tar -C /work -xf -
        cd /work
        cargo build --release -p lexicon-app
        VERSION="$VERSION" OUT=/out packaging/ubuntu/package.sh /work/target/release/lexicon-dict
    '

# rootless podman 通常已是宿主 UID；若以 root 运行容器，把产物交回当前用户。
if [ "$(id -u)" -ne 0 ]; then
    find "$OUT" -maxdepth 1 -type f -exec chown "$(id -u):$(id -g)" {} + 2>/dev/null || true
else
    chown -R "$(id -u):$(id -g)" "$OUT" 2>/dev/null || true
fi
cp "$OUT"/*.deb "$OUT"/*.tar.gz "$OUT"/SHA256SUMS.ubuntu "$DIST"/
echo "==> 完成："
ls -lh "$DIST"/*.deb "$DIST"/*ubuntu22.04*.tar.gz
