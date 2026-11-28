#!/usr/bin/env bash
# Builds the Linux release into target/dist/ (up to 0.2.4, each name ended in "-experimental"):
#   DannexDownloadManager-<version>-linux-x86_64.AppImage        runs on most distros
#   DannexDownloadManager-<version>-linux-x86_64.AppImage.zsync  its updates (AppImage tools)
#   DannexDownloadManager-<version>-linux-x86_64.tar.gz          the plain programs
#   DannexDownloadManager-<version>-linux-x86_64.deb             Debian, Ubuntu, Mint
#   DannexDownloadManager-<version>-linux-x86_64.rpm             Fedora, RHEL, openSUSE
# Update now (crates/tools/src/update.rs), the website (tools/website/build.py) and the docs
# look for these names, and still for the "-experimental" ones of older releases.
# Needs: cargo, pwsh (the extension), python3 with Pillow (the icon), dpkg-deb, rpmbuild, network access (appimagetool is fetched),
# and zsyncmake if the appimagetool fetched doesn't write the .zsync itself.
# Build on the oldest Ubuntu you want to support: the programs need its glibc or newer.
#   tools/installer/build-linux.sh [--no-build]
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
dist="$root/target/dist"
bin="$root/target/release"
version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -1)"
[ -n "$version" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
name="DannexDownloadManager-$version-linux-x86_64"
programs=(dannexdownloadmanager dannexdownloadmanager-nmhost dannexdownloadmanager-cli dannexdownloadmanager-mcp)

if [ "${1:-}" != "--no-build" ]; then
  # The build machine's paths out of the programs (#25): panic locations and debug info say
  # "cargo/..." for cargo's folder and "ddm/..." for this checkout. Encoded flags (unit separators),
  # as those paths may hold spaces.
  CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=cargo"$'\x1f'"--remap-path-prefix=$root=ddm" \
    cargo build --release -p mdm-app -p mdm-nmhost -p mdm-cli -p mdm-mcp
fi
pwsh -NoProfile -File "$root/tools/extension/pack.ps1"

mkdir -p "$dist"
stage="$dist/stage-linux"
rm -rf "$stage"
mkdir -p "$stage"

# One tree in the usual Linux layout; every package is made from it.
tree="$stage/tree"
mkdir -p "$tree/usr/bin" "$tree/usr/share/applications" \
  "$tree/usr/share/icons/hicolor/256x256/apps" "$tree/usr/share/dannexdownloadmanager" \
  "$tree/usr/share/doc/dannexdownloadmanager"
for p in "${programs[@]}"; do install -m 755 "$bin/$p" "$tree/usr/bin/$p"; done
# Crash reports: the release's symbols from the programs' debug info (the -symbols zip), then the
# programs without it or their symbol table (#25; the build id that matches them to the symbols,
# and the unwind tables, stay).
python3 "$root/tools/installer/symbols.py" linux-x86_64 "$version" "$dist" "${programs[@]/#/$bin/}"
for p in "${programs[@]}"; do strip --strip-all "$tree/usr/bin/$p"; done
# The oldest glibc the programs run on: the newest GLIBC_x.y symbol version they ask for. The
# .deb and .rpm require it, so a system too old refuses the package instead of installing
# programs that can't start ("GLIBC_2.38 not found" on Debian 12 from a build on Ubuntu 24.04).
glibc=$(for p in "${programs[@]}"; do objdump -T "$tree/usr/bin/$p"; done \
  | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1)
[ -n "$glibc" ] || { echo "no glibc version found in the programs" >&2; exit 1; }
echo "the programs need glibc $glibc or newer"
cp -r "$root/target/extension/chromium" "$tree/usr/share/dannexdownloadmanager/extension-chromium"
cp "$root/target/extension/ddm-firefox.xpi" "$tree/usr/share/dannexdownloadmanager/"
cp "$root/LICENSE" "$root/ATTRIBUTION.md" "$tree/usr/share/doc/dannexdownloadmanager/"
python3 - "$root/art/Dannex Download Manager.png" \
  "$tree/usr/share/icons/hicolor/256x256/apps/dannexdownloadmanager.png" <<'EOF'
import sys
from PIL import Image
Image.open(sys.argv[1]).convert("RGBA").resize((256, 256), Image.LANCZOS).save(sys.argv[2])
EOF
cat > "$tree/usr/share/applications/dannexdownloadmanager.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Dannex Download Manager
Comment=Download manager
Exec=dannexdownloadmanager %U
Icon=dannexdownloadmanager
Terminal=false
Categories=Network;FileTransfer;
MimeType=x-scheme-handler/magnet;application/x-bittorrent;
EOF

# tar.gz: the programs, the extension and the licence in one folder.
plain="$stage/$name"
mkdir -p "$plain"
for p in "${programs[@]}"; do cp "$tree/usr/bin/$p" "$plain/"; done
cp -r "$tree/usr/share/dannexdownloadmanager/extension-chromium" "$plain/extension-chromium"
cp "$tree/usr/share/dannexdownloadmanager/ddm-firefox.xpi" "$plain/"
cp "$root/LICENSE" "$root/ATTRIBUTION.md" "$plain/"
tar -C "$stage" -czf "$dist/$name.tar.gz" "$name"

# .deb. Its dependencies, and the .rpm's: the libraries the window loads at run time. Without
# libxkbcommon-x11 (winit's keyboard under X11, and XWayland, DDM's default in a Wayland session)
# the window panics at start; EGL draws Wayland windows; without the CA certificates no download
# starts, HTTP ones too ("No CA certificates were loaded from the system"). GTK pulls in the rest.
# The browsers' host manifests for everyone (#266), in the .deb and .rpm only: each folder the
# browsers read them from (/etc/opt/chrome, /etc/chromium, /etc/opt/edge, /usr/lib/mozilla and
# /usr/lib64/mozilla), naming /usr/bin/dannexdownloadmanager-nmhost. The package owns them, so
# uninstalling it removes them (not conffiles: `apt remove` takes them too), and DDM writes none
# in the home folder for a browser that reads them. The AppImage and the tar.gz register per user.
system="$stage/system"
"$tree/usr/bin/dannexdownloadmanager-nmhost" --package-manifests "$system"
deb="$stage/deb"
cp -r "$tree" "$deb"
# Debian and Ubuntu's Firefox reads /usr/lib/mozilla; lib64's is Fedora's and openSUSE's.
cp -r "$system/." "$deb/"
rm -rf "$deb/usr/lib64"
mkdir -p "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<EOF
Package: dannexdownloadmanager
Version: $version
Architecture: amd64
Maintainer: Dannex Download Manager <https://github.com/Riskraptor/dannexdownloadmanager>
Depends: libc6 (>= $glibc), libgtk-3-0, libgl1, libegl1, libxkbcommon0, libxkbcommon-x11-0, ca-certificates
Section: net
Priority: optional
Homepage: https://github.com/Riskraptor/dannexdownloadmanager
Description: Download manager
 Speeds up downloads with several connections at once, resumes them, catches them from the
 browser, and downloads videos and torrents.
EOF
dpkg-deb --root-owner-group --build "$deb" "$dist/$name.deb"

# .rpm, from the same tree (nothing is compiled by rpmbuild).
rpm="$stage/rpm"
mkdir -p "$rpm"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
cat > "$rpm/SPECS/dannexdownloadmanager.spec" <<EOF
Name: dannexdownloadmanager
Version: $version
Release: 1
Summary: Download manager
License: GPL-3.0-or-later
URL: https://github.com/Riskraptor/dannexdownloadmanager
Requires: glibc >= $glibc, gtk3, mesa-libGL, mesa-libEGL, libxkbcommon, libxkbcommon-x11, ca-certificates
AutoReqProv: no

%description
Speeds up downloads with several connections at once, resumes them, catches them from the
browser, and downloads videos and torrents.

%install
cp -a "$tree/." %{buildroot}/
cp -a "$system/." %{buildroot}/

%files
/usr/bin/dannexdownloadmanager
/usr/bin/dannexdownloadmanager-nmhost
/usr/bin/dannexdownloadmanager-cli
/usr/bin/dannexdownloadmanager-mcp
/usr/share/applications/dannexdownloadmanager.desktop
/usr/share/icons/hicolor/256x256/apps/dannexdownloadmanager.png
/usr/share/dannexdownloadmanager
/usr/share/doc/dannexdownloadmanager
$(cd "$system" && find . -mindepth 1 -type d | sed 's|^\.||' | grep -Ev '^/(etc|etc/opt|usr|usr/lib|usr/lib64)$' | sort | sed 's|^|%dir |')
$(cd "$system" && find . -type f | sed 's|^\.||' | sort)
EOF
rpmbuild --define "_topdir $rpm" --define "_build_id_links none" -bb "$rpm/SPECS/dannexdownloadmanager.spec"
cp "$rpm"/RPMS/x86_64/*.rpm "$dist/$name.rpm"

# AppImage: the same tree as an AppDir.
appdir="$stage/AppDir"
cp -r "$tree" "$appdir"
ln -s usr/bin/dannexdownloadmanager "$appdir/AppRun"
cp "$tree/usr/share/applications/dannexdownloadmanager.desktop" "$appdir/"
cp "$tree/usr/share/icons/hicolor/256x256/apps/dannexdownloadmanager.png" "$appdir/"
tool="$stage/appimagetool"
python3 "$root/tools/installer/fetch_release.py" AppImage/appimagetool continuous appimagetool-x86_64.AppImage "$tool"
chmod +x "$tool"
# Update information (#24): AppImageLauncher, Gear Lever and AppImageUpdate read it from the
# AppImage, find the latest release's .zsync and download only the parts that changed. DDM's own
# Update now doesn't need it. The pattern also matches the names up to 0.2.4, with "-experimental".
update="gh-releases-zsync|Riskraptor|dannexdownloadmanager|latest|DannexDownloadManager-*-linux-x86_64*.AppImage.zsync"
# No FUSE on CI runners: the tool runs from its own extracted copy.
# zstd at its strongest level (#25; unpacking is as fast): the tool's 128K blocks stay, as 1M
# blocks make the AppImage start slower (AppImage/appimagetool#64).
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$tool" -u "$update" \
  --mksquashfs-opt -Xcompression-level --mksquashfs-opt 22 "$appdir" "$dist/$name.AppImage"
# appimagetool writes the .zsync itself (beside the AppImage, or in the current folder); with
# one that doesn't, zsyncmake makes it.
[ -f "$name.AppImage.zsync" ] && mv "$name.AppImage.zsync" "$dist/"
if [ ! -f "$dist/$name.AppImage.zsync" ]; then
  (cd "$dist" && zsyncmake -u "$name.AppImage" -o "$name.AppImage.zsync" "$name.AppImage")
fi

rm -rf "$stage"
(cd "$dist" && sha256sum "$name".* > SHA256SUMS-linux.txt)
ls -l "$dist"
