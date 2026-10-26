# Windows 11

## 使用

- 解压 `lexicon-dict-0.1.0-windows-x86_64.zip`。
- 双击 `lexicon-dict.exe`。
- 在「设置 → 词典目录」中添加 `.ld2` 所在目录，再点“扫描并转换”；默认没有源目录。
- zip 内 `portable` 标记：配置/缓存在 `data\`。
- 删除 `portable`：配置 `%APPDATA%\lexicon-dict\`，
  缓存 `%LOCALAPPDATA%\lexicon-dict\stardict\`。

## 实现

- 同一份 `lexicon-app` 源码用 `x86_64-pc-windows-gnu` 交叉编译；单 exe 约 6.0 MB。
- 字体：`C:\Windows\Fonts\segoeui.ttf` / `msyh.ttc`（缺失则退回内置字体）。
- 发音：SAPI `ISpVoice`，按 en-US/en-GB 选择声库（失败退回默认）。
- Wine 未安装 SAPI 语音包时 `v/V` 无声，这是 Wine 环境限制；真机 Windows 用系统语音。
- 无托盘、无全局热键；普通窗口 + 应用菜单/快捷方式启动。

## 验证（Wine 11）

- 单文件启动、字体、查询、内嵌图片、添加源目录后转换、截图均通过。
- 交叉编译命令：

```bash
cargo build --release --target x86_64-pc-windows-gnu -p lexicon-app
```

## 待真机验收

中文 IME、高 DPI（125%/150%）、多显示器、SAPI 声库、Windows Defender 首次启动。
