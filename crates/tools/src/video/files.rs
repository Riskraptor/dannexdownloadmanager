//! The files a run leaves: those of a download (its media, subtitles and yt-dlp's leftovers, never
//! another download's), and the media file found by name or content when yt-dlp didn't say.

use super::*;

/// The containers and sounds yt-dlp saves (`%(ext)s`), merged, converted or as the site has them.
pub(super) const MEDIA: [&str; 19] = [
    "mp4", "m4a", "m4v", "webm", "weba", "mkv", "mka", "mov", "flv", "3gp", "ts", "mp3", "opus",
    "ogg", "oga", "flac", "wav", "aac", "avi",
];

/// The subtitle files yt-dlp saves beside a video (#68).
pub(super) const SUBTITLES: [&str; 10] = [
    "srt", "vtt", "ass", "ssa", "ttml", "srv1", "srv2", "srv3", "json3", "lrc",
];

/// Removes a file when dropped.
pub(super) struct Remove(pub(super) PathBuf);

impl Drop for Remove {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Whether `rest`, a file name after `<stem>.` (lower case), is one yt-dlp writes for a download
/// saved as `<stem>.%(ext)s`: an unfinished run's (`<ext>.part`, `<ext>.ytdl`,
/// `<ext>.part-Frag<N>[.part]`, each also for one stream of a merged choice as `f<id>.<ext>…`,
/// that stream once whole, `f<id>.<ext>`, and a merge's `temp.<ext>`; DDM's engine's part file
/// and resume state for such a stream, `f<id>.<ext>.ddm.part` and `.ddm.json`, #769, also under
/// their former names, `.mdm.part` and `.mdm.json`); with `outputs`, also the finished file
/// (`<ext>`) and its subtitles (`<language>.<srt|vtt…>`). Nothing else: another download's files
/// (DDM's own `.ddm.part`), a file merely named alike.
pub(super) fn yt_dlp_leftover(rest: &str, outputs: bool) -> bool {
    let media = |s: &str| MEDIA.contains(&s);
    let word = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    // A format id as yt-dlp names a stream: `137`, `hls-720p`, `dash-1`, `http-480`.
    let format = |s: &str| {
        s.strip_prefix('f').is_some_and(|id| {
            word(id)
                && (id.starts_with(|c: char| c.is_ascii_digit())
                    || ["hls-", "dash-", "http-"].iter().any(|p| {
                        id.strip_prefix(p)
                            .is_some_and(|r| r.chars().any(|c| c.is_ascii_digit()))
                    }))
        })
    };
    let fragment = |s: &str| {
        s.strip_prefix("part-frag")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    let parts: Vec<&str> = rest.split('.').collect();
    // An unfinished file: `<ext>` then `part`, `ytdl`, `part-frag<N>` or `part-frag<N>.part`.
    let unfinished = |p: &[&str]| match p {
        [ext, "part" | "ytdl"] => media(ext),
        [ext, frag] => media(ext) && fragment(frag),
        [ext, frag, "part"] => media(ext) && fragment(frag),
        _ => false,
    };
    let working = match parts.as_slice() {
        ["temp", ext] => media(ext),
        [f, ext] if format(f) => media(ext),
        [f, ext, "ddm" | "mdm", "part" | "json"] if format(f) => media(ext),
        [f, tail @ ..] if format(f) => unfinished(tail),
        p => unfinished(p),
    };
    let finished = outputs
        && match parts.as_slice() {
            [ext] => media(ext),
            [language, sub] => word(language) && SUBTITLES.contains(sub),
            _ => false,
        };
    working || finished
}

/// What yt-dlp left in `dir` for a download saved as `<stem>.%(ext)s` (our template), as
/// [`yt_dlp_leftover`] tells them, letter case aside: what an unfinished run left, and with
/// `outputs` the finished file and its subtitles. A file not of that shape is left alone.
pub fn own_files(dir: &Path, stem: &str, outputs: bool) -> Vec<PathBuf> {
    let prefix = format!("{}.", stem.to_lowercase());
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.strip_prefix(&prefix)
                .is_some_and(|rest| yt_dlp_leftover(rest, outputs))
        })
        .map(|e| e.path())
        .collect()
}

