#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef CoreVersion
  #define CoreVersion "0.1.0"
#endif
#ifndef SourceRoot
  #define SourceRoot ".."
#endif

[Setup]
AppId={{84E4C0ED-D995-4CA1-B15C-E75A49FDC657}
AppName=Tiny MD
AppVersion={#AppVersion}
AppPublisher=ynx-official
AppPublisherURL=https://github.com/ynx-official/tiny-md
VersionInfoVersion={#CoreVersion}
DefaultDirName={localappdata}\Programs\Tiny MD
DefaultGroupName=Tiny MD
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SourceDir={#SourceRoot}
OutputDir=target\release-assets
OutputBaseFilename=tiny-md-v{#AppVersion}-windows-x64-setup
SetupIconFile=assets\icons\tiny-md.ico
UninstallDisplayIcon={app}\tiny-md.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "defaultmd"; Description: "将 .md 文件默认使用 Tiny MD 打开（安装后在 Windows 中确认）"; GroupDescription: "默认打开方式："; Flags: unchecked
Name: "defaultmarkdown"; Description: "将 .markdown 文件默认使用 Tiny MD 打开（安装后在 Windows 中确认）"; GroupDescription: "默认打开方式："; Flags: unchecked

[Files]
Source: "target\windows\Tiny MD\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Tiny MD"; Filename: "{app}\tiny-md.exe"
Name: "{autodesktop}\Tiny MD"; Filename: "{app}\tiny-md.exe"; Tasks: desktopicon

[Registry]
; Only remove application-owned keys and values; extension defaults belong to Windows.
Root: HKCU; Subkey: "Software\Classes\TinyMD.Markdown"; ValueType: string; ValueData: "Markdown 笔记"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\TinyMD.Markdown\DefaultIcon"; ValueType: string; ValueData: """{app}\tiny-md.exe"",0"
Root: HKCU; Subkey: "Software\Classes\TinyMD.Markdown\shell\open"; ValueType: string; ValueData: "通过 Tiny MD 打开"
Root: HKCU; Subkey: "Software\Classes\TinyMD.Markdown\shell\open\command"; ValueType: string; ValueData: """{app}\tiny-md.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "TinyMD.Markdown"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKCU; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: string; ValueName: "TinyMD.Markdown"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKCU; Subkey: "Software\Classes\Applications\tiny-md.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Tiny MD"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\tiny-md.exe\DefaultIcon"; ValueType: string; ValueData: """{app}\tiny-md.exe"",0"
Root: HKCU; Subkey: "Software\Classes\Applications\tiny-md.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\tiny-md.exe\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\tiny-md.exe\shell\open\command"; ValueType: string; ValueData: """{app}\tiny-md.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.md\shell\TinyMD.Open"; ValueType: string; ValueData: "通过 Tiny MD 打开"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.md\shell\TinyMD.Open"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\tiny-md.exe"",0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.md\shell\TinyMD.Open"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Single"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.md\shell\TinyMD.Open\command"; ValueType: string; ValueData: """{app}\tiny-md.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.markdown\shell\TinyMD.Open"; ValueType: string; ValueData: "通过 Tiny MD 打开"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.markdown\shell\TinyMD.Open"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\tiny-md.exe"",0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.markdown\shell\TinyMD.Open"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Single"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.markdown\shell\TinyMD.Open\command"; ValueType: string; ValueData: """{app}\tiny-md.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Tiny MD\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Tiny MD"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Tiny MD\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "阅读和编辑 Markdown 笔记"
Root: HKCU; Subkey: "Software\Tiny MD\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: """{app}\tiny-md.exe"",0"
Root: HKCU; Subkey: "Software\Tiny MD\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "TinyMD.Markdown"
Root: HKCU; Subkey: "Software\Tiny MD\Capabilities\FileAssociations"; ValueType: string; ValueName: ".markdown"; ValueData: "TinyMD.Markdown"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Tiny MD"; ValueData: "Software\Tiny MD\Capabilities"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\tiny-md.exe"; Description: "Launch Tiny MD"; Flags: nowait postinstall skipifsilent unchecked

[Code]
#include "windows-update-compat.iss"

function WantsDefaultApps: Boolean;
begin
  Result := WizardIsTaskSelected('defaultmd') or WizardIsTaskSelected('defaultmarkdown');
end;

function DefaultAppsSettings(Param: String): String;
var
  Version: TWindowsVersion;
begin
  GetWindowsVersionEx(Version);
  if Version.Build >= 22000 then
    Result := 'ms-settings:defaultapps?registeredAppUser=Tiny%20MD'
  else
    Result := 'ms-settings:defaultapps';
end;

procedure CurPageChanged(CurPageID: Integer);
var
  Extensions: String;
begin
  if (CurPageID = wpFinished) and WantsDefaultApps then begin
    Extensions := '';
    if WizardIsTaskSelected('defaultmd') then
      Extensions := '.md';
    if WizardIsTaskSelected('defaultmarkdown') then begin
      if Extensions <> '' then
        Extensions := Extensions + '、';
      Extensions := Extensions + '.markdown';
    end;
    WizardForm.FinishedLabel.Caption := '安装完成。点击“完成”后，请在 Windows 默认应用设置中将 ' +
      Extensions + ' 设为使用 Tiny MD 打开。';
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ErrorCode: Integer;
begin
  { Wait until the installer has refreshed associations and the user clicks Finish. }
  if (CurStep = ssDone) and (not WizardSilent) and WantsDefaultApps then begin
    if not ShellExec('open', DefaultAppsSettings(''), '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode) then begin
      Log('Could not open Default Apps settings: ' + IntToStr(ErrorCode));
      MsgBox('无法打开默认应用设置。请在 Windows 设置中搜索“默认应用”，然后选择 Tiny MD。', mbInformation, MB_OK);
    end;
  end;
end;
