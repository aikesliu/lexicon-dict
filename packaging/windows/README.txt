词典（Windows 11，单文件免安装）
========================================

1. 双击 lexicon-dict.exe。
2. 在「设置 → 词典目录」中添加 .ld2 所在目录，再点“扫描并转换”。
3. 快捷键：j/k 移动、f/b 翻页、g/G 首尾、y 复制、v/V 发音、/ 输入、? 帮助。
4. 删除 portable 标记则改用 %APPDATA%\lexicon-dict 与
   %LOCALAPPDATA%\lexicon-dict\stardict。

无后台服务、无浏览器内核、无外部 UI 运行时；一个 exe 完成解析、查询、渲染与发音。

注意：Wine 自带的 SAPI 语音是占位实现，v/V 通常无声；真机 Windows 使用系统语音。