/// The pieces folders DDM's engine left in `dir` for a video saved as `<stem>.%(ext)s` (#769):
/// `<stem>.<ext>.ddm.pieces` or, for one stream of a merged choice, `<stem>.f<id>.<ext>.ddm.pieces`
/// (also under the former `.mdm.pieces`), letter case aside. Any other folder is left alone.
pub fn own_folders(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let prefix = format!("{}.", stem.to_lowercase());
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.strip_prefix(&prefix)
                .and_then(|rest| {
                    rest.strip_suffix(".ddm.pieces")
                        .or_else(|| rest.strip_suffix(".mdm.pieces"))
                })
                .is_some_and(|stream| {
                    let parts: Vec<&str> = stream.split('.').collect();
                    match parts.as_slice() {
                        [ext] => MEDIA.contains(ext),
                        [f, _] => yt_dlp_leftover(stream, false) && f.starts_with('f'),
                        _ => false,
                    }
                })
        })
        .map(|e| e.path())
        .collect()
}

/// The media file yt-dlp saved under our output name (`<dir>/<stem>.%(ext)s`), when it didn't say
/// where, or said it in a form that names no file (#79). Its subtitles (`<stem>.<lang>.srt`) and
/// unfinished parts don't count; the newest one wins.
pub(super) fn saved_by_name(ours: &[String]) -> Option<PathBuf> {
    const MEDIA: [&str; 15] = [
        "mp4", "mkv", "webm", "mov", "m4v", "m4a", "mp3", "opus", "ogg", "oga", "flac", "wav",
        "aac", "flv", "3gp",
    ];
    let template = ours
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| ours.get(i + 1))?;
    let t = Path::new(template);
    let dir = t.parent()?;
    let stem = t
        .file_name()?
        .to_string_lossy()
        .strip_suffix(".%(ext)s")?
        .replace("%%", "%");
    MEDIA
        .iter()
        .map(|ext| dir.join(format!("{stem}.{ext}")))
        .filter(|p| p.is_file())
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// What a saved file holds when its name says otherwise: an HLS stream's pieces joined as they
/// came, which only ffmpeg makes into the MP4 they're named for (#569). YouTube's HLS sound is
/// raw AAC (ADTS frames, each piece led by an ID3 tag with its time): `aac`, which players open.
/// A video's pieces are an MPEG transport stream: `ts`. A real MP4 (ffmpeg was there, or the
/// format came as a file) is none of these.
pub(super) fn content_ext(head: &[u8]) -> Option<&'static str> {
    // ID3v2 tags first: a 10-byte header, then the tag's size in 7-bit bytes.
    let mut at = 0usize;
    while head.len() >= at + 10 && &head[at..at + 3] == b"ID3" {
        let size = head[at + 6..at + 10]
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(b & 0x7f));
        let footer = if head[at + 5] & 0x10 != 0 { 10 } else { 0 };
        at += 10 + size + footer;
    }
    let rest = head.get(at..)?;
    // ADTS: a 12-bit sync word, then MPEG layer 0.
    if rest.len() >= 7 && rest[0] == 0xff && rest[1] & 0xf6 == 0xf0 {
        return Some("aac");
    }
    // A transport stream: a sync byte every 188 bytes.
    if at == 0 && head.len() > 376 && [0, 188, 376].iter().all(|&i| head[i] == 0x47) {
        return Some("ts");
    }
    None
}

/// `f`, renamed for what it holds when its MP4 name is wrong ([`content_ext`]); as it was
/// otherwise, or when the rename fails (the file is kept).
pub(super) fn named_for_content(f: PathBuf) -> PathBuf {
    let named = f
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    if !matches!(named.as_deref(), Some("mp4" | "m4a")) {
        return f;
    }
    let mut head = Vec::with_capacity(8192);
    let read = std::fs::File::open(&f).and_then(|file| {
        use std::io::Read;
        file.take(8192).read_to_end(&mut head)
    });
    let Some(ext) = read.ok().and_then(|_| content_ext(&head)) else {
        return f;
    };
    let stem = f
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let free = (1..100)
        .map(|n| match n {
            1 => f.with_file_name(format!("{stem}.{ext}")),
            n => f.with_file_name(format!("{stem} ({n}).{ext}")),
        })
        .find(|p| !p.exists());
    match free {
        Some(to) if std::fs::rename(&f, &to).is_ok() => to,
        _ => f,
    }
}

/// `%` would be read as a template field by yt-dlp.
pub(super) fn template_text(s: &str) -> String {
    s.replace('%', "%%")
}
