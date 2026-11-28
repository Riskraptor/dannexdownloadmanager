//! yt-dlp's folder build on Windows and macOS: unpacked once by DDM, so nothing is unpacked at
//! each start.
//!
//! yt-dlp's one-file programs (`yt-dlp.exe`, `yt-dlp_macos`) are a Python and its libraries in
//! one file, unpacked into a new temporary folder at every start: about half a second on Windows
//! before yt-dlp even begins (0.9 s against 0.4 s for `--version` on a recent PC), on each
//! look-up and each download.
//!
//! On macOS it was worse (#330, #339): an app still marked as downloaded by the browser passes
//! that mark to what it and its children create, so the Python just unpacked carries it, and
//! macOS refuses to load the library ("Python.framework is damaged", PYI-…): every look-up
//! failed. DDM has no hand in that temporary folder.
//!
//! The folder build (`yt-dlp_win.zip`, `yt-dlp_macos.zip`) unpacks nothing when it runs: DDM
//! unpacks it once, after checking the archive against yt-dlp's signed checksum list, into its
//! own tools folder. On macOS the mark is taken off those files and no others: never the app,
//! never a download (they are marked on purpose: `mark_downloaded`), never a program the person
//! chose. On Windows the program is marked as downloaded, as the one-file program was. Linux
//! uses its single-program build and doesn't compile the install (`#[cfg]` below); the
//! unpacking is portable and tested everywhere.
//!
//! An install of the one-file build (earlier versions of DDM) is replaced: on macOS at once (it
//! can't run there), on Windows by the next update, which takes the folder build even of the
//! same version (`lib.rs`, `install_latest`); until then the one-file program keeps working.

use std::path::{Component, Path};

use crate::{Error, Installed, make_current};

/// The folder build's program, beside its `_internal` folder.
#[cfg(not(target_os = "macos"))]
pub(crate) const PROGRAM: &str = "yt-dlp.exe";
#[cfg(target_os = "macos")]
pub(crate) const PROGRAM: &str = "yt-dlp_macos";
/// What the folder build keeps beside its program; the one-file build has none.
const INTERNAL: &str = "_internal";

fn bad(e: impl std::fmt::Display) -> Error {
    Error::BadRelease(e.to_string())
}

/// Whether `exe` is the one-file build, which earlier versions of DDM installed: nothing beside
/// it. DDM gets the folder build in its place.
pub(crate) fn one_file_build(exe: &Path) -> bool {
    !exe.with_file_name(INTERNAL).is_dir()
}

/// `from` renamed to `to`. On Windows a folder just written may be held for a moment by the
/// antivirus scanning its files, which refuses the rename: tried again for a few seconds then.
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    let tries = if cfg!(windows) { 40 } else { 1 };
    let mut last = None;
    for i in 0..tries {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename")))
}

/// Install the verified archive `zip` (downloaded from `from`) as `version` of yt-dlp
/// (`dir/yt-dlp/<version>/`), the files just unpacked made ready to run (`ready`), and make it
/// current. Unpacked to one side first, so a failure leaves what was installed. An earlier
/// install of the same version (the one-file build it replaces) is moved aside first: a run of it
/// still going (Windows lets a running program's folder move) ends from there, and the folder
/// goes with the next install (`make_current`) if it can't go now.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn put_folder(
    dir: &Path,
    version: &str,
    zip: &[u8],
    from: &str,
) -> Result<Installed, Error> {
    if version.is_empty() || version.contains(['/', '\\', ':']) || version.starts_with('.') {
        return Err(Error::BadRelease(format!("version {version:?}")));
    }
    let root = dir.join("yt-dlp");
    let unpacking = root.join(format!(".unpacking-{version}"));
    let replaced = root.join(format!(".replaced-{version}"));
    let _ = std::fs::remove_dir_all(&unpacking);
    let _ = std::fs::remove_dir_all(&replaced);
    std::fs::create_dir_all(&unpacking)?;
    let placed = unpack(zip, &unpacking)
        .and_then(|()| {
            if unpacking.join(PROGRAM).is_file() && unpacking.join(INTERNAL).is_dir() {
                Ok(())
            } else {
                Err(Error::Missing(format!(
                    "{PROGRAM} in the release's archive"
                )))
            }
        })
        .and_then(|()| {
            ready(&unpacking, from);
            let folder = root.join(version);
            // The same version again: the one-file build it replaces (`one_file_build`).
            let aside = folder.exists();
            if aside {
                rename(&folder, &replaced)?;
            }
            if let Err(e) = rename(&unpacking, &folder) {
                if aside {
                    let _ = rename(&replaced, &folder);
                }
                return Err(e.into());
            }
            // Gone now, or with the next install (`make_current` keeps only versions).
            let _ = std::fs::remove_dir_all(&replaced);
            Ok(folder)
        });
    let folder = match placed {
        Ok(folder) => folder,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&unpacking);
            return Err(e);
        }
    };
    make_current(
        &root,
        Installed {
            version: version.to_owned(),
            exe: folder.join(PROGRAM),
        },
    )
}

