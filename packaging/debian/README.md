# Debian / Ubuntu 打包

预编译 `.deb` 使用 **Ubuntu 22.04** 作为构建基线（glibc 2.35），
因此可在 Ubuntu 22.04/24.04、Debian 12/13（amd64）上运行。

## 从 Fedora/任意有 podman 的机器构建

```bash
make ubuntu
# 产物：
#   dist/lexicon-dict_0.1.0_amd64.deb
#   dist/lexicon-dict-0.1.0-ubuntu22.04-amd64.tar.gz
```

脚本会启动 `ubuntu:22.04` 容器，安装 X11/Wayland/OpenGL 开发库，复用宿主机的
Rust 工具链，把二进制和 deb 一起产出。容器只用于链接 Ubuntu 22.04 的 glibc，
不会把宿主机的 target 目录带进去。默认复用宿主 `~/.cargo` 缓存并离线构建；
缓存不完整时可用 `CARGO_OFFLINE=0 make ubuntu` 允许容器联网。

## 在 Ubuntu/Debian 上原生构建

```bash
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev \
    libwayland-dev libgl1-mesa-dev libegl1-mesa-dev libfontconfig1-dev libfreetype-dev
./packaging/ubuntu/build.sh --native
```

## 安装

```bash
sudo apt install ./lexicon-dict_0.1.0_amd64.deb
lexicon-dict
```

依赖：`libc6 (>= 2.35)`、`libgcc-s1`、X11（Xcursor/Xrandr/Xi/Xinerama/Xxf86vm）、
xkbcommon、Wayland client、OpenGL/EGL、`hicolor-icon-theme`。桌面菜单项和图标
安装在 `/usr/share/applications` 与 `/usr/share/icons/hicolor`。

> 仅 x86_64 预编译；aarch64 可在对应 Ubuntu 机器上执行 `--native`。
