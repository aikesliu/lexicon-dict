# 后续工作

## 必做验收

- [ ] 真实 Windows 11：中文 IME、125%/150% DPI、多显示器、SAPI 声库。
- [ ] 真实 Fedora GNOME Wayland：中文 IME（fcitx5）、触摸板滚动、剪贴板。
- [ ] 大词典（60 万条）下滚动/内存曲线；必要时做结果虚拟化或按需加载。

## 可选增强

- [ ] 全局热键（Windows RegisterHotKey；Linux X11 XGrabKey，Wayland 走 portal）。
- [ ] 托盘/最小化到后台。
- [ ] 设置里的源目录用原生文件选择器（会引入平台依赖，需权衡单二进制）。
- [ ] 结果卡片复制按钮的 toast 提示、发音失败提示。
- [ ] 浅色主题下图片边框的自适应微调。
- [ ] 应用内检查更新（可选，纯 HTTP，无浏览器）。
