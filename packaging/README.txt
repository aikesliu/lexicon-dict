词典（lexicon-dict）—— 单文件桌面应用（Fedora / Debian / Ubuntu / Windows 11）
=====================================================

Linux
-----
Debian/Ubuntu 也可用同一份 install.sh；RPM 用 dnf，.deb 用 apt：
   sudo apt install ./lexicon-dict_0.1.0_amd64.deb
1. 运行 ./install.sh（用户级安装，无需 root）；或把 lexicon-dict 复制到 PATH。
2. 启动「词典」（或终端 lexicon-dict），在「设置 → 词典目录」中添加 .ld2 所在目录。
3. 点“扫描并转换”；默认不预设任何词典目录。
4. 卸载：./uninstall.sh。

Windows
-------
1. 双击 lexicon-dict.exe（zip 已带 portable 标记）。
2. 在「设置 → 词典目录」中添加 .ld2 所在目录，再点“扫描并转换”。
3. 若移走 portable 标记，则使用 %APPDATA%\lexicon-dict 与
   %LOCALAPPDATA%\lexicon-dict\stardict。

通用
----
- 无后台服务、无浏览器内核、无外部 UI 工具包；词典解析/查询/渲染全部在一个进程内。
- 配置文件：Linux ~/.config/lexicon-dict/{config.toml,gui.toml}；
  Windows 便携 %EXE%\data\，非便携 %APPDATA%\lexicon-dict\。
- 快捷键：j/k 移动、f/b 翻页、g/G 首尾、y 复制、v/V 发音、/ 输入、? 帮助。