/// Unpack `zip` into `dest`, keeping the files' permissions. An entry that would land outside
/// `dest`, by its name or by a link, refuses the whole archive.
fn unpack(zip: &[u8], dest: &Path) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(bad)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        let Some(name) = entry.enclosed_name() else {
            return Err(bad("unsafe name in the archive"));
        };
        let path = dest.join(&name);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        if entry.is_symlink() {
            use std::io::Read;
            let mut target = String::new();
            entry.read_to_string(&mut target)?;
            if !link_stays_inside(&name, Path::new(&target)) {
                return Err(bad(format!("unsafe link in the archive: {target}")));
            }
            std::os::unix::fs::symlink(&target, &path)?;
            continue;
        }
        let mut out = std::fs::File::create(&path)?;
        std::io::copy(&mut entry, &mut out)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

/// Whether a link at `name` (relative to the archive's root) pointing at `target` stays within
/// the archive.
#[cfg_attr(not(unix), allow(dead_code))]
fn link_stays_inside(name: &Path, target: &Path) -> bool {
    let mut depth = name.components().count().saturating_sub(1);
    for c in target.components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => return false,
        }
    }
    true
}

/// macOS: take the mark off the verified files just unpacked in `folder` (recursively, links not
/// followed). The program is the system's own, not found through `PATH`. Whatever it says, the
/// install goes on: a file without the mark is fine, and a mark left behind makes the run fail
/// with its own message.
#[cfg(target_os = "macos")]
fn clear_mark(folder: &Path) {
    let _ = std::process::Command::new("/usr/bin/xattr")
        .args(["-drs", "com.apple.quarantine"])
        .arg(folder)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The verified files just unpacked in `folder` (from `from`), made ready to run: on macOS the
/// mark taken off them ([`clear_mark`]); elsewhere the program marked as downloaded, as a
/// one-file program is (`mark_downloaded`: Windows checks it like a browser download).
fn ready(folder: &Path, from: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = from;
        clear_mark(folder);
    }
    #[cfg(not(target_os = "macos"))]
    crate::mark_downloaded(&folder.join(PROGRAM), from);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Where the archive came from (its program is marked with it on Windows).
    const FROM: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/x.zip";

    /// An archive with the folder build's layout: `files` are (name, contents, mode).
    fn archive(files: &[(&str, &[u8], u32)], links: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data, mode) in files {
            let opts = zip::write::SimpleFileOptions::default().unix_permissions(*mode);
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        for (name, target) in links {
            zip.add_symlink(*name, *target, zip::write::SimpleFileOptions::default())
                .unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn good() -> Vec<u8> {
        archive(
            &[
                (PROGRAM, b"program", 0o755),
                ("_internal/Python", b"library", 0o644),
                ("_internal/lib/data.txt", b"data", 0o644),
            ],
            &[],
        )
    }

    #[test]
    fn the_folder_build_is_unpacked_as_one_and_made_current() {
        let dir = tempfile::tempdir().unwrap();
        let now = put_folder(dir.path(), "2026.10.26", &good(), FROM).unwrap();
        assert_eq!(now.version, "2026.10.26");
        assert_eq!(
            now.exe,
            dir.path().join("yt-dlp/2026.10.26").join(PROGRAM),
            "beside its folder, under its version"
        );
        assert_eq!(std::fs::read(&now.exe).unwrap(), b"program");
        assert_eq!(
            std::fs::read(now.exe.with_file_name("_internal/Python")).unwrap(),
            b"library"
        );
        assert!(!one_file_build(&now.exe));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&now.exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "the program runs");
        }
        // Nothing left to one side.
        let left: Vec<_> = std::fs::read_dir(dir.path().join("yt-dlp"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".unpacking"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn the_same_version_replaces_the_one_file_build() {
        let dir = tempfile::tempdir().unwrap();
        // Earlier versions of DDM: a single program, nothing beside it.
        let old = crate::put(
            dir.path(),
            "yt-dlp",
            "2026.10.26",
            PROGRAM,
            b"old one-file program",
            "https://x/",
        )
        .unwrap();
        assert!(one_file_build(&old.exe));
        let now = put_folder(dir.path(), "2026.10.26", &good(), FROM).unwrap();
        assert_eq!(now.exe, old.exe, "the same place");
        assert_eq!(std::fs::read(&now.exe).unwrap(), b"program");
        assert!(!one_file_build(&now.exe));
        // Nothing left to one side.
        let root = dir.path().join("yt-dlp");
        assert!(!root.join(".replaced-2026.10.26").exists());
        assert!(!root.join(".unpacking-2026.10.26").exists());
    }

    /// Windows: the one-file program it replaces may be running (a look-up, a download). Its
    /// folder is moved aside all the same (Windows lets a running program's folder move), the
    /// program going on from there, and the folder build takes its place at once; the folder
    /// moved aside goes once the program ended, with the next install. The program is marked as
    /// downloaded, as the one-file program was.
    #[cfg(windows)]
    #[test]
    fn a_running_one_file_build_is_replaced_at_once() {
        let dir = tempfile::tempdir().unwrap();
        // A real program in its place, running: Windows' own `ping`, waiting.
        let ping = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("PING.EXE");
        let old = crate::put(
            dir.path(),
            "yt-dlp",
            "2026.10.26",
            PROGRAM,
            &std::fs::read(&ping).unwrap(),
            "https://x/",
        )
        .unwrap();
        let mut running = std::process::Command::new(&old.exe)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let root = dir.path().join("yt-dlp");
        let now = put_folder(dir.path(), "2026.10.26", &good(), FROM).unwrap();
        assert_eq!(now.exe, old.exe, "the same place");
        assert_eq!(std::fs::read(&now.exe).unwrap(), b"program");
        assert!(!one_file_build(&now.exe));
        assert!(running.try_wait().unwrap().is_none(), "the old one goes on");
        let mut stream = now.exe.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let mark = std::fs::read_to_string(stream).unwrap();
        assert!(mark.contains("ZoneId=3") && mark.contains(FROM), "{mark}");
        running.kill().unwrap();
        running.wait().unwrap();
        // The next install clears what was moved aside.
        let next = put_folder(dir.path(), "2026.11.01", &good(), FROM).unwrap();
        assert_eq!(next.exe, root.join("2026.11.01").join(PROGRAM));
        assert!(!root.join(".replaced-2026.10.26").exists());
        assert!(
            root.join("2026.10.26").join(PROGRAM).is_file(),
            "the previous version stays"
        );
    }

    #[test]
    fn an_archive_without_the_program_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let zip = archive(&[("_internal/Python", b"x", 0o644)], &[]);
        assert!(put_folder(dir.path(), "2026.10.26", &zip, FROM).is_err());
        assert!(!dir.path().join("yt-dlp/current.json").exists());
        assert!(!dir.path().join("yt-dlp/2026.10.26").exists());
        assert!(!dir.path().join("yt-dlp/.unpacking-2026.10.26").exists());
    }

    #[test]
    fn nothing_lands_outside_the_install() {
        let dir = tempfile::tempdir().unwrap();
        let zip = archive(
            &[
                (PROGRAM, b"p", 0o755),
                ("_internal/ok", b"y", 0o644),
                ("../escaped", b"x", 0o644),
            ],
            &[],
        );
        assert!(put_folder(dir.path(), "2026.10.26", &zip, FROM).is_err());
        assert!(!dir.path().join("yt-dlp/escaped").exists());
        assert!(!dir.path().join("escaped").exists());
        assert!(put_folder(dir.path(), "../evil", &good(), FROM).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn links_inside_are_kept_and_links_out_refuse_the_archive() {
        let dir = tempfile::tempdir().unwrap();
        let inside = archive(
            &[
                (PROGRAM, b"p", 0o755),
                ("_internal/Versions/1/Python", b"lib", 0o644),
            ],
            &[("_internal/Python", "Versions/1/Python")],
        );
        let now = put_folder(dir.path(), "1", &inside, FROM).unwrap();
        assert_eq!(
            std::fs::read(now.exe.with_file_name("_internal/Python")).unwrap(),
            b"lib"
        );
        for target in ["/etc/passwd", "../../outside", "../../../outside"] {
            let out = archive(
                &[(PROGRAM, b"p", 0o755), ("_internal/x", b"x", 0o644)],
                &[("_internal/link", target)],
            );
            assert!(
                put_folder(dir.path(), "2", &out, FROM).is_err(),
                "{target} was accepted"
            );
        }
        assert!(link_stays_inside(
            Path::new("a/b/link"),
            Path::new("../c/file")
        ));
        assert!(!link_stays_inside(
            Path::new("link"),
            Path::new("../outside")
        ));
    }

    /// The mark comes off the unpacked files only (macOS, where `xattr` exists).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_mark_comes_off_what_was_unpacked_and_nothing_else() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let mark = |p: &Path| {
            let status = Command::new("/usr/bin/xattr")
                .args(["-w", "com.apple.quarantine", "0083;00000000;Test;"])
                .arg(p)
                .status()
                .unwrap();
            assert!(status.success());
        };
        let marked = |p: &Path| {
            Command::new("/usr/bin/xattr")
                .args(["-p", "com.apple.quarantine"])
                .arg(p)
                .output()
                .unwrap()
                .status
                .success()
        };
        // What a marked app leaves on what it creates, and a file that must keep its mark.
        let folder = dir.path().join("unpacked");
        std::fs::create_dir_all(folder.join("_internal")).unwrap();
        let (program, lib) = (folder.join(PROGRAM), folder.join("_internal/Python"));
        std::fs::write(&program, b"p").unwrap();
        std::fs::write(&lib, b"l").unwrap();
        let elsewhere = dir.path().join("a download");
        std::fs::write(&elsewhere, b"d").unwrap();
        for p in [&program, &lib, &elsewhere] {
            mark(p);
            assert!(marked(p));
        }
        clear_mark(&folder);
        assert!(!marked(&program));
        assert!(!marked(&lib));
        assert!(marked(&elsewhere), "only the helper's own folder");
    }
}
