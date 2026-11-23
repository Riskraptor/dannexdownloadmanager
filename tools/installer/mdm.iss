; Dannex Download Manager's Windows installer (Inno Setup 6). Built by tools/installer/build.ps1,
; which passes /DAppVersion=... and, when a certificate is set up, /DSign with /Smdmsign=...
;
; Install for the current user (no administrator rights, %LOCALAPPDATA%\Programs) or for
; everyone (Program Files); the wizard asks. Tasks: browser integration (native messaging host
; for Chrome, Edge, Brave and Firefox), .torrent files and magnet links, start with Windows, a
; desktop shortcut. A running DDM is asked to save and quit first (/exit). Uninstalling asks
; whether to keep the download list and settings; removing them also removes DDM's sign-ins from
; Windows Credential Manager (dannexdownloadmanager --forget-sign-ins).

#ifndef AppVersion
  #define AppVersion "0.5.2"
#endif
#define AppName "Dannex Download Manager"
#define AppExe "dannexdownloadmanager.exe"
#define HostExe "dannexdownloadmanager-nmhost.exe"
#define Root "..\.."
#define Bin Root + "\target\release"

[Setup]
; Never change AppId: upgrades and the uninstaller find the installation by it. Dannex has its
; own (not Mister Download Manager's), so the two install side by side.
AppId={{CF4A5FBF-E3D1-463D-9E89-4E26D222BC75}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Dannex Download Manager contributors
AppPublisherURL=https://github.com/Riskraptor/dannexdownloadmanager
AppSupportURL=https://github.com/Riskraptor/dannexdownloadmanager/issues
AppUpdatesURL=https://github.com/Riskraptor/dannexdownloadmanager/releases
AppCopyright=Copyright (C) 2026 Riskraptor and contributors
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; The Run entry is the installing user's, also when installing for everyone.
UsedUserAreasWarning=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
#if Ver >= EncodeVer(6, 6, 0)
; The wizard follows Windows' light or dark mode, with Dannex's own panels: the night sky and its
; comet beside the Welcome and Finished pages, the icon in the header. One of each per screen
; scale from 100 to 250%, Setup picks the nearest (tools/icon/build_icon.py draws them). No
; words in them: the wizard's text is translated.
WizardStyle=modern dynamic
; The Welcome page (off by default since Inno Setup 6) shows the panel first.
DisableWelcomePage=no
WizardImageFile={#Root}\tools\installer\wizard\side-*.png
WizardSmallImageFile={#Root}\tools\installer\wizard\small-*.png
#else
; Inno Setup before 6.6: no dark mode, nor these panels' sizes; its own pictures.
WizardStyle=modern
#endif
SetupIconFile={#Root}\crates\app\assets\icons\app\mdm.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir={#Root}\target\dist
OutputBaseFilename=DannexDownloadManager-{#AppVersion}-setup
; A 64 MB dictionary (#25): an 8 MB one (lzma2/max) only finds what repeats close by in the
; solid archive. A few MB smaller; building needs about 750 MB of memory, installing 64 MB.
Compression=lzma2/ultra64
SolidCompression=yes
ChangesAssociations=yes
; A running DDM is asked to quit by [Code] (it holds the mutex DannexDownloadManager.Running);
; AppMutex isn't used because it would refuse to start before that. Restart Manager is the
; fallback for anything else holding the files (a browser running the host).
CloseApplications=yes
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Dannex Download Manager contributors
VersionInfoDescription={#AppName} setup
VersionInfoProductName={#AppName}
#ifdef Sign
SignTool=mdmsign
SignedUninstaller=yes
  #define SignFlag "signonce"
#else
  #define SignFlag ""
#endif

; Every interface language (i18n/languages.tsv): tools/installer/languages.py writes the list and
; the translations of the messages below. The wizard starts in Windows' display language.
#include "languages.iss"

[CustomMessages]
lang_en.TaskBrowser=Set up &browser integration (Chrome, Edge, Brave, Firefox)
lang_en.TaskTorrents=Open .&torrent files and magnet links with {#AppName}
lang_en.TaskStartup=Start {#AppName} when &Windows starts
lang_en.TaskGroupOther=Other:
lang_en.Launch=Start {#AppName}
lang_en.KeepData=Also delete your download list and settings?%n%nYour downloaded files are not touched either way.

[Tasks]
Name: "browser"; Description: "{cm:TaskBrowser}"
Name: "torrents"; Description: "{cm:TaskTorrents}"
Name: "startup"; Description: "{cm:TaskStartup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:TaskGroupOther}"; Flags: unchecked

[Files]
Source: "{#Bin}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\{#HostExe}"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\dannexdownloadmanager-cli.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\dannexdownloadmanager-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
; The extension, for "Load unpacked" (Chromium) and about:debugging (Firefox) until it is in
; the stores.
Source: "{#Root}\target\extension\chromium\*"; DestDir: "{app}\extension\chromium"; Flags: ignoreversion recursesubdirs
Source: "{#Root}\target\extension\ddm-firefox.xpi"; DestDir: "{app}\extension"; Flags: ignoreversion
Source: "{#Root}\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "{#Root}\ATTRIBUTION.md"; DestDir: "{app}"; DestName: "ATTRIBUTION.txt"; Flags: ignoreversion

[InstallDelete]
; Firefox's add-on file under its former name (Dannex up to 0.6.5): today's is ddm-firefox.xpi.
Type: files; Name: "{app}\extension\mdm-firefox.xpi"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; The language chosen in the wizard ({language}: "lang_" + a languages.tsv code with "_" for "-",
; see languages.py inno_name). The app
; takes it on its first start, when it has no language setting yet (settings.rs), then deletes it.
Root: HKCU; Subkey: "Software\DannexDownloadManager"; ValueType: string; ValueName: "InstallLanguage"; ValueData: "{language}"; Flags: uninsdeletevalue uninsdeletekeyifempty
; Start with Windows: the same value Options -> "Launch on startup" writes and reads, with the
; argument that starts DDM in the notification area (#226; win32.rs startup_command).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "DannexDownloadManager"; ValueData: """{app}\{#AppExe}"" /startup"; Flags: uninsdeletevalue; Tasks: startup
; .torrent files and magnet links (HKA: the user's or everyone's classes, as installed).
Root: HKA; Subkey: "Software\Classes\DannexDownloadManager.torrent"; ValueType: string; ValueName: ""; ValueData: "Torrent file"; Flags: uninsdeletekey; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\DannexDownloadManager.torrent\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\DannexDownloadManager.torrent\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\.torrent"; ValueType: string; ValueName: ""; ValueData: "DannexDownloadManager.torrent"; Flags: uninsdeletekeyifempty; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\.torrent\OpenWithProgids"; ValueType: string; ValueName: "DannexDownloadManager.torrent"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: ""; ValueData: "URL:Magnet link"; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\magnet"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\magnet\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: torrents
Root: HKA; Subkey: "Software\Classes\magnet\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: torrents

[Run]
Filename: "{app}\{#HostExe}"; Parameters: "--register"; Flags: runhidden; Tasks: browser; Check: not IsAdminInstallMode
Filename: "{app}\{#HostExe}"; Parameters: "--register --all-users"; Flags: runhidden; Tasks: browser; Check: IsAdminInstallMode
Filename: "{app}\{#AppExe}"; Description: "{cm:Launch}"; Flags: nowait postinstall skipifsilent
; Update now in DDM runs this setup silently with /relaunch=1: DDM starts again once installed,
; as the user (not as the administrator an all-users update asked for).
Filename: "{app}\{#AppExe}"; Parameters: "/updated"; Flags: nowait runasoriginaluser; Check: Relaunch

[UninstallRun]
Filename: "{app}\{#HostExe}"; Parameters: "--unregister"; Flags: runhidden; RunOnceId: "UnregisterUser"; Check: not IsAdminInstallMode
Filename: "{app}\{#HostExe}"; Parameters: "--unregister --all-users"; Flags: runhidden; RunOnceId: "UnregisterAll"; Check: IsAdminInstallMode

[UninstallDelete]
Type: files; Name: "{app}\com.dannexdownloadmanager.host*.json"

[Code]
const
  RunningMutex = 'DannexDownloadManager.Running';

{ /relaunch=1 on the command line (DDM's Update now): start DDM again after installing. }
function Relaunch: Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

{ Ask a running DDM to save and quit, and wait up to 15 s for it. }
procedure ExitRunningApp(Exe: String);
var
  Code, Waited: Integer;
begin
  if CheckForMutexes(RunningMutex) and FileExists(Exe) then
  begin
    Exec(Exe, '/exit', '', SW_HIDE, ewWaitUntilTerminated, Code);
    Waited := 0;
    while CheckForMutexes(RunningMutex) and (Waited < 15000) do
    begin
      Sleep(250);
      Waited := Waited + 250;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  ExitRunningApp(ExpandConstant('{app}\{#AppExe}'));
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  ExitRunningApp(ExpandConstant('{app}\{#AppExe}'));
  Result := True;
end;

{ Remove a class key only while it still points at us (another program may have taken over
  magnet links or .torrent files since). }
procedure ForgetIfOurs(Key, Ours: String);
var
  Value: String;
begin
  if RegQueryStringValue(HKA, Key + '\shell\open\command', '', Value) and (Pos(Lowercase(Ours), Lowercase(Value)) > 0) then
    RegDeleteKeyIncludingSubkeys(HKA, Key);
end;

{ Asked before the files go (the sign-ins are removed by DDM itself), the list and settings
  removed after. A silent uninstall keeps both. }
var
  RemoveData: Boolean;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Value, Data: String;
  Code: Integer;
begin
  Data := ExpandConstant('{userappdata}\dannexdownloadmanager');
  if CurUninstallStep = usUninstall then
  begin
    ForgetIfOurs('Software\Classes\magnet', ExpandConstant('{app}\{#AppExe}'));
    if RegQueryStringValue(HKA, 'Software\Classes\.torrent', '', Value) and (Value = 'DannexDownloadManager.torrent') then
      RegDeleteValue(HKA, 'Software\Classes\.torrent', '');
    RemoveData := False;
    if DirExists(Data) and not UninstallSilent() then
      RemoveData := MsgBox(CustomMessage('KeepData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
    { The sign-ins DDM keeps in Windows Credential Manager (only DDM's own entries). }
    if RemoveData then
      Exec(ExpandConstant('{app}\{#AppExe}'), '--forget-sign-ins', '', SW_HIDE, ewWaitUntilTerminated, Code);
  end;
  if (CurUninstallStep = usPostUninstall) and RemoveData then
    DelTree(Data, True, True, True);
end;
