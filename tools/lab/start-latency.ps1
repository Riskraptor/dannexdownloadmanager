# The time from a browser's download to Download File Info and to the file's first bytes (#3),
# step by step, in a Dannex Download Manager instance of its own (MDM_DATA_DIR, MDM_OFFSCREEN: its
# windows open far off the screens) and with its own labserver. Each run hands a download to the
# native host as a browser does (its own message on the host's standard input, framed as native
# messaging frames it), waits for Download File Info to show and for the download started behind
# it (Options' "Start downloading immediately", on by default) to get its first bytes, then
# closes the window (Cancel: the download goes). The steps' times come from the lines DDM and the
# host write to the run's log ("start time: ..."); the whole from the clock here.
#
#   cargo build -p mdm-app -p mdm-nmhost -p mdm-labserver
#   tools\lab\start-latency.ps1 -Bin target\debug -Runs 7
#   tools\lab\start-latency.ps1 -Bin target\debug -Kind address    # Download with DDM on a link:
#                                                                  # no name or size, looked up
#   tools\lab\start-latency.ps1 -Bin target\debug -Faults "latency_ms=300"   # a slow server
param(
  [string]$Bin = "target\debug",
  [int]$Runs = 5,
  [int]$Port = 8131,
  # "browser": the browser's own download (its name and size come with it, nothing looked up);
  # "address": Download with DDM on a link (the address is looked up for its name and size).
  # "video": the video button's menu (looked up by yt-dlp, then its first quality picked).
  [ValidateSet("browser", "address", "video")][string]$Kind = "browser",
  # The yt-dlp.exe for -Kind video (its folder is copied: a one-folder build); default your own
  # DDM's (Tasks → Download video from a page installs it), only read.
  [string]$YtDlp = "",
  # The labserver's faults for every request (query syntax), e.g. "latency_ms=300".
  [string]$Faults = "",
  # MDM_RENDERER for the run (opengl, d3d12); the system's default when empty.
  [string]$Renderer = "",
  # The run's folder (data, downloads, log); default target\start-latency in this checkout.
  [string]$Work = "",
  # Keep the run's folder afterwards (its log).
  [switch]$Keep,
  # As DDM was before #3 (MDM_NO_NUDGE=1): a hidden main window's passes held 100 ms apart.
  [switch]$NoNudge
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$bin = (Resolve-Path $(if ([IO.Path]::IsPathRooted($Bin)) { $Bin } else { Join-Path $root $Bin })).Path
$exe = Join-Path $bin "dannexdownloadmanager.exe"
$hostExe = Join-Path $bin "dannexdownloadmanager-nmhost.exe"
$work = if ($Work) { $Work } else { Join-Path $root "target\start-latency" }
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Threading;
public static class SL {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static IntPtr Find(uint pid, string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
        if (sb.ToString() == title) { found = h; return false; } }
      return true; }, IntPtr.Zero);
    return found;
  }
  // The moment (UTC ticks) window `title` of `pid` is visible, polled every millisecond; 0 when it
  // isn't within `ms`.
  public static long WaitShown(uint pid, string title, int ms) {
    var until = DateTime.UtcNow.AddMilliseconds(ms);
    while (DateTime.UtcNow < until) {
      if (Find(pid, title) != IntPtr.Zero) return DateTime.UtcNow.Ticks;
      Thread.Sleep(1);
    }
    return 0;
  }
  // Every visible window of `pid` closed (WM_CLOSE) but the main one (closing it quits a copy
  // with no tray icon): the dialogs a run left open.
  public static void CloseAll(uint pid) {
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h)) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
        if (!sb.ToString().StartsWith("Dannex Download Manager ")) PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero); }
      return true; }, IntPtr.Zero);
  }
  public static bool WaitGone(uint pid, string title, int ms) {
    var until = DateTime.UtcNow.AddMilliseconds(ms);
    while (DateTime.UtcNow < until) {
      if (Find(pid, title) == IntPtr.Zero) return true;
      Thread.Sleep(5);
    }
    return false;
  }
}
"@

