# Update now, end to end: Help -> Check for updates -> Update now, checked the way a user's update
# goes. This header is the guide to tools/lab/update/; this script is the Windows runner (the
# portable zip path), upd-lin.sh the Linux one. The runners:
#   1. find a newer release;
#   2. download it and check its signed checksum list;
#   3. show "Installing the update...";
#   4. put it in place;
#   5. start the new copy (/updated) while the old one closes.
# They also check that DDM refuses a release whose checksum is wrong or whose list is signed by
# another key, and that closing the window half-way starts nothing.
#
# Nothing here touches the installed DDM or the real releases:
# - The test copies: each run starts a copy of the given build in a folder of its own,
#   off-screen (MDM_OFFSCREEN=1).
# - The releases: fake 9.9.9 ones made by mkrel.py, served by the lab server on 127.0.0.1
#   (--files). Their latest.json is the release feed (MDM_UPDATE_FEED).
# - The keys: the checksum lists are signed with two throwaway Ed25519 keys made for each run in
#   its work folder, "test" and "other". DDM's release key is never read or needed.
#
# The build: these checks need the `test-update` cargo feature. It is off by default, and no
# release build turns it on. It adds:
# - MDM_TEST_UPDATE_KEY: a public key the release's checksum list may be signed with, besides
#   DDM's own;
# - /show update-now: Update now pressed, with no clicks, in the window /show updates opens (that
#   one every build has);
# - MDM_TEST_UPDATE_HOLD=N: holds "Installing the update..." for N seconds.
# The build must be release mode. A debug build is "built from source" (Install::detect), and
# those never update.
#   cargo build --release -p mdm-app --features test-update -p mdm-labserver          # Windows
#   cargo build --profile vmtest -p mdm-app --features test-update -p mdm-labserver   # Linux (mdm-lab)
#
# Windows, the portable zip:
#   tools\lab\update\upd-win.ps1 -Exe target\release\dannexdownloadmanager.exe -Server target\release\labserver.exe
#   ... -Only good,bad-hash         some scenarios
#   ... -Shots -ShotDir <folder>    also Download complete and "Installing the update..." in
#                                   en, de, ar, light and dark (the scenarios still run first),
#                                   with PrintWindow (tools\lab\cap-own.ps1)
# Needs Python 3 and gpg (Git for Windows has one). The work folder is target\update-e2e (or
# -Work), emptied first: the copies, releases, keys and log.txt are there. Each copy is portable
# (data\ beside its program), so Install::PortableZip. The windows open off the screens and
# nothing is clicked: closing a window is its title bar's WM_CLOSE.
#
# Linux, the .tar.gz and the AppImage: upd-lin.sh (its header says how). macOS (the .dmg:
# hdiutil, ditto) has no runner yet: build and clippy only.
#
# Scenarios and what each must show. Each runner prints PASS or FAIL per scenario and exits with
# 1 when any fails. Windows' name first, then Linux's:
#   good (targz-good, appimage-good)
#       downloaded, checked, put in place; the new copy started with /updated; the old one ended
#   bad-hash (targz-bad-hash)
#       refused ("the downloaded program doesn't match its signed checksum"); nothing replaced,
#       nothing started
#   other-key (appimage-other-key)
#       refused ("the release's checksum list is not signed by its publisher's key"); nothing
#       replaced or started
#   close-download (targz-close-download)
#       the update window closed while it downloads (the asset served at 1 MB/s by
#       slowserve.py); nothing replaced or started
#   close-install (appimage-close-install)
#       the window closed during "Installing the update..." (held 8 s): the files are put in
#       place, since that can't stop half-way, but nothing is started; the running copy stays and
#       the next start is the new version
#
# The files:
# - mkrel.py: makes a fake release (the asset, SHA256SUMS.txt and its signature, latest.json). On
#   its first use in a work folder it also makes the throwaway keys there.
# - slowserve.py: serves a folder slowly (the cancel-while-downloading scenario).
# - upd-win.ps1, upd-lin.sh: the runners.
param(
  [Parameter(Mandatory)][string]$Exe,
  [Parameter(Mandatory)][string]$Server,
  [string]$Work = "",
  [string[]]$Only = @(),
  [switch]$Shots,
  [string]$ShotDir = "",
  [int]$Port = 8298,
  [int]$SlowPort = 8297
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$Exe = (Resolve-Path $Exe).Path
$Server = (Resolve-Path $Server).Path
if (-not $Work) { $Work = Join-Path $root "target\update-e2e" }
if (Test-Path $Work) { Remove-Item -Recurse -Force $Work }
New-Item -ItemType Directory -Force $Work | Out-Null
if ($Shots -and -not $ShotDir) { $ShotDir = Join-Path $Work "shots" }
if ($ShotDir) { New-Item -ItemType Directory -Force $ShotDir | Out-Null }
Add-Type @"
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class UW {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(P f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static List<KeyValuePair<IntPtr,string>> Of(uint pid) {
    var r = new List<KeyValuePair<IntPtr,string>>();
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) {
        var sb = new StringBuilder(512); GetWindowText(h, sb, 512);
        r.Add(new KeyValuePair<IntPtr,string>(h, sb.ToString()));
      }
      return true;
    }, IntPtr.Zero);
    return r;
  }
}
"@
$t0 = Get-Date
# To the console and log.txt, never to the pipeline (a function's output is its result).
function Log([string]$m) {
  $l = "[{0,6:N1}s] {1}" -f ((Get-Date) - $t0).TotalSeconds, $m
  Write-Host $l
  Add-Content -Encoding utf8 -Path "$Work\log.txt" -Value $l
}
$WM_CLOSE = 0x10

