//! Subtitles: the arguments that ask for them, the side files they leave, translations kept with
//! their originals, and refusals retried later.

use super::*;

/// A subtitle language the site wouldn't give (#68).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubProblem {
    /// yt-dlp's code (`en`) and the site's name for it (`English`; the code when none).
    pub code: String,
    pub name: String,
    pub reason: SubReason,
    /// They came on the second try, but beside the video, not in it (they were to be embedded).
    pub beside: bool,
    /// The site's name for the subtitles saved in their place: the ones a refused machine
    /// translation was made from ([`SubReason::Translation`]).
    pub instead: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubReason {
    /// HTTP 429: the site limits how often subtitles may be fetched.
    TooManyRequests,
    /// The site's machine translation of another language's subtitles, which it refused
    /// (YouTube answers HTTP 429 "automated queries" to every program asking for one, at once
    /// and again later, while the language spoken and the uploader's subtitles come).
    Translation,
    /// yt-dlp's own words.
    Other(String),
}

/// A machine translation asked for, and the subtitles it is made from ([`translation_originals`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Original {
    /// The translation's code (`en`).
    pub(super) translated: String,
    /// The code of what it translates (`id-orig`: YouTube's captions of the language spoken).
    pub(super) original: String,
    /// That one is the uploader's (`--write-subs`), not an automatic caption.
    pub(super) uploaders: bool,
}

/// What a second, subtitles-only run needs (#68): what the first asked for, and its own
/// arguments (the output name, the browser's context, where ffmpeg is).
#[derive(Clone, Debug)]
pub(super) struct SubsRetry {
    pub(super) automatic: bool,
    pub(super) embed: bool,
    pub(super) ffmpeg: bool,
    pub(super) base: Vec<String>,
}

impl SubsRetry {
    /// yt-dlp's arguments to fetch `codes` again, beside the video (a second run can't put them
    /// into a video already saved).
    pub(super) fn args(&self, codes: &[String]) -> Vec<String> {
        let again = Subs {
            langs: codes.to_vec(),
            automatic: self.automatic,
            embed: false,
        };
        let mut out = self.base.clone();
        out.push("--skip-download".into());
        out.extend(subtitle_args(&again, self.ffmpeg));
        out
    }
}

/// Which subtitles to save with a video (#68). `langs` empty: none.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Subs {
    /// yt-dlp's language codes, or [`ALL_UPLOADERS`] alone for every one the uploader gave.
    pub langs: Vec<String>,
    /// The site's automatic captions rather than the uploader's.
    pub automatic: bool,
    /// Put into the video file instead of beside it (needs ffmpeg; otherwise beside it).
    pub embed: bool,
}

/// `Subs::langs` for all of the uploader's subtitles.
pub const ALL_UPLOADERS: &str = "all";

/// The files subtitles are saved as.
pub(super) const SUB_EXTS: [&str; 4] = ["srt", "vtt", "ass", "ttml"];

/// The `lang=` an address of a subtitle file asks for.
pub(super) fn lang_of(url: &str) -> Option<&str> {
    url.split(['?', '&']).find_map(|p| p.strip_prefix("lang="))
}

/// The subtitle languages `ours` (a run's arguments) asks for that are machine translations on
/// the looked-up page (`info`), each with the subtitles it is made from: of the same `lang=`,
/// not translated, YouTube's `<lang>-orig` first, the automatic captions before the uploader's.
pub(super) fn translation_originals(ours: &[String], info: &serde_json::Value) -> Vec<Original> {
    let Some(k) = ours.iter().position(|a| a == "--sub-langs") else {
        return Vec::new();
    };
    let asked: Vec<&str> = ours
        .get(k + 1)
        .map(|l| l.split(',').collect())
        .unwrap_or_default();
    let urls = |list: &str, code: &str| -> Vec<String> {
        info[list][code]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f["url"].as_str().map(str::to_owned))
            .collect()
    };
    let not_translated_of = |list: &str, lang: &str| -> Option<String> {
        let codes: Vec<&String> = info[list].as_object()?.keys().collect();
        let fits = |code: &str| {
            let u = urls(list, code);
            !u.is_empty()
                && u.iter()
                    .all(|u| !crate::resolve::is_translation(u) && lang_of(u) == Some(lang))
        };
        let orig = format!("{lang}-orig");
        codes
            .iter()
            .find(|c| **c == &orig && fits(c))
            .or_else(|| codes.iter().find(|c| fits(c)))
            .map(|c| (*c).clone())
    };
    let mut out: Vec<Original> = Vec::new();
    for &code in &asked {
        let translated = urls("automatic_captions", code);
        let Some(lang) = translated
            .iter()
            .find(|u| crate::resolve::is_translation(u))
            .and_then(|u| lang_of(u))
        else {
            continue;
        };
        let found = not_translated_of("automatic_captions", lang)
            .map(|c| (c, false))
            .or_else(|| not_translated_of("subtitles", lang).map(|c| (c, true)));
        // Asked for itself, it is no stand-in (and stays, whatever comes of the translation).
        if let Some((original, uploaders)) = found
            && !asked.contains(&original.as_str())
            && !out
                .iter()
                .any(|o| o.original == original && o.translated == code)
        {
            out.push(Original {
                translated: code.to_owned(),
                original,
                uploaders,
            });
        }
    }
    out
}

