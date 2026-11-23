//! DDM's own temporary files that no download owns any more (beyond IDM, which leaves them): part
//! and state files (`<file>.ddm.part`, `<file>.ddm.json`) and the engine's pieces folders
//! (`<file>.ddm.pieces`, #769), with their former `.mdm.` names, left by a crash, a power cut, a
//! row removed while its drive was unplugged, or a list restored from an older copy. Found and
//! removed beside the window (#743): at start (old ones only) and on the user's command
//! (Downloads → Clean Up Temporary Files). Nothing else is ever touched: never the user's files,
//! never a browser's or yt-dlp's `.part` file, never inside a folder other than a pieces folder.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A download in the list, as far as its temporary files go: where its file is, and whether it
/// is a video (yt-dlp's formats' pieces folders take its stem, `<stem>.f137.mp4.ddm.pieces`).
pub struct Owner {
    pub dest: PathBuf,
    pub video: bool,
}

/// Where to look and what is owned: the folders (each looked into, not below), Options'
/// temporary folder (its part files are named after their file and a hash of its path), every
/// download in the list, and how old a leftover must be (one being made now is no leftover).
pub struct Plan {
    pub folders: Vec<PathBuf>,
    pub temp: Option<PathBuf>,
    pub owners: Vec<Owner>,
    pub older_than: Duration,
}

/// What a sweep removed: how many, how many bytes, and how many couldn't go (in use).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Swept {
    pub count: usize,
    pub bytes: u64,
    pub failed: usize,
}

/// Comparison key: letter case aside (Windows' and macOS' file systems ignore it; on Linux it
/// only ever keeps more files, never removes one owned).
fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Whether `name` (lowercase) is one of DDM's temporary names: a file's part or state, or a
/// pieces folder (`dir`).
fn ddm_name(name: &str, dir: bool) -> bool {
    if dir {
        name.ends_with(".ddm.pieces") || name.ends_with(".mdm.pieces")
    } else {
        [".ddm.part", ".ddm.json", ".mdm.part", ".mdm.json"]
            .iter()
            .any(|s| name.ends_with(s))
    }
}

/// The leftovers `plan` finds, each with its size: DDM's temporary names in its folders that no
/// download owns, older than `plan.older_than`. Reads the disk: beside the window.
pub fn find(plan: &Plan) -> Vec<(PathBuf, u64)> {
    // Per folder, the names its downloads' temporary files start with (`<name>.`, a video's
    // `<stem>.` too).
    let mut prefixes: HashMap<String, Vec<String>> = HashMap::new();
    let mut owned: HashSet<String> = HashSet::new();
    for o in &plan.owners {
        let (Some(dir), Some(name)) = (o.dest.parent(), o.dest.file_name()) else {
            continue;
        };
        let list = prefixes.entry(key(dir)).or_default();
        list.push(format!("{}.", name.to_string_lossy().to_lowercase()));
        if o.video
            && let Some(stem) = o.dest.file_stem()
        {
            list.push(format!("{}.", stem.to_string_lossy().to_lowercase()));
        }
        if let Some(temp) = &plan.temp {
            owned.insert(key(&mdm_engine::naming::part_path_in(
                &o.dest,
                Some(temp.as_path()),
            )));
        }
    }
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for folder in plan.folders.iter().chain(plan.temp.iter()) {
        if !seen.insert(key(folder)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        let mine = prefixes.get(&key(folder));
        for e in entries.flatten() {
            // Never a link: what it points at isn't DDM's to remove.
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !ddm_name(&name, meta.is_dir()) {
                continue;
            }
            let path = e.path();
            let by_name = mine.is_some_and(|m| m.iter().any(|p| name.starts_with(p.as_str())));
            if by_name || owned.contains(&key(&path)) {
                // A download's own state says where its part file is (an earlier temporary
                // folder kept it there).
                if name.ends_with(".json")
                    && let Some(part) = mdm_engine::state::ResumeState::load(&path)
                        .ok()
                        .and_then(|s| s.part)
                {
                    owned.insert(key(&part));
                }
                continue;
            }
            candidates.push((path, meta));
        }
    }
    let now = SystemTime::now();
    candidates
        .into_iter()
        .filter(|(p, _)| !owned.contains(&key(p)))
        .filter(|(_, meta)| {
            meta.modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age >= plan.older_than)
        })
        .map(|(p, meta)| {
            let size = if meta.is_dir() {
                size_of_dir(&p)
            } else {
                meta.len()
            };
            (p, size)
        })
        .collect()
}