# One message to the host as the browser sends it: a fresh host each time (browsers start one
# per sendNativeMessage), the frame on its input, the answer's frame back. The moment it was sent
# (UTC ticks) and the answer.
function Send-ToHost([hashtable]$message) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo $hostExe
  $psi.Arguments = "chrome-extension://nphkobgebehllokfnmfdchmcbolllkif/"
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $sent = [DateTime]::UtcNow.Ticks
  $p = [System.Diagnostics.Process]::Start($psi)
  $json = [Text.Encoding]::UTF8.GetBytes(($message | ConvertTo-Json -Compress -Depth 4))
  $in = $p.StandardInput.BaseStream
  $in.Write([BitConverter]::GetBytes([uint32]$json.Length), 0, 4)
  $in.Write($json, 0, $json.Length)
  $in.Flush()
  $out = $p.StandardOutput.BaseStream
  $len = New-Object byte[] 4
  $got = 0
  while ($got -lt 4) { $n = $out.Read($len, $got, 4 - $got); if ($n -le 0) { break }; $got += $n }
  $answered = [DateTime]::UtcNow.Ticks
  $reply = ""
  if ($got -eq 4) {
    $size = [BitConverter]::ToUInt32($len, 0)
    $buf = New-Object byte[] $size
    $got = 0
    while ($got -lt $size) { $n = $out.Read($buf, $got, $size - $got); if ($n -le 0) { break }; $got += $n }
    $reply = [Text.Encoding]::UTF8.GetString($buf, 0, $got)
  }
  $in.Close()
  $p.WaitForExit(5000) | Out-Null
  @{ sent = $sent; answered = $answered; reply = $reply }
}

