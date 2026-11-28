#!/usr/bin/env bash
# mdm-lab: Linux builds and window shots on WSL2. A WSL2 distribution (Ubuntu 24.04) of its own
# that builds DDM for Linux at the PC's full speed and screenshots its windows on a virtual
# screen (Xvfb) that never shows on Windows. The VirtualBox machines in tools/lab/linux-vm/ run on
# top of Hyper-V here (VirtualBox's NEM mode), about three times slower: builds there took 15 to
# 60 minutes and a shot about 40 s. Here a build of main takes under 5 minutes from scratch and
# about 35 s after a one-line change, and a window shot about 1.2 s. This lab takes over those two
# jobs, and deploy.ps1 hands its builds to the VMs for the checks only a real desktop can do
# ("What mdm-lab can't replace", at the end of this header).
#
# == Setting it up
# From PowerShell on the Windows host, once:
#   wsl --install -d Ubuntu-24.04 --name mdm-lab --location C:\mdm-wsl --no-launch
#   wsl -d mdm-lab -u root -- bash /mnt/d/<checkout>/tools/lab/wsl/setup.sh
#   wsl --terminate mdm-lab        # wsl.conf (no Windows programs, user lab) applies at the next start
#
# This script (as root) writes the distribution's /etc/wsl.conf (systemd, the `lab` user, and no
# interop: nothing in mdm-lab can start a Windows program), installs the build's libraries (those
# of ci.yml and the VMs), Xvfb, Metacity, ImageMagick, xwd, xdotool, wmctrl, Mesa's software
# OpenGL and Vulkan (llvmpipe, lavapipe), ffmpeg, 7-Zip and an Ubuntu desktop's fonts with every
# script DDM's languages need, then, as `lab`: Rust, the VMs' `vmtest` cargo profile, the checkout
# in ~/src/mdm (the distribution's own ext4 disk: building from /mnt/c or /mnt/d goes through 9P
# and is many times slower) and the lab scripts in ~/lab/tools/. Running it again only adds what's
# missing and updates the checkout and the scripts: after a change to these scripts, the same
# setup.sh line copies them in again (a minute: the packages are there already).
#
# From Git Bash rather than PowerShell, prefix MSYS_NO_PATHCONV=1 (or Git Bash rewrites /mnt/...
# and ~/... paths) and use `wsl -d mdm-lab --exec bash -c '...'` when the command has $VARIABLES:
# without --exec, wsl.exe hands the line to a shell that expands them first.
#
# It touches nothing outside mdm-lab: never %USERPROFILE%\.wslconfig, never another distribution.
# Never run `wsl --shutdown` for it (that stops every distribution): `wsl --terminate mdm-lab`
# stops this one.
#
# == Building (build.sh)
#   wsl -d mdm-lab -- bash ~/lab/tools/build.sh main                 # ~/lab/bin/main/
#   wsl -d mdm-lab -- bash ~/lab/tools/build.sh pr/167 pr-167        # a pull request, labelled
#   wsl -d mdm-lab -- bash ~/lab/tools/build.sh --release main       # fat LTO, what ships: main-release
# A branch, pr/<number> or a commit is fetched and checked out (detached), built with the vmtest
# profile (release without LTO, incremental) on every core, and its four programs
# (dannexdownloadmanager, -nmhost, -cli, labserver) copied to ~/lab/bin/<label>/ with a COMMIT
# file. The target folder stays, so the next build recompiles only what changed. The log is
# ~/lab/build.log. Builds take turns (a lock): every caller shares the one checkout. From Windows
# the builds are at \\wsl$\mdm-lab\home\lab\lab\bin\.
#
# == Window shots (shots.sh)
#   wsl -d mdm-lab -- bash ~/lab/tools/shots.sh main --langs en,de,ar,ja,hi
#   wsl -d mdm-lab -- bash ~/lab/tools/shots.sh pr-199 --modes light,dark --theme adwaita `
#       --windows main,options-general=options:general,about
# One DDM per language and mode: each window opens through DDM's own `/show <window>` command
# line, is shot once it has painted (capture.py: two reads of it in a row the same, at most 2 s
# for a window that never settles, such as progress), with its title bar and borders, is closed
# from its title bar, then the next. No restart per shot. The default list is 21 windows: the main
# window and the 20 dialogs of tools/lab/docs-shots.ps1 (<file name>=<show argument> when they
# differ). Lists take commas: wsl.exe hands its command line to a shell again, which splits quoted
# spaces. A menu:... show is shot with the main window, grown by MDM_GROW pixels (default 300).
#
# Each run has its own data folder (MDM_DATA_DIR), download folder and lab server (port 8197),
# under ~/lab/run/<label>; the list is downloaded once, for the first language, and given to the
# others. --finished-only leaves out the download still running (shots that must not change
# between runs); --keep-list reuses the label's last list (the same names and dates: two runs
# compared pixel by pixel). Shots land in ~/lab/shots/<label>/<lang>-<mode>/ with timing.txt.
# MDM_SET adds settings (a JSON object), as in tools/lab/linux-vm/win.sh; other MDM_* variables
# (MDM_RENDERER, MDM_OFFSCREEN, ...) reach DDM.
#
# renderers.sh <label> compares OpenGL and Vulkan (lavapipe) on nine windows, pixel by pixel
# (pixdiff.py from tools/lab/linux-vm/), for builds with View -> Draw with -> Vulkan: forced, as
# the saved choice (OpenGL, saying why, with no hardware driver) and with MDM_TEST_SOFTWARE_CARD=1.
#
# screen.sh start|stop|status runs the virtual screen by hand (shots.sh starts it itself): Xvfb
# :99 at 1600x1000, a session bus, and Metacity (GNOME's window manager, without a compositor) for
# title bars and borders. For anything else on that screen:
#   . ~/lab/tools/env.sh && bash ~/lab/tools/screen.sh start
#   ~/lab/bin/main/dannexdownloadmanager &     # on :99, never on Windows
#   import -window root /tmp/screen.png        # or capture.py <window id> <out.png>
#   bash ~/lab/tools/screen.sh stop            # DDM, the lab server and the screen ended
#
# Why nothing shows on Windows: WSLg shows Linux windows on the Windows desktop and plays their
# sound there, and it can only be turned off for every distribution at once (guiApplications in
# .wslconfig), which the lab leaves alone. So env.sh, which every lab script sources, removes
# WAYLAND_DISPLAY and PULSE_SERVER, points DISPLAY at :99 and XDG_RUNTIME_DIR at the lab's own
# folder (WSLg's Wayland and PulseAudio sockets are in its own), and draws with software OpenGL
# (llvmpipe). With interop off, xdg-open can't reach a Windows browser either.
#
# == Into a VM (deploy.ps1)
#   tools\lab\wsl\deploy.ps1 main mdm-ubuntu            # mdm-lab's ~/lab/bin/main -> the VM's
#   tools\lab\wsl\deploy.ps1 pr-167 mdm-kde -DryRun     # what it would copy (no VBoxManage call)
# The programs are read from \\wsl$\mdm-lab\... and copied with `VBoxManage guestcontrol copyto`
# into a folder beside ~/lab/bin/<label>/, then swapped in whole. The VM must be running. Its
# account comes from MDM_VM_USER / MDM_VM_PASSWORD or %USERPROFILE%\.mdm-dev\vm-account.txt
# (outside the repository). The VM's own harnesses (win.sh, shots.sh, streams.mjs, ...) then run
# that build as one of theirs.
#
# == What mdm-lab can't replace
# Builds and window shots, yes; a desktop, no. These still need the VMs in tools/lab/linux-vm/:
# - The desktop's portals and settings: the GNOME and KDE file choosers (xdg-desktop-portal), the
#   system's dark mode, accent and contrast as DDM reads them, notifications.
# - The tray: there is no StatusNotifier host or AppIndicator on Xvfb, so the tray icon, its menu
#   and Hide to the notification area can't be checked here.
# - Real browsers and native messaging: the Snap Firefox, Chromium and Google Chrome with the
#   extension handing downloads and streams to DDM (streams.mjs, store-check.mjs, ...).
# - Desktop-specific behaviour: GNOME Shell's, KWin's, Cinnamon's or XFCE's own window placement,
#   focus, frames and Wayland sessions; AT-SPI and screen readers; real hardware OpenGL.
#   Metacity's frames look like GNOME's but are not GNOME Shell's.
# - Sound, and anything else that leaves the machine through the desktop.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "run as root: wsl -d mdm-lab -u root -- bash $0" >&2; exit 2; }
user=lab
repo_url=https://github.com/Riskraptor/dannexdownloadmanager.git
export DEBIAN_FRONTEND=noninteractive