/// The bytes of the files in `dir` and below (a pieces folder holds files only).
fn size_of_dir(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| std::fs::symlink_metadata(e.path()).ok().map(|m| (e, m)))
                .map(|(e, m)| {
                    if m.is_dir() {
                        size_of_dir(&e.path())
                    } else if m.is_file() {
                        m.len()
                    } else {
                        0
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Remove the leftovers `plan` finds ([`find`]). Beside the window.
pub fn sweep(plan: &Plan) -> Swept {
    let mut swept = Swept::default();
    for (path, size) in find(plan) {
        let gone = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        match gone {
            Ok(()) => {
                swept.count += 1;
                swept.bytes += size;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::debug!("leftover {} not removed: {e}", path.display());
                swept.failed += 1;
            }
        }
    }
    swept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path, bytes: usize) {
        std::fs::write(p, vec![0u8; bytes]).unwrap();
    }

    /// Only DDM's own names that no download owns go; a download's own files, the user's files
    /// and other programs' `.part` files stay, and so does one too new to be a leftover.
    #[test]
    fn only_unowned_ddm_files_go() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let temp = d.join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        // Owned: a paused download's part and state (letter case aside), a video's format's
        // pieces folder.
        touch(&d.join("Paused.ZIP.ddm.part"), 5);
        touch(&d.join("paused.zip.ddm.json"), 2);
        std::fs::create_dir_all(d.join("Clip.f137.mp4.ddm.pieces")).unwrap();
        let in_temp = mdm_engine::naming::part_path_in(&d.join("intemp.iso"), Some(&temp));
        touch(&in_temp, 3);
        // Not DDM's: the user's file, a browser's and yt-dlp's part files.
        touch(&d.join("photo.jpg"), 1);
        touch(&d.join("film.mkv.part"), 1);
        touch(&d.join("Clip.f137.mp4.part"), 1);
        // Leftovers: a gone download's part, state and pieces folder, one in the temp folder,
        // one under the former name.
        touch(&d.join("gone.bin.ddm.part"), 100);
        touch(&d.join("gone.bin.ddm.json"), 10);
        let pieces = d.join("gone.bin.ddm.pieces");
        std::fs::create_dir_all(&pieces).unwrap();
        touch(&pieces.join("0"), 7);
        touch(&temp.join("old.zip.1f2e3d4c.ddm.part"), 20);
        touch(&d.join("ancient.exe.mdm.part"), 4);
        let plan = |older_than| Plan {
            folders: vec![d.to_owned(), d.to_owned()],
            temp: Some(temp.clone()),
            owners: vec![
                Owner {
                    dest: d.join("paused.zip"),
                    video: false,
                },
                Owner {
                    dest: d.join("Clip.mp4"),
                    video: true,
                },
                Owner {
                    dest: d.join("intemp.iso"),
                    video: false,
                },
            ],
            older_than,
        };
        // All new: none old enough.
        assert!(find(&plan(Duration::from_secs(3600))).is_empty());
        let swept = sweep(&plan(Duration::ZERO));
        assert_eq!(
            swept,
            Swept {
                count: 5,
                bytes: 100 + 10 + 7 + 20 + 4,
                failed: 0
            }
        );
        for stays in [
            d.join("Paused.ZIP.ddm.part"),
            d.join("paused.zip.ddm.json"),
            d.join("Clip.f137.mp4.ddm.pieces"),
            in_temp,
            d.join("photo.jpg"),
            d.join("film.mkv.part"),
            d.join("Clip.f137.mp4.part"),
        ] {
            assert!(stays.exists(), "{} stays", stays.display());
        }
        assert!(!pieces.exists() && !d.join("gone.bin.ddm.part").exists());
        assert_eq!(
            sweep(&plan(Duration::ZERO)),
            Swept::default(),
            "nothing left"
        );
    }

    /// A part file kept in an earlier temporary folder is its download's while the state beside
    /// the file says so.
    #[test]
    fn a_state_keeps_its_part_wherever_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let temp = d.join("temp");
        std::fs::create_dir_all(&temp).unwrap();
        let dest = d.join("big.iso");
        let part = temp.join("big.iso.00000000.ddm.part");
        touch(&part, 9);
        let url = url::Url::parse("https://example.com/big.iso").unwrap();
        let mut st = mdm_engine::state::ResumeState::new(
            url.clone(),
            url,
            dest.clone(),
            9,
            Default::default(),
        );
        st.set_part(&part);
        st.write(&mdm_engine::state::state_path(&dest)).unwrap();
        let plan = Plan {
            folders: vec![d.to_owned()],
            temp: Some(temp),
            owners: vec![Owner { dest, video: false }],
            older_than: Duration::ZERO,
        };
        assert_eq!(sweep(&plan).count, 0);
        assert!(part.exists());
    }
}