# The log's "start time:" lines written since `$from` (UTC ticks), as (ticks, text).
function Get-Steps([string]$log, [long]$from) {
  if (-not (Test-Path $log)) { return @() }
  # Shared: DDM and the host keep writing it.
  $fs = [IO.File]::Open($log, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
  try { $lines = (New-Object IO.StreamReader($fs)).ReadToEnd() -split "`r?`n" } finally { $fs.Dispose() }
  foreach ($l in $lines) {
    if ($l -notmatch '^(\S+Z)\s+\w+\s.*start time: (.*)$') { continue }
    $t = [DateTime]::Parse($Matches[1], $null, [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal).Ticks
    if ($t -ge $from) { , @($t, $Matches[2]) }
  }
}

function Median([double[]]$xs) {
  $xs = @($xs | Where-Object { $_ -ge 0 } | Sort-Object)
  if (-not $xs.Count) { return $null }
  $m = [int][Math]::Floor($xs.Count / 2)
  if ($xs.Count % 2) { $xs[$m] } else { ($xs[$m - 1] + $xs[$m]) / 2 }
}

$srv = $null; $app = $null
try {
  if (Test-Path $work) { Remove-Item $work -Recurse -Force }
  $data = Join-Path $work "data"
  $downloads = Join-Path $work "Downloads"
  New-Item -ItemType Directory -Force $data, $downloads | Out-Null
  $dirs = @("General", "Compressed", "Documents", "Music", "Programs", "Video", "Torrents") |
    ForEach-Object { , @($_, $(if ($_ -eq "General") { $downloads } else { "$downloads\$_" })) }
  $version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
  $list = [ordered]@{
    next_id = 1; version = 2; items = @()
    settings = [ordered]@{
      language = "en"; language_asked = $true; helpers_asked = $true; extension_never_ask = $true
      update_check = $false; notes_seen = $version; category_dirs = $dirs
    }
  }
  [IO.File]::WriteAllText("$data\downloads.json", ($list | ConvertTo-Json -Depth 8))
  if ($Kind -eq "video") {
    if (-not $YtDlp) {
      $cur = Join-Path ([Environment]::GetFolderPath("ApplicationData")) "dannexdownloadmanager\tools\yt-dlp\current.json"
      if (-not (Test-Path $cur)) { throw "no yt-dlp: pass -YtDlp" }
      $YtDlp = (Get-Content $cur -Raw | ConvertFrom-Json).exe
    }
    $to = "$data\tools\yt-dlp\given"
    New-Item -ItemType Directory -Force $to | Out-Null
    Copy-Item "$(Split-Path -Parent (Resolve-Path $YtDlp).Path)\*" $to -Recurse
    [IO.File]::WriteAllText("$data\tools\yt-dlp\current.json", (@{ version = "given"; exe = "$to\$(Split-Path -Leaf $YtDlp)" } | ConvertTo-Json))
  }
  $srvArgs = @("--bind", "127.0.0.1:$Port")
  if ($Faults) { $srvArgs += @("--faults", $Faults) }
  $srv = Start-Process -PassThru -WindowStyle Hidden (Join-Path $bin "labserver.exe") -ArgumentList $srvArgs
  $env:MDM_DATA_DIR = $data
  $env:MDM_OFFSCREEN = "1"
  if ($Renderer) { $env:MDM_RENDERER = $Renderer }
  if ($NoNudge) { $env:MDM_NO_NUDGE = "1" }
  # Started as the browser's host starts it: in the notification area, only the download's
  # own window comes up.
  $ipc = Join-Path $data "ipc.json"
  function Start-App {
    if (Test-Path $ipc) { Remove-Item $ipc -Force }
    $a = Start-Process -PassThru $exe -ArgumentList "/startup"
    for ($i = 0; $i -lt 200 -and -not (Test-Path $ipc); $i++) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path $ipc)) { throw "DDM didn't open its channel" }
    # Settled: its first frames drawn, its start's own work done.
    Start-Sleep 4
    $a
  }
  $app = Start-App
  $log = Join-Path $data "logs\ddm.log"
  $rows = @()
  # Run 0 warms up (the programs read from the disk, an antivirus' first look at them) and isn't
  # counted.
  for ($run = 0; $run -le $Runs; $run++) {
    if ($Kind -eq "video") {
      # The video button's click: the app looks the page up (yt-dlp), the menu asks how it goes
      # (through a host each time, as video.js does: again at once after an answer the host
      # held), then its first quality is picked; downloaded with no window to answer.
      # A page playing the clip (yt-dlp's generic reader finds its <video>).
      $clip = [Uri]::EscapeDataString("/gen/16M/clip-$run.mp4?rate=8M&seed=$run")
      $page = "http://127.0.0.1:$Port/page?video=$clip"
      $base = @{ cookies = ""; private = $false; v = "2.0.0"; brands = @("Chromium", "Google Chrome")
        userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36" }
      $look = $base.Clone(); $look.videoLookup = @{ page = $page }
      $h = Send-ToHost $look
      $token = ([regex]::Match($h.reply, '"token":"([^"]+)"')).Groups[1].Value
      if (-not $token) { throw "run ${run}: no look-up ($($h.reply))" }
      $ready = $null; $polls = 0
      while (-not $ready -and ([DateTime]::UtcNow.Ticks - $h.sent) / 1e4 -lt 120000) {
        Start-Sleep -Milliseconds 10
        $q = $base.Clone(); $q.videoPoll = @{ token = $token }
        $p = Send-ToHost $q
        $polls++
        if ($p.reply -match '"state":"ready"') { $ready = $p.answered }
        elseif ($p.reply -notmatch '"state":"pending"') { throw "run ${run}: $($p.reply)" }
      }
      if (-not $ready) { throw "run ${run}: the look-up never ended" }
      # The choice opens Download video with that quality chosen.
      $k = $base.Clone(); $k.videoPick = @{ token = $token; choice = 0 }
      $pk = Send-ToHost $k
      $pickShown = [SL]::WaitShown([uint32]$app.Id, "Download File Info", 15000)
      $steps = @(Get-Steps $log $h.sent)
      $lookedUp = @($steps | Where-Object { $_[1] -match 'video: looked up in (\d+) ms' })[0]
      $row = [ordered]@{
        run = $run
        lookup_answer_ms = [Math]::Round(($h.answered - $h.sent) / 1e4)
        ytdlp_ms = if ($lookedUp -and $lookedUp[1] -match '(\d+) ms') { [double]$Matches[1] } else { -1 }
        ready_total_ms = [Math]::Round(($ready - $h.sent) / 1e4)
        ready_after_ytdlp_ms = if ($lookedUp) { [Math]::Round(($ready - $lookedUp[0]) / 1e4) } else { -1 }
        polls = $polls
        pick_answer_ms = [Math]::Round(($pk.answered - $pk.sent) / 1e4)
        pick_window_ms = if ($pickShown) { [Math]::Round(($pickShown - $pk.sent) / 1e4) } else { -1 }
      }
      if ($run -gt 0) { $rows += [pscustomobject]$row }
      # A DDM of its own for each video (its look-ups' places, its window's state).
      Stop-Process -Id $app.Id -ErrorAction SilentlyContinue
      Start-Sleep 1
      $app = Start-App
      continue
    }
    $name = "start-$run.bin"
    $url = "http://127.0.0.1:$Port/gen/200M/$name" + "?rate=8M&seed=$run"
    $message = @{ url = $url; finalUrl = ""; referrer = "http://127.0.0.1:$Port/page"; cookies = ""
      userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36"
      brands = @("Chromium", "Google Chrome"); v = "2.0.0" }
    if ($Kind -eq "browser") {
      $message.filename = $name; $message.size = 209715200; $message.mime = "application/octet-stream"
    } else {
      $message.explicit = $true
    }
    $h = Send-ToHost $message
    if ($h.reply -notmatch '"ok":true') { throw "run ${run}: the host answered $($h.reply)" }
    $shown = [SL]::WaitShown([uint32]$app.Id, "Download File Info", 15000)
    if (-not $shown) { throw "run ${run}: no Download File Info" }
    # The first bytes, from the log (the download started behind the dialog).
    $first = $null
    for ($i = 0; $i -lt 300 -and -not $first; $i++) {
      $first = @(Get-Steps $log $h.sent | Where-Object { $_[1] -match 'first bytes' })[0]
      if (-not $first) { Start-Sleep -Milliseconds 50 }
    }
    $steps = @(Get-Steps $log $h.sent)
    $num = { param($pattern) $s = @($steps | Where-Object { $_[1] -match $pattern })[0]; if ($s -and $s[1] -match '(\d+) ms') { [double]$Matches[1] } else { -1 } }
    $row = [ordered]@{
      run = $run
      host_ms = & $num 'browser integration: handed'
      host_answer_ms = [Math]::Round(($h.answered - $h.sent) / 1e4)
      taken_ms = & $num 'taken by the window'
      dialog_ms = & $num 'Download File Info.* shown'
      shown_total_ms = [Math]::Round(($shown - $h.sent) / 1e4)
      answered_ms = & $num 'the address answered'
      early_ms = & $num 'started behind Download File Info'
      first_bytes_ms = & $num 'first bytes'
      first_total_ms = if ($first) { [Math]::Round(($first[0] - $h.sent) / 1e4) } else { -1 }
    }
    if ($run -gt 0) { $rows += [pscustomobject]$row }
    # Cancel: the dialog's window closed (its × does the same), the early download goes with it.
    $hwnd = [SL]::Find([uint32]$app.Id, "Download File Info")
    [SL]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    [SL]::WaitGone([uint32]$app.Id, "Download File Info", 5000) | Out-Null
    Start-Sleep -Milliseconds 1500
  }
  $rows | Format-Table -AutoSize | Out-String -Width 250
  $cols = if ($Kind -eq "video") {
    "lookup_answer_ms", "ytdlp_ms", "ready_total_ms", "ready_after_ytdlp_ms", "polls", "pick_answer_ms", "pick_window_ms"
  } else {
    "host_ms", "host_answer_ms", "taken_ms", "dialog_ms", "shown_total_ms", "answered_ms", "early_ms", "first_bytes_ms", "first_total_ms"
  }
  $med = [ordered]@{ run = "median" }
  foreach ($c in $cols) { $med[$c] = Median ($rows | ForEach-Object { $_.$c }) }
  [pscustomobject]$med | Format-Table -AutoSize | Out-String -Width 250
} finally {
  if ($app) { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
  if ($srv) { Stop-Process -Id $srv.Id -ErrorAction SilentlyContinue }
  Remove-Item Env:MDM_DATA_DIR, Env:MDM_OFFSCREEN, Env:MDM_RENDERER, Env:MDM_NO_NUDGE -ErrorAction SilentlyContinue
  Start-Sleep 1
  if (-not $Keep) { Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue }
}
