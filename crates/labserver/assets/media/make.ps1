# The lab server's sample media (GET /media/NAME, crates/labserver/src/media.rs), made from
# nothing but ffmpeg's own test sources: a tone, a colour test picture. Small, so they can be
# kept in the repository and built into the server.
#   pwsh crates/labserver/assets/media/make.ps1 [-Ffmpeg path\to\ffmpeg.exe]
param([string]$Ffmpeg = "ffmpeg")
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$common = @("-hide_banner", "-loglevel", "error", "-y")
# After the inputs: no metadata of ffmpeg's own (its version), the same bytes each time.
$plain = @("-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact", "-flags:a", "+bitexact")
# 20 s of a 440 Hz tone, MP3 (16 kb/s, mono): a song or a podcast's sound.
& $Ffmpeg @common -f lavfi -i "sine=frequency=440:duration=20:sample_rate=22050" @plain -ac 1 -c:a libmp3lame -b:a 16k -id3v2_version 0 (Join-Path $here "tone.mp3")
# 16 s of a colour test picture with a tone, H.264 and AAC in an MP4 whose header comes first.
& $Ffmpeg @common -f lavfi -i "smptebars=size=320x180:rate=10:duration=16" -f lavfi -i "sine=frequency=330:duration=16:sample_rate=22050" `
  @plain -c:v libopenh264 -b:v 24k -rc_mode bitrate -g 20 -c:a aac -b:a 24k -ac 1 -movflags +faststart -shortest (Join-Path $here "clip.mp4")
# 8 s of the same in WebM (VP9, Opus).
& $Ffmpeg @common -f lavfi -i "smptebars=size=320x180:rate=10:duration=8" -f lavfi -i "sine=frequency=550:duration=8:sample_rate=48000" `
  @plain -c:v libvpx-vp9 -b:v 40k -deadline realtime -cpu-used 8 -c:a libopus -b:a 24k -ac 1 -shortest (Join-Path $here "clip.webm")
Get-ChildItem $here -File -Exclude *.ps1 | ForEach-Object { "{0,-10} {1,8} bytes" -f $_.Name, $_.Length }
