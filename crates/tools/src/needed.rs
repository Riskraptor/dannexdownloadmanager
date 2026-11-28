//! The helpers DDM needs for videos and gets by itself, never asked (the owner's decision,
//! 2026-11-27: they come with the installation, as a modern app's parts do): yt-dlp everywhere,
//! and ffmpeg where DDM has a build of it to install (Windows; elsewhere the system's is used).
//! The Windows setup gets them as it installs (the app's `--install-helpers`); whatever is still
//! missing when DDM starts, it downloads in the background (the app's `autohelpers`).
//!
//! What counts as there is what the rest of DDM uses: the person's own program (Helpers...,
//! [`crate::chosen`]), else DDM's own copy, else, on Linux and macOS, the system's. Nothing is
//! downloaded over any of them.

use std::path::Path;

use crate::chosen::Helper;
use crate::{Error, Installed};

/// Whether DDM installs ffmpeg itself on this system: its builds are Windows ones. Elsewhere the
/// system's package manager has it, and the video window says so.
pub fn installs_ffmpeg() -> bool {
    cfg!(windows)
}

/// The helpers DDM needs and doesn't find in `dir` (its tools folder), in the order they are
/// got: yt-dlp first (the one every video needs), then ffmpeg.
pub fn missing(dir: &Path) -> Vec<Helper> {
    let mut list = Vec::new();
    if crate::installed(dir).is_none() {
        list.push(Helper::YtDlp);
    }
    if installs_ffmpeg() && crate::ffmpeg::installed(dir).is_none() {
        list.push(Helper::Ffmpeg);
    }
    list
}

/// Install `h` into `dir`: its newest release, checked as each helper's own install checks it
/// (yt-dlp's and gallery-dl's checksum lists signed by their developers' keys, ffmpeg's against
/// the checksums its release publishes and GitHub's own digest), downloaded with DDM's engine
/// (`net`) and counted into `progress`.
pub async fn install(
    h: Helper,
    dir: &Path,
    net: &mdm_engine::Options,
    progress: Option<&crate::update::Progress>,
) -> Result<Installed, Error> {
    match h {
        Helper::YtDlp => crate::install_latest(dir, net, progress).await,
        Helper::Ffmpeg => crate::ffmpeg::install_latest(dir, net, progress).await,
        Helper::GalleryDl => crate::gallery::install_latest(dir, net, progress).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty tools folder misses yt-dlp (unless the system has one, on Linux and macOS) and,
    /// on Windows only, ffmpeg; DDM's own copies and the person's own programs count as there.
    #[test]
    fn what_is_missing_follows_what_the_rest_of_ddm_uses() {
        let dir = tempfile::tempdir().unwrap();
        let empty = missing(dir.path());
        assert_eq!(empty.contains(&Helper::Ffmpeg), cfg!(windows));
        assert!(
            !empty.contains(&Helper::GalleryDl),
            "galleries ask for theirs"
        );
        #[cfg(windows)]
        {
            assert_eq!(empty, [Helper::YtDlp, Helper::Ffmpeg]);
            // DDM's own yt-dlp (a program in its version's folder, named current).
            crate::put(
                dir.path(),
                "yt-dlp",
                "2026.10.26",
                "yt-dlp.exe",
                b"x",
                "https://x/",
            )
            .unwrap();
            assert_eq!(missing(dir.path()), [Helper::Ffmpeg]);
            // The person's own ffmpeg (Helpers... → Use another program...): there, never
            // downloaded over.
            let mine = dir.path().join("my-ffmpeg.exe");
            std::fs::write(&mine, b"x").unwrap();
            std::fs::write(
                dir.path().join("programs.json"),
                serde_json::to_vec(&serde_json::json!({
                    "ffmpeg": { "version": "8.0", "exe": mine }
                }))
                .unwrap(),
            )
            .unwrap();
            assert!(missing(dir.path()).is_empty());
            // Gone from the disk: missing again.
            std::fs::remove_file(&mine).unwrap();
            assert_eq!(missing(dir.path()), [Helper::Ffmpeg]);
        }
    }
}
