#!/bin/sh
# 用户级安装：单文件 app + 桌面菜单项（无需 root）。
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
PREFIX="${PREFIX:-$HOME/.local}"
if [ -x "$HERE/lexicon-dict" ]; then
    BIN="$HERE/lexicon-dict"
elif [ -x "$HERE/../target/release/lexicon-dict" ]; then
    BIN="$HERE/../target/release/lexicon-dict"
else
    echo "未找到 lexicon-dict 可执行文件" >&2
    exit 1
fi
install -d "$PREFIX/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/scalable/apps"
install -m 0755 "$BIN" "$PREFIX/bin/lexicon-dict"
install -m 0644 "$HERE/io.github.lexicon.dict.desktop" "$HOME/.local/share/applications/io.github.lexicon.dict.desktop"
install -m 0644 "$HERE/io.github.lexicon.dict.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.lexicon.dict.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "已安装：$PREFIX/bin/lexicon-dict"
echo "启动 lexicon-dict，在「设置 → 词典目录」中添加 .ld2 所在目录"
echo "从应用菜单或终端运行 lexicon-dict"
