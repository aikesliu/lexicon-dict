#!/bin/sh
set -eu
PREFIX="${PREFIX:-$HOME/.local}"
rm -f "$PREFIX/bin/lexicon-dict"
rm -f "$HOME/.local/share/applications/io.github.lexicon.dict.desktop"
rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.lexicon.dict.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "已卸载 app；词典与配置保留在 ~/.config/lexicon-dict 和 ~/.cache/lexicon-dict"
