#!/bin/sh
# 构建时把源码根目录与构建机 HOME 重映射到 /build，
# 避免发布二进制里出现真实用户目录、项目父目录或绝对源码路径。
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P 2>/dev/null || true)
args=""
if [ -n "${HOME:-}" ]; then
    args="--remap-path-prefix=$HOME=/build/.home"
fi
if [ -n "${ROOT:-}" ]; then
    args="$args --remap-path-prefix=$ROOT=/build/src"
fi
# shellcheck disable=SC2086
exec "$@" $args
