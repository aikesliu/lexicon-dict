; Inno Setup 6 脚本（在 Windows 上编译）。
#define MyAppName "词典"
#define MyAppVersion "0.1.0"
#define MyAppExe "lexicon-dict.exe"

[Setup]
AppId={{7C0E9D0A-2B7E-4B9A-9F2E-6C3B1D5A8E10}
AppName={#MyAppName}
SetupIconFile=..\..\crates\lexicon-app\assets\icon.ico
AppVersion={#MyAppVersion}
DefaultDirName={autopf}\Lexicon Dict
DefaultGroupName={#MyAppName}
OutputDir=..\..\dist
OutputBaseFilename=lexicon-dict-{#MyAppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#MyAppExe}
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "..\..\dist\lexicon-dict-{#MyAppVersion}-windows-x86_64\lexicon-dict.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\lexicon-dict-{#MyAppVersion}-windows-x86_64\README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务："

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "启动 {#MyAppName}"; Flags: nowait postinstall skipifsilent