/// `ours` asking for `originals` too, beside the languages it asks for (the uploader's
/// subtitles as well when one of them is the uploader's).
pub(super) fn with_originals(ours: &[String], originals: &[Original]) -> Vec<String> {
    let mut out = ours.to_vec();
    let Some(k) = out.iter().position(|a| a == "--sub-langs") else {
        return out;
    };
    for o in originals {
        let langs = &mut out[k + 1];
        if !langs.split(',').any(|l| l == o.original) {
            langs.push(',');
            langs.push_str(&o.original);
        }
    }
    if originals.iter().any(|o| o.uploaders) && !out.iter().any(|a| a == "--write-subs") {
        out.insert(k, "--write-subs".into());
    }
    out
}

/// The subtitle file of language `code` saved beside `video` (`<stem>.<code>.<ext>`), removed:
/// a stand-in not needed after all.
pub(super) fn drop_side_file(video: &Path, code: &str) {
    let Some(stem) = video.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return;
    };
    let prefix = format!("{stem}.{code}.");
    for f in side_files(video) {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned());
        if name.is_some_and(|n| n.starts_with(&prefix) && !n[prefix.len()..].contains('.')) {
            let _ = std::fs::remove_file(&f);
        }
    }
}

/// How long to wait before asking once more for the subtitles a site refused. A site that
/// answers "too many requests" wants a pause, not a quick retry. `MDM_SUBS_RETRY_SECS`
/// shortens it for the tests (tests/fake_ytdlp.rs).
pub(super) fn subs_retry_delay() -> std::time::Duration {
    let secs = std::env::var("MDM_SUBS_RETRY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    std::time::Duration::from_secs(secs)
}

/// The subtitle languages yt-dlp said it couldn't download (with `--ignore-errors` it goes on
/// with the video and only warns: `Unable to download video subtitles for 'en': HTTP Error 429:
/// Too Many Requests`), and why.
pub(super) fn subtitle_failures(warnings: &[String]) -> Vec<(String, SubReason)> {
    const SAID: &str = "Unable to download video subtitles for '";
    let mut out: Vec<(String, SubReason)> = Vec::new();
    for w in warnings {
        let Some(rest) = w.split_once(SAID).map(|(_, r)| r) else {
            continue;
        };
        let Some((code, why)) = rest.split_once('\'') else {
            continue;
        };
        let why = why.trim_start_matches(':').trim();
        let reason =
            if why.contains("429") || why.to_ascii_lowercase().contains("too many requests") {
                SubReason::TooManyRequests
            } else {
                SubReason::Other(why.to_owned())
            };
        if !out.iter().any(|(c, _)| c == code) {
            out.push((code.to_owned(), reason));
        }
    }
    out
}

/// The site's name for a subtitle language, from the looked-up page (`code` when none).
pub(super) fn sub_name(info: &serde_json::Value, code: &str) -> String {
    ["subtitles", "automatic_captions"]
        .iter()
        .find_map(|k| info[k][code][0]["name"].as_str())
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| code.to_owned())
}

/// yt-dlp's arguments for `subs`: written beside the video as `<stem>.<lang>.srt` (converted to
/// SRT by ffmpeg, which every player reads; without ffmpeg SRT when the site has it, else its
/// own format, usually WebVTT), or embedded into the video with ffmpeg (yt-dlp removes the
/// separate files then).
pub(super) fn subtitle_args(subs: &Subs, ffmpeg: bool) -> Vec<String> {
    if subs.langs.is_empty() {
        return Vec::new();
    }
    let langs = if subs.langs.len() == 1 && subs.langs[0] == ALL_UPLOADERS {
        // Not the chat replay of a live stream, which yt-dlp lists with them.
        "all,-live_chat".to_owned()
    } else {
        subs.langs.join(",")
    };
    let embed = subs.embed && ffmpeg;
    let mut out: Vec<String> = Vec::new();
    // `--embed-subs` fetches the uploader's itself, and keeps no files behind when it is the
    // only one asked (`--write-subs` too would keep them).
    match (subs.automatic, embed) {
        (true, _) => out.push("--write-auto-subs".into()),
        (false, false) => out.push("--write-subs".into()),
        (false, true) => {}
    }
    out.extend(["--sub-langs".into(), langs]);
    if ffmpeg {
        out.extend([
            "--sub-format".into(),
            "srt/best".into(),
            "--convert-subs".into(),
            "srt".into(),
        ]);
    } else {
        out.extend(["--sub-format".into(), "srt/vtt/best".into()]);
    }
    if embed {
        out.push("--embed-subs".into());
    }
    // A subtitle the site refuses must not cost the video: yt-dlp stops on it unless told to
    // go on, and then only warns (`subtitle_failures`). Whether the video was saved is checked
    // on its own (`attempt`).
    out.push("--ignore-errors".into());
    out
}

/// The subtitle files saved beside `video` (`<stem>.<lang>.<srt|vtt…>`, as yt-dlp names them).
pub(super) fn side_files(video: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem()) else {
        return Vec::new();
    };
    let prefix = format!("{}.", stem.to_string_lossy());
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p != video && p.is_file())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            name.is_some_and(|n| n.starts_with(&prefix))
                && ext.is_some_and(|e| SUB_EXTS.contains(&e.as_str()))
        })
        .collect();
    out.sort();
    out
}

/// A subtitle refusal in the log's words.
pub(super) fn english(r: &SubReason) -> String {
    match r {
        SubReason::TooManyRequests => "the site refused (too many requests)".into(),
        SubReason::Translation => "the site refused its machine translation".into(),
        SubReason::Other(why) => why.clone(),
    }
}