# --- The releases: one folder each, signed with this run's throwaway keys.
$keys = "$Work\keys"
$base = "http://127.0.0.1:$Port/files"
$asset = "DannexDownloadManager-9.9.9-portable.zip"
function Release([string]$name, [string[]]$more) {
  $out = "$Work\rel-$name"
  & python "$PSScriptRoot\mkrel.py" --keys $keys --out $out --base $base --asset $asset --kind zip `
    --program $Exe --program-name dannexdownloadmanager.exe @more | ForEach-Object { Log "  $_" }
  if ($LASTEXITCODE) { throw "mkrel.py failed" }
  $out
}
Log "making the fake releases (throwaway keys in $keys)"
$rel = @{
  "good" = Release "good" @()
  "bad-hash" = Release "bad-hash" @("--sums", "bad")
  "other-key" = Release "other-key" @("--signer", "other")
  "slow" = Release "slow" @("--pad", "30", "--asset-base", "http://127.0.0.1:$SlowPort")
}
$key = "$keys\test-key.asc"

# --- One run: a copy in its own folder (portable: data\ beside it), its update window, Update now.
function Run([string]$name, [string]$relDir, [int]$hold = 0, [string]$close = "", [string]$lang = "en",
             [bool]$dark = $false, [bool]$shots = $false) {
  $w = "$Work\$name"; $copy = "$w\copy"
  New-Item -ItemType Directory -Force "$copy\data", "$w\Downloads" | Out-Null
  Copy-Item $Exe "$copy\dannexdownloadmanager.exe"
  # The settings every run starts from (docs-shots.ps1's), written by Python: exact JSON, no BOM.
  @'
import json, sys
path, lang, dark, dl = sys.argv[1:5]
dark = dark == "1"
cats = ["General", "Compressed", "Documents", "Music", "Programs", "Video", "Torrents"]
s = dict(language=lang, language_asked=True, helpers_asked=True, extension_never_ask=True,
         update_check=False, torrent_incoming=False, torrent_upnp=False, torrent_dht=False,
         torrent_lsd=False, dark_mode_support=dark, always_dark=dark,
         category_dirs=[[c, dl] for c in cats])
json.dump({"next_id": 1, "items": [], "version": 2, "settings": s}, open(path, "w"))
'@ | python - "$copy\data\downloads.json" $lang $(if ($dark) { "1" } else { "0" }) "$w\Downloads"
  $srv = Start-Process -PassThru -WindowStyle Hidden $Server -ArgumentList "--bind", "127.0.0.1:$Port", "--files", $relDir
  $slow = $null
  if ($close -eq "download") {
    $slow = Start-Process -PassThru -WindowStyle Hidden python -ArgumentList "`"$PSScriptRoot\slowserve.py`"", "`"$relDir`"", "$SlowPort", "1000000"
  }
  $env:MDM_OFFSCREEN = "1"
  $env:MDM_UPDATE_FEED = "$base/latest.json"
  $env:MDM_TEST_UPDATE_KEY = $key
  if ($hold) { $env:MDM_TEST_UPDATE_HOLD = "$hold" } else { Remove-Item Env:MDM_TEST_UPDATE_HOLD -ErrorAction SilentlyContinue }
  $r = @{ started = $false; oldEnded = $false; marker = $false }
  try {
    $app = Start-Process -PassThru "$copy\dannexdownloadmanager.exe"
    $main = $null
    for ($i = 0; $i -lt 200 -and -not $main; $i++) { Start-Sleep -Milliseconds 100; $main = [UW]::Of([uint32]$app.Id) | Select-Object -First 1 }
    if (-not $main) { throw "$name`: DDM showed no window" }
    Start-Sleep 2
    $suffix = "$lang-" + $(if ($dark) { "dark" } else { "light" })
    if ($shots) {
      $before = [UW]::Of([uint32]$app.Id) | ForEach-Object { $_.Key }
      & "$copy\dannexdownloadmanager.exe" /d "http://127.0.0.1:$Port/gen/2M/holiday-photos.zip" /p "$w\Downloads" /n | Out-Null
      Start-Sleep 4
      $dc = [UW]::Of([uint32]$app.Id) | Where-Object { $before -notcontains $_.Key } | Select-Object -Last 1
      & "$root\tools\lab\cap-own.ps1" -ProcessId $app.Id -Title $dc.Value -Out "$ShotDir\download-complete-$suffix.png" | Out-Null
      Log "  shot download-complete-$suffix.png ($($dc.Value))"
      [UW]::PostMessage($dc.Key, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
      Start-Sleep 1
    }
    $before = [UW]::Of([uint32]$app.Id) | ForEach-Object { $_.Key }
    & "$copy\dannexdownloadmanager.exe" /show updates | Out-Null
    $upd = $null
    for ($i = 0; $i -lt 150 -and -not $upd; $i++) { Start-Sleep -Milliseconds 100; $upd = [UW]::Of([uint32]$app.Id) | Where-Object { $before -notcontains $_.Key } | Select-Object -First 1 }
    if (-not $upd) { throw "$name`: no update window" }
    Start-Sleep 2
    & "$copy\dannexdownloadmanager.exe" /show update-now | Out-Null
    Log "  Update now pressed ($($upd.Value))"
    if ($close -eq "download") {
      Start-Sleep -Milliseconds 1500
      [UW]::PostMessage($upd.Key, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
      Log "  the update window closed while it downloads"
    }
    if ($shots) {
      # Downloaded and checked well within this; the hold keeps "Installing the update...".
      Start-Sleep 12
      & "$root\tools\lab\cap-own.ps1" -ProcessId $app.Id -Title $upd.Value -Out "$ShotDir\update-installing-$suffix.png" | Out-Null
      Log "  shot update-installing-$suffix.png"
      return $r
    }
    if ($close -eq "install") {
      Start-Sleep 3
      [UW]::PostMessage($upd.Key, $WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
      Log "  the update window closed during 'Installing the update...'"
    }
    $deadline = (Get-Date).AddSeconds([Math]::Max(30, $hold + 25))
    while ((Get-Date) -lt $deadline) {
      if (-not $r.oldEnded -and $app.HasExited) { $r.oldEnded = $true; Log "  the old copy ended" }
      if (-not $r.started) {
        $new = Get-CimInstance Win32_Process -Filter "Name='dannexdownloadmanager.exe'" | Where-Object { $_.CommandLine -like "*$copy*" -and $_.CommandLine -like "*/updated*" }
        if ($new) { $r.started = $true; Log "  a new copy started: $($new.CommandLine)" }
      }
      if ($r.oldEnded -and $r.started) { break }
      Start-Sleep -Milliseconds 250
    }
    if (-not $r.started) {
      $still = [UW]::Of([uint32]$app.Id) | Where-Object { $_.Key -eq $upd.Key }
      if ($still) { & "$root\tools\lab\cap-own.ps1" -ProcessId $app.Id -Title $upd.Value -Out "$w\update-window.png" | Out-Null }
    }
    $r.marker = Test-Path "$copy\NEW-COPY.txt"
    Log "  folder: $((Get-ChildItem $copy -Name) -join ', ')"
    return $r
  } finally {
    Get-CimInstance Win32_Process -Filter "Name='dannexdownloadmanager.exe'" | Where-Object { $_.CommandLine -like "*$copy*" } |
      ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
    Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue
    if ($slow) { Stop-Process -Id $slow.Id -Force -ErrorAction SilentlyContinue }
    Remove-Item Env:MDM_OFFSCREEN, Env:MDM_UPDATE_FEED, Env:MDM_TEST_UPDATE_KEY, Env:MDM_TEST_UPDATE_HOLD -ErrorAction SilentlyContinue
    Start-Sleep 1
  }
}

# --- The scenarios: (release, hold, close, expected started / old ended / replaced).
$scenarios = [ordered]@{
  "good"           = @{ rel = "good";      hold = 4; close = "";         want = @{ started = $true;  oldEnded = $true;  marker = $true } }
  "bad-hash"       = @{ rel = "bad-hash";  hold = 0; close = "";         want = @{ started = $false; oldEnded = $false; marker = $false } }
  "other-key"      = @{ rel = "other-key"; hold = 0; close = "";         want = @{ started = $false; oldEnded = $false; marker = $false } }
  "close-download" = @{ rel = "slow";      hold = 0; close = "download"; want = @{ started = $false; oldEnded = $false; marker = $false } }
  "close-install"  = @{ rel = "good";      hold = 8; close = "install";  want = @{ started = $false; oldEnded = $false; marker = $true } }
}
$failed = @()
foreach ($name in $scenarios.Keys) {
  if ($Only.Count -and $Only -notcontains $name) { continue }
  $s = $scenarios[$name]
  Log "=== $name"
  $got = Run $name $rel[$s.rel] $s.hold $s.close
  $bad = @($s.want.Keys | Where-Object { $got[$_] -ne $s.want[$_] })
  $line = "started=$($got.started) old-ended=$($got.oldEnded) replaced=$($got.marker)"
  if ($bad.Count) { $failed += $name; Log "FAIL $name`: $line (expected $(($bad | ForEach-Object { "$_=$($s.want[$_])" }) -join ' '))" }
  else { Log "PASS $name`: $line" }
}
if ($Shots) {
  foreach ($lang in "en", "de", "ar") {
    foreach ($dark in $false, $true) {
      Log "=== shots $lang $(if ($dark) { 'dark' } else { 'light' })"
      Run "shots-$lang-$dark" $rel["good"] 300 "" $lang $dark $true | Out-Null
    }
  }
  Log "shots in $ShotDir"
}
if ($failed.Count) { Log "failed: $($failed -join ', ')"; exit 1 }
Log "all passed"
