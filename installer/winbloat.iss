#define AppName "WinBloat"
#define AppPublisher "TamKungZ_"
#define AppVersion GetEnv("APP_VERSION")
#define AppExeName "winbloat.exe"

[Setup]
AppId={{2F760392-199C-4F6D-8FE1-0B4744A029B4}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/TamKungZ/WinBloat
AppSupportURL=https://github.com/TamKungZ/WinBloat/issues
AppUpdatesURL=https://github.com/TamKungZ/WinBloat/releases
DefaultDirName={autopf}\WinBloat
DefaultGroupName=WinBloat
UninstallDisplayIcon={app}\{#AppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog
LicenseFile=..\LICENSE
SetupIconFile=..\assets\icon-setup.ico
OutputDir=..\dist
OutputBaseFilename=winbloat-setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "..\target\release\winbloat.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\winbloat-gui.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\WinBloat"; Filename: "{app}\winbloat-gui.exe"; IconFilename: "{app}\winbloat-gui.exe"
Name: "{autodesktop}\WinBloat"; Filename: "{app}\winbloat-gui.exe"; IconFilename: "{app}\winbloat-gui.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\winbloat-gui.exe"; Description: "Launch WinBloat"; Flags: postinstall nowait skipifsilent
