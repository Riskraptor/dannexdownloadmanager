; Dannex Download Manager's Windows installer (Inno Setup 6). Built by tools/installer/build.ps1,
; which passes /DAppVersion=... and, when a certificate is set up, /DSign with /Smdmsign=...
;
; Install for the current user (no administrator rights, %LOCALAPPDATA%\Programs) or for
; everyone (Program Files); the wizard asks. Tasks: browser integration (native messaging host
; for Chrome, Edge, Brave and Firefox), .torrent files and magnet links, start with Windows, a
; desktop shortcut. A running DDM is asked to save and quit first (/exit). Once the files are in,
; the wizard gets the video helpers (yt-dlp, ffmpeg) under one generic line, "Finishing
; installation...", with a progress bar: the installed program's --install-helpers downloads and
; checks them as DDM always does (crates/app/src/autohelpers.rs). They can't be skipped and are
; never named or explained (the owner's wish); offline, or when a download fails, the setup
; finishes with one short note and DDM gets them by itself once it's online. The Finished page can open DDM's "Add Dannex
; Download Manager to your browser" window (the extension is in the Chrome Web Store).
; Uninstalling asks whether to keep the download list and settings; removing them also removes
; DDM's sign-ins from Windows Credential Manager (dannexdownloadmanager --forget-sign-ins).
;
; The wizard speaks plainly (the owner's wish, 2026-11-27): what is installed, the few choices,
; done. No Ready page, no free-space line, no file names while copying; [Code] trims Inno's own
; texts where they say more than a person needs.

#ifndef AppVersion
  #define AppVersion "0.5.2"
#endif
#define AppName "Dannex Download Manager"
#define AppExe "dannexdownloadmanager.exe"
#define HostExe "dannexdownloadmanager-nmhost.exe"
#define Root "..\.."
; The release programs: cargo's target folder (build.ps1 passes /DBin= when CARGO_TARGET_DIR
; puts it elsewhere).
#ifndef Bin
  #define Bin Root + "\target\release"
#endif

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
; The choices page goes straight to Install: the Ready page only repeated them.
DisableReadyPage=yes
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
lang_en.ChoicesTitle=A few choices
lang_en.ChoicesDesc={#AppName} can set these up for you.
lang_en.ChoicesLabel=Tick what you want, then click %1.
lang_en.ToolsLater=Some parts will finish setting up the next time you're online.
lang_en.Launch=Start {#AppName}
lang_en.AddExtension=Add {#AppName} to my &browser
lang_en.KeepData=Also delete your download list and settings?%n%nYour downloaded files are not touched either way.

[Tasks]
Name: "browser"; Description: "{cm:TaskBrowser}"
Name: "torrents"; Description: "{cm:TaskTorrents}"
Name: "startup"; Description: "{cm:TaskStartup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "{#Bin}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\{#HostExe}"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\dannexdownloadmanager-cli.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
Source: "{#Bin}\dannexdownloadmanager-mcp.exe"; DestDir: "{app}"; Flags: ignoreversion {#SignFlag}
; The extension for a browser that can't use its store ("Load unpacked"), and Firefox's add-on
; file until addons.mozilla.org lists it; Chrome, Edge, Brave, Opera and Vivaldi take it from the
; Chrome Web Store.
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
; DDM's "Add Dannex Download Manager to your browser" window (the Chrome Web Store's page in each
; browser it finds), until a browser's extension has reached DDM.
Filename: "{app}\{#AppExe}"; Parameters: "/show extension"; Description: "{cm:AddExtension}"; Flags: nowait postinstall skipifsilent; Tasks: browser; Check: ExtensionMissing
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

{ No browser's extension has reached DDM yet (the native host writes this file once one has). }
function ExtensionMissing: Boolean;
begin
  Result := not FileExists(ExpandConstant('{userappdata}\dannexdownloadmanager\extension-seen'));
end;

{ ---- The video helpers, got as the setup installs (crates/app/src/autohelpers.rs) ------------ }

var
  { They couldn't all be got now: the Finished page says DDM gets them itself. }
  ToolsLater: Boolean;

{ One line of the program's --install-helpers output: "get <name> <k> <n>", "progress <0-1000>",
  "ready <name> <version>", "failed <name> <why>" (autohelpers.rs, Line). Only the bar shows it:
  the line above it stays generic, and the names go to the setup's log. }
procedure HelperLine(const S: String; const Error, FirstLine: Boolean);
begin
  if Copy(S, 1, 9) = 'progress ' then
    WizardForm.ProgressGauge.Position := StrToIntDef(Trim(Copy(S, 10, 8)), WizardForm.ProgressGauge.Position)
  else
  begin
    Log('The video helpers: ' + S);
    if Copy(S, 1, 7) = 'failed ' then
      ToolsLater := True;
  end;
end;

{ After the files: the installed program gets yt-dlp and ffmpeg into the user's data folder and
  checks them (yt-dlp's signed checksum list, ffmpeg's published checksums), while the bar shows
  how far it is, under Inno's own "Finishing installation..." (no names, no explanation). Installed for everyone, the setup runs as an administrator: the program then
  runs as the user who started the setup (their data folder), and the bar only moves. A silent
  setup (DDM's own Update now) leaves anything missing to DDM, which gets it in the background. }
procedure GetVideoTools;
var
  Exe: String;
  Code: Integer;
  Ran: Boolean;
begin
  if WizardSilent then
    Exit;
  Exe := ExpandConstant('{app}\{#AppExe}');
  WizardForm.StatusLabel.Caption := SetupMessage(msgStatusRunProgram);
  WizardForm.FilenameLabel.Caption := '';
  WizardForm.ProgressGauge.Min := 0;
  WizardForm.ProgressGauge.Max := 1000;
  WizardForm.ProgressGauge.Position := 0;
  Code := -1;
  if IsAdminInstallMode then
  begin
    WizardForm.ProgressGauge.Style := npbstMarquee;
    Ran := ExecAsOriginalUser(Exe, '--install-helpers', '', SW_HIDE, ewWaitUntilTerminated, Code);
    WizardForm.ProgressGauge.Style := npbstNormal;
  end
  else
  begin
    try
      Ran := ExecAndLogOutput(Exe, '--install-helpers', '', SW_HIDE, ewWaitUntilTerminated, Code, @HelperLine);
    except
      Ran := False;
      Log('The video helpers: ' + GetExceptionMessage);
    end;
  end;
  if not Ran or (Code <> 0) then
    ToolsLater := True;
  Log(Format('The video helpers: exit code %d', [Code]));
  WizardForm.ProgressGauge.Position := WizardForm.ProgressGauge.Max;
  WizardForm.FilenameLabel.Caption := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    GetVideoTools;
end;

{ ---- Plain words: Inno's own texts trimmed to what a person needs ---------------------------- }

{ A button's caption as the text names it: without its access key ("&Install", or "(&I)" after
  the word in Chinese, Japanese and Korean). }
function ButtonWord(S: String): String;
var
  P: Integer;
begin
  P := Pos('(&', S);
  if (P > 0) and (Length(S) >= P + 3) and (S[P + 3] = ')') then
    Delete(S, P, 4);
  StringChangeEx(S, '&', '', True);
  Result := Trim(S);
end;

procedure InitializeWizard;
var
  S: String;
  P: Integer;
begin
  { "This will install ... on your computer." alone, without "close all other applications"
    (the setup closes DDM itself): the message's first paragraph, in every language. }
  S := WizardForm.WelcomeLabel2.Caption;
  P := Pos(#13#10#13#10, S);
  if P > 0 then
  begin
    WizardForm.WelcomeLabel2.Caption := Copy(S, 1, P - 1);
    WizardForm.AdjustLabelHeight(WizardForm.WelcomeLabel2);
  end;
  { The choices page: what to tick, not "additional tasks"; then Install, as its button says
    (no Ready page after it). }
  WizardForm.SelectTasksLabel.Caption := FmtMessage(CustomMessage('ChoicesLabel'), [ButtonWord(SetupMessage(msgButtonInstall))]);
  P := WizardForm.AdjustLabelHeight(WizardForm.SelectTasksLabel);
  WizardForm.TasksList.Top := WizardForm.TasksList.Top + P;
  WizardForm.TasksList.Height := WizardForm.TasksList.Height - P;
end;

procedure CurPageChanged(CurPageID: Integer);
var
  S: String;
  D: Integer;
begin
  case CurPageID of
    { Where it goes, without the free space it needs (a few hundred MB, never the question). }
    wpSelectDir:
      WizardForm.DiskSpaceLabel.Visible := False;
    { The last page before installing (no Ready page): its button installs, and says so. }
    wpSelectTasks:
      begin
        WizardForm.PageNameLabel.Caption := CustomMessage('ChoicesTitle');
        WizardForm.PageDescriptionLabel.Caption := CustomMessage('ChoicesDesc');
        WizardForm.NextButton.Caption := SetupMessage(msgButtonInstall);
      end;
    { The bar, not the names of the files being copied. }
    wpInstalling:
      WizardForm.FilenameLabel.Visible := False;
    wpFinished:
      begin
        { "Setup has finished installing ... on your computer.", without "The application may be
          launched by selecting the installed shortcuts"; and, when the video helpers couldn't
          be got now, that some parts finish once online (DDM gets them itself). }
        S := SetupMessage(msgFinishedLabelNoIcons);
        StringChangeEx(S, '[name]', '{#AppName}', True);
        if ToolsLater then
          S := S + #13#10#13#10 + CustomMessage('ToolsLater');
        WizardForm.FinishedLabel.Caption := S;
        D := WizardForm.AdjustLabelHeight(WizardForm.FinishedLabel);
        WizardForm.RunList.Top := WizardForm.RunList.Top + D;
        WizardForm.RunList.Height := WizardForm.RunList.Height - D;
      end;
  end;
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
