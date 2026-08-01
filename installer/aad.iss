; Inno Setup script — AAD (Accelerated Architectural Drafting)
; -----------------------------------------------------------------------------
; Build:    iscc installer\aad.iss     ->  installer\Output\AAD-Setup-<ver>.exe
; Requires: Inno Setup 6+  (https://jrsoftware.org/isinfo.php)
; Prereq:   cargo build --release -p aadt-cli --features app
;           (installer\build.ps1 does the build + this step for you)
;
; Per-user install (no admin prompt): installs to the user's Programs folder,
; adds Start-Menu / optional desktop shortcuts that launch "AAD.exe app" with the
; working directory set to the install folder, bundles the runtime libraries\,
; and registers an uninstaller.

#define AppName "AAD"
#define AppFull "AAD - Accelerated Architectural Drafting"
#define AppVer "0.0.0"
#define AppPublisher "HolagundiWorks"
#define AppExe "AAD.exe"
#define SrcRoot ".."

[Setup]
AppId={{7E1D4B2A-6C3F-4A9E-B0D2-AAD000AAD000}
AppName={#AppFull}
AppVersion={#AppVer}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\AAD
DefaultGroupName=AAD
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
UninstallDisplayName={#AppFull}
UninstallDisplayIcon={app}\{#AppExe}
OutputDir=Output
OutputBaseFilename=AAD-Setup-{#AppVer}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile={#SrcRoot}\crates\aadt-app\assets\branding\favicon.ico

[Files]
; The desktop shell (the `aadt` CLI binary, installed as AAD.exe).
Source: "{#SrcRoot}\target\release\aadt.exe"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
; Runtime block / hatch / font libraries (loaded next to the executable).
Source: "{#SrcRoot}\libraries\*"; DestDir: "{app}\libraries"; Flags: recursesubdirs createallsubdirs ignoreversion
Source: "{#SrcRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "{#SrcRoot}\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Icons]
; The GUI launches via the `app` subcommand; WorkingDir anchors the libraries\ lookup.
Name: "{group}\AAD"; Filename: "{app}\{#AppExe}"; Parameters: "app"; WorkingDir: "{app}"
Name: "{group}\Uninstall AAD"; Filename: "{uninstallexe}"
Name: "{autodesktop}\AAD"; Filename: "{app}\{#AppExe}"; Parameters: "app"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Parameters: "app"; WorkingDir: "{app}"; Description: "Launch AAD"; Flags: nowait postinstall skipifsilent
