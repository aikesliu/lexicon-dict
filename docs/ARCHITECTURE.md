# 架构

## 目标

一个可执行文件完成全部工作：读取 `.ld2`、转换 StarDict 缓存、mmap 查询、清洗 markup、
渲染 UI、发音、设置。**没有 daemon、没有 GNOME 扩展、没有浏览器内核。**

- Linux：`lexicon-dict`（eframe/egui + glow，Wayland/X11）
- Windows 11：`lexicon-dict.exe`（同一份 UI 代码，WGL）

## 数据流

```
*.ld2 ──lexicon-core::ld2──► entries
      ──stardict::writer──► <cache>/<slug>/{.ifo,.idx,.dict,res/}
      ──manifest.json─────► 指纹 / 元信息

Query ──► DictSet（order/enabled）──► StarDict mmap 查找
      ──► markup::to_pango / to_blocks ──► egui 富文本 / 图片
```

## 进程模型

```
lexicon-dict (单进程)
  ├─ eframe/egui 主线程：窗口、输入、绘制、设置
  ├─ lexicon-core（进程内）：DictSet、查询、转换
  ├─ 转换线程：仅在用户手动“扫描并转换”时存在，channel 回报进度
  ├─ 图片纹理缓存：PNG 解码 → egui TextureHandle
  └─ 发音：Linux 调 espeak-ng + pw-play；Windows 调 SAPI（同进程）
```

查询是同步的（25 本约 10–20 ms），不需要前后端协议；转换放到工作线程避免卡 UI。
启动时 `DictSet::reload` 只 mmap `.idx`，首次查询按需触页。

## 代码结构

```
crates/lexicon-core/
  ld2/             .ld2 header/streams/zlib/entries/crossRefs/编码/资源
  stardict/        StarDict 3.0.6 读写、idx 采样 + 二分、mmap
  query.rs         精确/大小写/前缀 + SequenceMatcher 风格 score
  markup.rs        Pango 清洗、to_blocks（文本/图片块）
  convert.rs       ld2 → StarDict（rayon）
  config.rs/manifest.rs/paths.rs
crates/lexicon-app/
  main.rs          窗口、布局、键盘、设置、截图测试钩子
  config.rs        核心 config.toml + gui.toml、便携路径
  query.rs         进程内 Engine（加载/查询/块整理）
  rich.rs          Pango → egui LayoutJob
  fonts.rs         系统 Cantarell/Droid 或 Segoe UI/微软雅黑
  speech.rs        espeak-ng/pw-play、SAPI
crates/lexicon-cli/  转换/诊断 CLI（可选，不随 app 安装）
```

## UI 设计

- 深/浅两套 palette；背景/卡片/边框/强调色集中定义。
- 卡片：圆角 14、1 px 边框、选中金色描边 + 左侧 accent bar；词头 21 px 粗体，
  正文 16 px（可配），词典名 11.5 px 弱化；US/UK/复制为低对比 pill。
- 富文本：`b/i/u/span foreground` 映射到 egui `LayoutJob`，支持选择与 `Ctrl+C`。
- 图片：按块顺序内联，最长边 120 px，圆角 6。
- 键盘：查询后焦点交给结果区（`/` 回输入框），因此 j/k 不会写进搜索框。

## 兼容性

- 转换产物与既有实现逐字节一致，可直接复用已有 StarDict 缓存
  `~/.cache/lexicon-dict/stardict`。
- 核心配置 `config.toml` 字段沿用；GUI 偏好另存 `gui.toml`。
- Pango markup 与 Python 版抽样一致（Wikipedia 例外：原实现截断，本实现保留内容）。

## 打包

- Linux：tar.gz（二进制 + desktop + icon + install.sh）与 RPM（单个二进制 + desktop/icon）。
- Debian/Ubuntu：在 Ubuntu 22.04 容器内构建的 `.deb`（amd64）与同基线 tar.gz；二进制动态依赖基线为 glibc 2.35。
- Windows：zip（exe + README + portable 标记）；另附 Inno Setup 脚本。
- 二进制静态链接 Rust 依赖，动态依赖仅 glibc 与图形/输入系统库（Mesa/Wayland/X11）。