# --- The distribution's own settings (per distribution; .wslconfig is global and stays as it is).
# Interop off: nothing in here can start a Windows program (a browser, explorer.exe), and
# Windows' PATH stays out of builds. WSLg can't be turned off per distribution (guiApplications
# is a .wslconfig key), so env.sh keeps every program off its display, sound and runtime folder.
cat > /etc/wsl.conf <<'EOF'
# mdm-lab: DDM's build and screenshot machine (tools/lab/wsl/setup.sh).
[boot]
systemd=true

[interop]
enabled=false
appendWindowsPath=false

[user]
default=lab
EOF
id "$user" >/dev/null 2>&1 || useradd -m -s /bin/bash -G sudo "$user"
echo "$user ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/$user
chmod 440 /etc/sudoers.d/$user

# --- Packages: the build's libraries (the same as ci.yml, release.yml and the VMs' setup.sh),
# the helpers DDM drives (ffmpeg, 7-Zip: their windows show what is found), the virtual screen
# with a window manager that frames windows as a desktop does (Metacity: GNOME's own frames,
# without a compositor, so a capture reads the screen as drawn), capture and input tools, and
# the fonts of an Ubuntu desktop with every script the languages need.
apt-get update -q
apt-get install -y -q --no-install-recommends \
  build-essential pkg-config git curl ca-certificates python3 python3-pil \
  libgtk-3-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev cmake libclang-dev \
  libgl1-mesa-dri libglx-mesa0 mesa-utils mesa-vulkan-drivers vulkan-tools \
  xvfb xauth x11-utils x11-apps x11-xserver-utils xdotool wmctrl imagemagick metacity dbus-x11 \
  adwaita-icon-theme gnome-themes-extra yaru-theme-gtk yaru-theme-icon \
  ffmpeg p7zip-full \
  fontconfig fonts-ubuntu fonts-dejavu-core fonts-freefont-ttf fonts-liberation \
  fonts-noto-core fonts-noto-ui-core fonts-noto-mono fonts-noto-cjk fonts-noto-color-emoji \
  fonts-kacst fonts-indic fonts-khmeros-core fonts-lao fonts-thai-tlwg fonts-sil-abyssinica
