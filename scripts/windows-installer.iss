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

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "target\windows\Tiny MD\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Tiny MD"; Filename: "{app}\tiny-md.exe"
Name: "{autodesktop}\Tiny MD"; Filename: "{app}\tiny-md.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\tiny-md.exe"; Description: "Launch Tiny MD"; Flags: nowait postinstall skipifsilent unchecked
