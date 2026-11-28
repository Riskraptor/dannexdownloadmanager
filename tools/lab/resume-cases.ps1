# Resume, made robust (#4), in the real app: one Dannex Download Manager instance of its own per
# case (its own data folder, MDM_DATA_DIR; off the screens, MDM_OFFSCREEN) downloads one file from
# its own lab server while the case breaks something part-way, then the file is checked against
# a clean download of the same address. Nothing is clicked: the automation API starts the
# download (`dannexdownloadmanager-cli api on`, `start 1`) and `status` follows it. The user's own
# DDM, its data and the computer's network are never touched: "the network" is the lab server
# itself (stopped: connections broken and refused; started again on the same port).
#
#   cargo build -p mdm-app -p mdm-cli -p mdm-labserver
#   tools\lab\resume-cases.ps1 -Bin C:\ddm-targets\dlux\debug
#
# Cases (-Only to pick): network (gone 40 s), restart (gone 1 s), expired (403 for a while, then the
# address works again), noranges (the server comes back without ranges), sleep (DDM suspended
# 30 s while the server drops every connection), kill (DDM killed mid-write, started again).
param(
  [Parameter(Mandatory)][string]$Bin,
  [int]$Port = 8097,
  [string]$Work = "",
  [string[]]$Only = @(),
  [string]$Size = "48M",
  [string]$Rate = "300K"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exe = Join-Path $Bin "dannexdownloadmanager.exe"
$cli = Join-Path $Bin "dannexdownloadmanager-cli.exe"
$lab = Join-Path $Bin "labserver.exe"
$work = if ($Work) { $Work } else { Join-Path $root "target\resume-cases" }
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class RC {
  [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr h);
  [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr h);
}
"@
function Want([string]$name) { $Only.Count -eq 0 -or $Only -contains $name }

$script:srv = $null
function Start-Lab([string]$faults = "") {
  $a = @("--bind", "127.0.0.1:$Port")
  if ($faults) { $a += @("--faults", $faults) }
  $script:srv = Start-Process -PassThru -WindowStyle Hidden $lab -ArgumentList $a
  for ($i = 0; $i -lt 50; $i++) {
    try { $c = New-Object Net.Sockets.TcpClient("127.0.0.1", $Port); $c.Close(); return } catch { Start-Sleep -Milliseconds 100 }
  }
  throw "the lab server didn't start"
}
function Stop-Lab {
  if ($script:srv) { Stop-Process -Id $script:srv.Id -Force -ErrorAction SilentlyContinue; $script:srv.WaitForExit(); $script:srv = $null }
}

# The status of download 1, as the automation API says it (null when the app doesn't answer).
function Status1 {
  $out = & $cli status 2>$null
  if (-not $out) { return $null }
  try { (($out | Out-String) | ConvertFrom-Json).downloads | Where-Object { $_.id -eq 1 } | Select-Object -First 1 } catch { $null }
}

# The app answers its automation API (a debug build takes a few seconds to start).
function Wait-Api {
  for ($w = 0; $w -lt 60; $w++) {
    Start-Sleep -Milliseconds 500
    & $cli api on 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { return }
  }
  throw "the app never answered"
}

function Sha([string]$p) { (Get-FileHash -Algorithm SHA256 $p).Hash }

# The clean copy every case's file is compared with: the same address, no fault.
New-Item -ItemType Directory -Force $work | Out-Null
$results = @()
try {
  Start-Lab
  $refDir = Join-Path $work "reference"
  Remove-Item $refDir -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force $refDir | Out-Null

  function Case([string]$name, [int]$seed, [scriptblock]$break, [int]$timeout = 600) {
    if (-not (Want $name)) { return }
    # A server of its own, without the faults a case before it left.
    Stop-Lab; Start-Lab
    $url = "http://127.0.0.1:$Port/gen/$Size/$name.bin?seed=$seed&rate=$Rate"
    $ref = Join-Path $refDir "$name.bin"
    & $exe --cli get "http://127.0.0.1:$Port/gen/$Size/$name.bin?seed=$seed" -o $ref | Out-Null
    $want = Sha $ref
    $dir = Join-Path $work $name
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$dir\data", "$dir\Downloads" | Out-Null
    $dest = Join-Path "$dir\Downloads" "$name.bin"
    $list = [ordered]@{
      next_id = 2; version = 2
      items = @([ordered]@{ id = 1; url = $url; file_name = "$name.bin"; dest = $dest; size = $null; downloaded = 0
        status = "Queued"; category = "General"; description = ""; last_try = $null; error = $null; resumable = $null })
      settings = [ordered]@{ language = "en"; language_asked = $true; helpers_asked = $true; extension_never_ask = $true
        update_check = $false; max_connections = 8 }
    }
    [IO.File]::WriteAllText("$dir\data\downloads.json", ($list | ConvertTo-Json -Depth 8))
    $env:MDM_DATA_DIR = "$dir\data"
    $env:MDM_OFFSCREEN = "1"
    $script:app = Start-Process -PassThru $exe
    $t0 = Get-Date
    $row = [ordered]@{ case = $name; at_break = 0; lowest_after = $null; recovered_s = $null; restarted_from_zero = $false; file_ok = $false; leftovers = ""; notes = "" }
    try {
      Wait-Api
      & $cli start 1 | Out-Null
      # Until a third of it is in.
      $size = $null
      while ($true) {
        Start-Sleep -Milliseconds 300
        $s = Status1
        if ($s -and $s.size) { $size = [long]$s.size }
        if ($s -and $size -and $s.downloaded -ge $size / 3) { break }
        if (((Get-Date) - $t0).TotalSeconds -gt 120) { throw "${name}: never got a third in" }
      }
      $row.at_break = [long]$s.downloaded
      $tb = Get-Date
      & $break
      # Followed to its end: the lowest figure after the break, when it was moving again.
      $lowest = [long]::MaxValue; $moving = $null; $last = $row.at_break
      while ($true) {
        Start-Sleep -Milliseconds 500
        $s = Status1
        if (-not $s) { continue }
        $d = [long]$s.downloaded
        if ($d -lt $lowest) { $lowest = $d }
        if (-not $moving -and $d -gt $row.at_break + 1MB) { $moving = (Get-Date) }
        if ($s.status -eq "Complete") { break }
        if (((Get-Date) - $tb).TotalSeconds -gt $timeout) { $row.notes = "timed out: $($s.status) $($s.error)"; break }
      }
      $row.lowest_after = $lowest
      $row.restarted_from_zero = $lowest -lt 1MB
      if ($moving) { $row.recovered_s = [int]($moving - $tb).TotalSeconds }
      Start-Sleep 1
      if (Test-Path $dest) { $row.file_ok = (Sha $dest) -eq $want }
      $row.leftovers = (Get-ChildItem "$dir\Downloads" -Force | Where-Object { $_.Name -ne "$name.bin" } | ForEach-Object Name) -join ", "
      # What the download's log says (its events, from the list saved at exit).
      & $cli api off | Out-Null
    } finally {
      Stop-Process -Id $script:app.Id -ErrorAction SilentlyContinue
      Start-Sleep 1
      Remove-Item Env:MDM_DATA_DIR, Env:MDM_OFFSCREEN -ErrorAction SilentlyContinue
    }
    $saved = Get-Content "$dir\data\downloads.json" -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
    if ($saved) { $row.notes += (($saved.items | Where-Object { $_.id -eq 1 }).log | ForEach-Object { $_[1] } | Select-Object -Last 8) -join " | " }
    $script:results += [pscustomobject]$row
    "{0}: at break {1:N0}, lowest after {2:N0}, moving again after {3} s, file ok {4}, leftovers [{5}]" -f $name, $row.at_break, $row.lowest_after, $row.recovered_s, $row.file_ok, $row.leftovers
  }

  # The network goes for 40 s (longer than the connections' retries): it fails as a network
  # failure, DDM starts it again by itself once it is back.
  Case "network" 51 { Stop-Lab; Start-Sleep 40; Start-Lab }
  # The server restarts (a second's outage): the connections' retries ride it out.
  Case "restart" 52 { Stop-Lab; Start-Sleep 1; Start-Lab }
  # The link expires (403 to every request for 20 s), then the address works again: Resume.
  Case "expired" 53 { Stop-Lab; Start-Lab "refuse=403"; Start-Sleep 20; Stop-Lab; Start-Lab; Start-Sleep 2; & $cli start 1 | Out-Null }
  # The server comes back without ranges: nothing can be continued; the file comes whole again.
  Case "noranges" 54 { Stop-Lab; Start-Lab "no_range=1"; Start-Sleep 25; & $cli start 1 | Out-Null }
  # The PC sleeps: DDM frozen 30 s, the server drops every connection meanwhile (it restarts).
  Case "sleep" 55 {
    # Every process of this DDM frozen at once, as a sleeping PC freezes them (its crash monitor
    # too: it would see a frozen window otherwise); the process objects kept, their handles with
    # them.
    $ids = @($script:app.Id) + @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($script:app.Id)" | ForEach-Object ProcessId)
    $procs = @($ids | ForEach-Object { Get-Process -Id $_ })
    foreach ($p in $procs) { [void][RC]::NtSuspendProcess($p.Handle) }
    Stop-Lab; Start-Sleep 30; Start-Lab
    foreach ($p in $procs) {
      $r = [RC]::NtResumeProcess($p.Handle)
      if ($r -ne 0) { throw "DDM could not be woken ($r)" }
    }
  }
  # DDM killed mid-write, started again, Resume.
  Case "kill" 56 {
    Stop-Process -Id $script:app.Id -Force
    Start-Sleep 2
    $script:app = Start-Process -PassThru $exe
    Wait-Api
    & $cli start 1 | Out-Null
  }
} finally {
  Stop-Lab
}
$results | Format-Table -AutoSize | Out-String -Width 400
$results | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $work "results.json")