fc-cache -f >/dev/null

# --- As lab: Rust, the lab's cargo profile, the checkout on the distribution's own disk (ext4:
# building from /mnt/c or /mnt/d goes through 9P, many times slower).
here=$(cd "$(dirname "$0")" && pwd)
sudo -u "$user" -H bash -s "$repo_url" "$here" <<'USER'
set -euo pipefail
repo_url=$1; here=$2
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
fi
. "$HOME/.cargo/env"
rustup update stable >/dev/null
# The VMs' profile (tools/lab/linux-vm/setup.sh): release without the shipping build's fat LTO,
# incremental, so a branch switch or a one-line change recompiles only what changed.
mkdir -p "$HOME/.cargo"
if ! grep -q 'profile.vmtest' "$HOME/.cargo/config.toml" 2>/dev/null; then
  cat >> "$HOME/.cargo/config.toml" <<'EOF'

[profile.vmtest]
inherits = "release"
lto = false
codegen-units = 256
incremental = true
debug = false
opt-level = 2

[profile.vmtest.package."*"]
opt-level = 3
EOF
fi
mkdir -p "$HOME/src" "$HOME/lab/bin"
[ -d "$HOME/src/mdm/.git" ] || git clone -q "$repo_url" "$HOME/src/mdm"
git -C "$HOME/src/mdm" fetch -q origin
# The lab scripts, copied in so they run from ext4 (refreshed by every setup run).
mkdir -p "$HOME/lab/tools"
cp "$here"/*.sh "$here"/*.py "$here/../linux-vm/pixdiff.py" "$HOME/lab/tools/"
# The toolchain rust-toolchain.toml names, installed now rather than at the first build.
# (Under build.sh's lock: never in the middle of someone's build.)
mkdir -p "$HOME/lab"
(cd "$HOME/src/mdm" && flock "$HOME/lab/build.lock" git checkout -q -f --detach origin/main \
  && rustup show active-toolchain >/dev/null)
echo "lab ready: $(. /etc/os-release; echo "$PRETTY_NAME"), $(rustc --version), $(nproc) cores"
USER
