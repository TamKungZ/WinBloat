#define AppName "WinBloat"
#define AppPublisher "TamKungZ_"
#define AppVersion GetEnv("APP_VERSION")
#define AppArchitecture GetEnv("APP_ARCH")
#define CliPath GetEnv("CLI_PATH")
#define GuiPath GetEnv("GUI_PATH")

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
UninstallDisplayIcon={app}\winbloat-gui.exe
PrivilegesRequired=admin
LicenseFile=..\LICENSE
SetupIconFile=..\assets\icon-setup.ico
OutputDir=..\dist
OutputBaseFilename=winbloat-setup-{#AppVersion}-{#AppArchitecture}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

#if AppArchitecture == "x64"
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#elif AppArchitecture == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x86compatible and not x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "{#CliPath}"; DestDir: "{app}"; DestName: "winbloat.exe"; Flags: ignoreversion
Source: "{#GuiPath}"; DestDir: "{app}"; DestName: "winbloat-gui.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\WinBloat CLI"; Filename: "{sys}\cmd.exe"; Parameters: "/K """"{app}\winbloat.exe"" --help"""; IconFilename: "{app}\winbloat.exe"
Name: "{group}\WinBloat GUI"; Filename: "{app}\winbloat-gui.exe"; IconFilename: "{app}\winbloat-gui.exe"
Name: "{autodesktop}\WinBloat CLI"; Filename: "{sys}\cmd.exe"; Parameters: "/K """"{app}\winbloat.exe"" --help"""; IconFilename: "{app}\winbloat.exe"; Tasks: desktopicon

[Run]
Filename: "{sys}\cmd.exe"; Parameters: "/K """"{app}\winbloat.exe"" --help"""; Description: "Open the WinBloat CLI"; Flags: postinstall nowait skipifsilent

[Code]
const
  EnvironmentKey = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';
  WM_SETTINGCHANGE = $001A;
  SMTO_ABORTIFHUNG = $0002;

function SendMessageTimeout(hWnd: LongWord; Msg: LongWord; wParam: LongWord;
  lParam: string; fuFlags: LongWord; uTimeout: LongWord;
  var lpdwResult: LongWord): LongWord;
  external 'SendMessageTimeoutW@user32.dll stdcall';

procedure BroadcastEnvironmentChange;
var
  ResultCode: LongWord;
begin
  SendMessageTimeout(HWND_BROADCAST, WM_SETTINGCHANGE, 0, 'Environment',
    SMTO_ABORTIFHUNG, 5000, ResultCode);
end;

function PathContains(PathValue, Entry: string): Boolean;
var
  SeparatorPos: Integer;
  Part: string;
  IsLastPart: Boolean;
begin
  while True do
  begin
    SeparatorPos := Pos(';', PathValue);
    IsLastPart := SeparatorPos = 0;
    if IsLastPart then
      Part := PathValue
    else
    begin
      Part := Copy(PathValue, 1, SeparatorPos - 1);
      Delete(PathValue, 1, SeparatorPos);
    end;
    if CompareText(Trim(Part), Entry) = 0 then
    begin
      Result := True;
      Exit;
    end;
    if IsLastPart then
      Break;
  end;
  Result := False;
end;

function RemovePathEntry(PathValue, Entry: string): string;
var
  SeparatorPos: Integer;
  Part: string;
  IsLastPart: Boolean;
  FirstPart: Boolean;
begin
  Result := '';
  FirstPart := True;
  while True do
  begin
    SeparatorPos := Pos(';', PathValue);
    IsLastPart := SeparatorPos = 0;
    if IsLastPart then
      Part := PathValue
    else
    begin
      Part := Copy(PathValue, 1, SeparatorPos - 1);
      Delete(PathValue, 1, SeparatorPos);
    end;
    if CompareText(Trim(Part), Entry) <> 0 then
    begin
      if not FirstPart then
        Result := Result + ';';
      Result := Result + Part;
      FirstPart := False;
    end;
    if IsLastPart then
      Break;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  SystemPath: string;
  PathRoot: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    if IsWin64 then
      PathRoot := HKLM64
    else
      PathRoot := HKLM;
    if not RegQueryStringValue(PathRoot, EnvironmentKey, 'Path', SystemPath) then
      SystemPath := '';
    if not PathContains(SystemPath, ExpandConstant('{app}')) then
    begin
      if SystemPath <> '' then
        SystemPath := SystemPath + ';';
      SystemPath := SystemPath + ExpandConstant('{app}');
      if not RegWriteExpandStringValue(PathRoot, EnvironmentKey, 'Path', SystemPath) then
        RaiseException('Could not add the WinBloat installation folder to the system PATH.');
      BroadcastEnvironmentChange;
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  SystemPath: string;
  NewPath: string;
  PathRoot: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    if IsWin64 then
      PathRoot := HKLM64
    else
      PathRoot := HKLM;
    if RegQueryStringValue(PathRoot, EnvironmentKey, 'Path', SystemPath) then
    begin
      NewPath := RemovePathEntry(SystemPath, ExpandConstant('{app}'));
      if NewPath <> SystemPath then
      begin
        if RegWriteExpandStringValue(PathRoot, EnvironmentKey, 'Path', NewPath) then
          BroadcastEnvironmentChange
        else
          MsgBox('Could not remove the WinBloat installation folder from the system PATH.',
            mbError, MB_OK);
      end;
    end;
  end;
end;
