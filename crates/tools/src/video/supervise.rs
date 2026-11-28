//! yt-dlp run and supervised: its arguments (`Plan`), the format selector and the nearest quality,
//! the attempts of a run and how each ended, its progress read as it goes, and yt-dlp and what it
//! started ended whenever the download is given up.

use super::*;

pub(super) type Child = Arc<Mutex<Option<proc::Spawned>>>;

/// How one run of yt-dlp ended.
pub(super) enum Attempt {
    /// The video saved, the subtitles that didn't come with it, and the quality yt-dlp chose
    /// (`137+251 1920x1080`).
    Done(PathBuf, Vec<SubProblem>, Option<String>),
    Stopped,
    /// `rerouted`: links were moved to other servers for this run.
    Failed {
        error: String,
        rerouted: bool,
    },
}

/// The most the bar shows while yt-dlp still works: all the bytes in is not the end, another
/// stream or the merge may follow (#344). The window's percentage takes it from here; the sizes
/// stay what they are.
pub const MAX_WHILE_RUNNING: f64 = 0.999;

/// How one yt-dlp run on the looked-up page ended.
pub(super) struct Ran {
    pub(super) status: std::io::Result<std::process::ExitStatus>,
    /// Where it said it saved the file (`MDMF`).
    pub(super) saved: Option<PathBuf>,
    /// The quality it chose (`MDMQ`).
    pub(super) quality: Option<String>,
    pub(super) warnings: Vec<String>,
    /// What it said when it failed.
    pub(super) failure: String,
}

/// yt-dlp's choice as a line of JSON: `MDMJ {…}` (`filename` and the chosen formats in it).
pub(super) const CHOICE: &str = "MDMJ %()j";

/// Our progress line: `MDMP <downloaded> <total> <estimate> <speed> <format id>` (`NA` when
/// unknown; the format id is the stream's, `NA` for a subtitle file).
pub(super) const PROGRESS: &str = "download:MDMP %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(info.format_id)s";

/// The final file: `MDMF <path>`.
pub(super) const FINAL: &str = "after_move:MDMF %(filepath)s";

/// The quality yt-dlp chose, before it downloads: `MDMQ <format id> <resolution>` (the log names
/// it when it isn't the one asked for, #94).
pub(super) const QUALITY: &str = "video:MDMQ %(format_id)s %(resolution)s";

/// What a run passes yt-dlp, made from where it goes (`dir/<stem>`): its own arguments, those
/// it chooses the engine's formats with (#769), the subtitles' second run (#68), and the size
/// the progress starts from.
pub(super) struct Plan {
    pub(super) ours: Vec<String>,
    pub(super) choose: Vec<String>,
    pub(super) retry: Option<SubsRetry>,
    pub(super) expected: Option<u64>,
}

impl Plan {
    /// For format `selector` into `dir/<stem>.<ext>` (`VideoJob::start_with`'s arguments),
    /// with what the browser's request carried (`theirs`); refused when an argument isn't one
    /// yt-dlp may be given (`proc::safe`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        selector: &str,
        dir: &Path,
        stem: &str,
        rate: Option<u64>,
        ffmpeg: Option<&Path>,
        expected: Option<u64>,
        subs: &Subs,
        theirs: &[String],
        engine: bool,
    ) -> Result<Plan, ProcError> {
        let output = dir.join(format!("{}.%(ext)s", template_text(stem)));
        // An MP3, Opus or FLAC choice (#32, #70): the best sound, converted by ffmpeg.
        let convert = crate::resolve::audio_conversion(selector);
        let format = if convert.is_some() {
            "ba/b".to_owned()
        } else if ffmpeg.is_none() {
            // A format that comes as a file first (#569): without ffmpeg an HLS stream stays
            // its raw pieces.
            crate::resolve::without_remuxing(selector)
        } else {
            selector.to_owned()
        };
        // What comes down is the sound as the site has it, not the file the choice's size
        // estimated: the progress follows yt-dlp's own total.
        let expected = if convert.is_some() { None } else { expected };
        // Subtitles go with a picture only (none for a sound).
        let with_subs = convert.is_none() && !subs.langs.is_empty();
        // How yt-dlp chooses the formats for the engine (#769): the same choice, merge and
        // conversion as the download's run, without downloading.
        let mut choose: Vec<String> = vec![
            "-f".into(),
            format.clone(),
            "-o".into(),
            output.to_string_lossy().into_owned(),
            "--no-playlist".into(),
        ];
        let mut ours: Vec<String> = vec![
            "-f".into(),
            format,
            "-o".into(),
            output.to_string_lossy().into_owned(),
            "--no-playlist".into(),
            "--newline".into(),
            // `--print` makes yt-dlp quiet; this keeps the progress lines.
            "--progress".into(),
            "--continue".into(),
            "--no-mtime".into(),
            "--progress-template".into(),
            PROGRESS.into(),
            "--print".into(),
            FINAL.into(),
            "--print".into(),
            QUALITY.into(),
        ];
        ours.extend(limit_args(rate));
        // A subtitles-only second run, for the ones a site refuses (#68): the same output name.
        let mut retry_base: Vec<String> = vec![
            "-o".into(),
            output.to_string_lossy().into_owned(),
            "--no-playlist".into(),
        ];
        match convert {
            Some(a) => {
                let x = ["-x".into(), "--audio-format".into(), a.format.into()];
                ours.extend(x.clone());
                choose.extend(x);
                if let Some(q) = a.quality {
                    ours.extend(["--audio-quality".into(), q]);
                }
            }
            None => ours.extend(subtitle_args(subs, ffmpeg.is_some())),
        }
        retry_base.extend(theirs.iter().cloned());
        ours.extend(theirs.iter().cloned());
        choose.extend(theirs.iter().cloned());
        if let Some(dir) = ffmpeg.and_then(Path::parent) {
            let merge = [
                "--ffmpeg-location".into(),
                dir.to_string_lossy().into_owned(),
                "--merge-output-format".into(),
                "mp4".into(),
            ];
            ours.extend(merge.clone());
            choose.extend(merge);
            retry_base.extend([
                "--ffmpeg-location".into(),
                dir.to_string_lossy().into_owned(),
            ]);
        }
        // With the engine, every HLS stream is yt-dlp's hlsnative's (`--hls-prefer-native`): the
        // engine joins its pieces as hlsnative does, and yt-dlp's fixups after it then follow
        // (a live one hlsnative hands to ffmpeg, as before).
        if engine {
            ours.push("--hls-prefer-native".into());
            choose.push("--hls-prefer-native".into());
        }
        let retry = with_subs.then(|| SubsRetry {
            automatic: subs.automatic,
            embed: subs.embed && ffmpeg.is_some(),
            ffmpeg: ffmpeg.is_some(),
            base: retry_base,
        });
        for a in retry.iter().flat_map(|r| r.base.iter()) {
            proc::safe(a)?;
        }
        for a in ours.iter().chain(&choose) {
            proc::safe(a)?;
        }
        // The engine's streams are there before yt-dlp's run: it fixes them up all the same
        // (`--fixup force`; by default only what it downloaded itself), as after its own
        // download: a DASH M4A sound, HLS pieces' MPEG-TS in an MP4.
        if engine {
            ours.extend(["--fixup".into(), "force".into()]);
        }
        Ok(Plan {
            ours,
            choose,
            retry,
            expected,
        })
    }
}

/// End yt-dlp and everything it started (#79): whenever the download is given up, nothing may
/// go on downloading unseen (Windows: its job, the launcher's child and ffmpeg with it).
pub(super) fn end(child: &Child) {
    let mut slot = child.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut s) = slot.take() {
        s.kill();
    }
}

/// The format selector in yt-dlp's arguments (`-f <selector>`).
pub(super) fn selector_of(ours: &[String]) -> Option<&str> {
    ours.iter()
        .position(|a| a == "-f")
        .and_then(|i| ours.get(i + 1))
        .map(String::as_str)
}

/// `ours` with another format selector.
pub(super) fn with_selector(ours: &[String], selector: &str) -> Vec<String> {
    let mut out = ours.to_vec();
    if let Some(i) = out.iter().position(|a| a == "-f")
        && let Some(s) = out.get_mut(i + 1)
    {
        *s = selector.to_owned();
    }
    out
}

/// The nearest quality to a choice from the window (`bv*[height=1080]+ba/b[height=1080]`) when
/// that one isn't offered (#94: the servers this PC reaches don't serve it): the best at or below
/// that height, else the best video there is, always a video (`b` alone would take a sound-only
/// format when no video is left; live 2026-11-05). None when the choice names no height.
pub(super) fn relaxed(selector: &str) -> Option<String> {
    selector
        .contains("[height=")
        .then(|| format!("{}/bv*+ba/bv*", selector.replace("[height=", "[height<=")))
}

/// Runs the download, and again when a run fails in a way a new look-up can mend:
/// - a media server yt-dlp couldn't reach (it resolved when looked up, then didn't: #94) is
///   treated as dead, so its streams move to an alternative;
/// - a run whose links had to be moved to other servers gets one fresh look-up (new links, often
///   other servers: YouTube's alternatives refuse some streams some of the time);
/// - the chosen quality not offered at all (left out by the move, #94) falls back to the nearest
///   one below it, said in the log; never the whole download's failure while another quality
///   is there;
/// - no video left at all after the move: once more on the page's own servers, unmoved.
///
/// Whenever it is given up, yt-dlp's whole job is ended (#79).
#[allow(clippy::too_many_arguments)]
pub(super) fn supervise(
    exe: &Path,
    url: &str,
    ours: &[String],
    browser: &[String],
    progress: &Mutex<Progress>,
    child: &Child,
    expected: Option<u64>,
    retry: Option<&SubsRetry>,
    preloaded: Option<Preloaded>,
    engine: Option<&ByEngine>,
) {
    // The first run only: a run after it looks the page up afresh (links expire).
    let mut preloaded = preloaded;
    let mut ours = ours.to_vec();
    let mut dead: std::collections::BTreeSet<String> = Default::default();
    let mut looked_again = false;
    let mut relaxed_to: Option<String> = None;
    let mut reroute = true;
    let mut last = String::from("yt-dlp stopped");
    for _ in 0..5 {
        let page = preloaded.take();
        match attempt(
            exe,
            url,
            &ours,
            browser,
            progress,
            child,
            expected,
            retry,
            &dead,
            reroute,
            page.as_ref(),
            engine,
        ) {
            Attempt::Stopped => return,
            Attempt::Done(f, problems, quality) => {
                end(child);
                // Named for what it holds (#569): HLS pieces joined without ffmpeg.
                let f = named_for_content(f);
                if relaxed_to.is_some()
                    && let Some(q) = quality
                {
                    note(progress, format!("Downloaded {q} instead"));
                }
                // The saved file's own size: a merged video or a converted MP3 isn't the size
                // of what came down.
                let saved = std::fs::metadata(&f).ok().map(|m| m.len());
                let side = side_files(&f);
                let mut p = progress.lock().unwrap();
                if p.state != State::Stopped {
                    p.speed = 0.0;
                    p.side_files = side;
                    p.sub_problems = problems;
                    if let Some(len) = saved {
                        p.total = Some(len);
                        p.exact = Some(true);
                        p.downloaded = len;
                    }
                    p.state = State::Done(f);
                }
                return;
            }
            Attempt::Failed { error, rerouted } => {
                end(child);
                let gone: Vec<String> = servers::unreachable_in(&error)
                    .into_iter()
                    .filter(|h| !dead.contains(h))
                    .collect();
                // On the page's own servers (no moving any more), an unreachable one is the end:
                // another look-up would go to it again (live 2026-11-05).
                if !gone.is_empty() && reroute {
                    for h in &gone {
                        note(
                            progress,
                            format!(
                                "The server {h} couldn't be reached; looking the page up again without it"
                            ),
                        );
                    }
                    dead.extend(gone);
                } else if servers::format_unavailable(&error) && rerouted && !looked_again {
                    looked_again = true;
                    note(progress, format!("{error}; looking the page up again"));
                } else if servers::format_unavailable(&error)
                    && relaxed_to.is_none()
                    && let Some(sel) = selector_of(&ours).and_then(relaxed)
                {
                    note(
                        progress,
                        "The chosen quality isn't offered by the servers this computer can reach; downloading the nearest quality below it".into(),
                    );
                    ours = with_selector(&ours, &sel);
                    relaxed_to = Some(sel);
                } else if servers::format_unavailable(&error) && rerouted && reroute {
                    note(
                        progress,
                        "No video the servers this computer can reach serve is left; trying the page's own servers".into(),
                    );
                    reroute = false;
                } else if rerouted && !looked_again {
                    looked_again = true;
                    note(progress, format!("{error}; looking the page up again"));
                } else {
                    last = error;
                    break;
                }
                last = error;
            }
        }
    }
    end(child);
    let mut p = progress.lock().unwrap();
    if p.state != State::Stopped {
        p.speed = 0.0;
        p.state = State::Failed(last);
    }
}

/// Run yt-dlp with `ours` on the page looked up into `file`, following its progress with
/// `meter`. None: stopped before or while it ran.
pub(super) fn run(
    exe: &Path,
    ours: &[String],
    file: &Path,
    progress: &Mutex<Progress>,
    child: &Child,
    mut meter: Option<&mut Meter>,
) -> Result<Option<Ran>, String> {
    let stdout = {
        // Under the child's lock: stop() either came first (nothing starts) or finds the child
        // to kill.
        let mut slot = child.lock().unwrap();
        if progress.lock().unwrap().state == State::Stopped {
            return Ok(None);
        }
        let spawned =
            proc::ytdlp_info(exe, &[], ours, file).and_then(|cmd| proc::spawn(cmd, "yt-dlp"));
        match spawned {
            Ok(mut s) => {
                let out = s.child.stdout.take().expect("piped");
                *slot = Some(s);
                out
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut saved = None;
    let mut quality = None;
    // Every line to the end, read as bytes (#79): a line that isn't UTF-8 (a title in a legacy
    // code page) used to end the reading there, while yt-dlp went on; it is read leniently now,
    // and the saved file is also found by its name (`attempt`).
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&buf);
        let line = text.trim_end_matches(['\r', '\n']);
        if let Some(rest) = line.strip_prefix("MDMF ") {
            saved = Some(PathBuf::from(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("MDMQ ") {
            if let Some(m) = meter.as_deref_mut() {
                m.quality(rest);
            }
            quality = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("MDMP ")
            && let Some(m) = meter.as_deref_mut()
        {
            m.line(rest, &mut progress.lock().unwrap());
        }
    }
    // Poll (the lock is free in between, so stop() can still kill it).
    let status = loop {
        let mut slot = child.lock().unwrap();
        let Some(s) = slot.as_mut() else {
            break Err(std::io::Error::other("yt-dlp is gone"));
        };
        match s.child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        drop(slot);
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    std::thread::sleep(std::time::Duration::from_millis(50));
    if progress.lock().unwrap().state == State::Stopped {
        return Ok(None);
    }
    let slot = child.lock().unwrap();
    let (warnings, failure) = slot.as_ref().map_or_else(
        || (Vec::new(), "yt-dlp stopped".to_owned()),
        |s| (s.warnings(), s.failure().to_string()),
    );
    Ok(Some(Ran {
        status,
        saved,
        quality,
        warnings,
        failure,
    }))
}

/// Wait `d`, or less if the download is stopped meanwhile (false then).
pub(super) fn pause(d: std::time::Duration, progress: &Mutex<Progress>) -> bool {
    let end = std::time::Instant::now() + d;
    while std::time::Instant::now() < end {
        if progress.lock().unwrap().state == State::Stopped {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    progress.lock().unwrap().state != State::Stopped
}

/// One run: look the page up (or take the quality menu's look-up, `preloaded`), move links off
/// unreachable servers (and off `dead`, the ones an earlier run couldn't reach), download from
/// that list. Subtitles the site refused are asked for once more after a pause (`retry`), beside
/// the video; the ones still refused are reported with it, never failing it.
#[allow(clippy::too_many_arguments)]
pub(super) fn attempt(
    exe: &Path,
    url: &str,
    ours: &[String],
    browser: &[String],
    progress: &Mutex<Progress>,
    child: &Child,
    expected: Option<u64>,
    retry: Option<&SubsRetry>,
    dead: &std::collections::BTreeSet<String>,
    reroute: bool,
    preloaded: Option<&Preloaded>,
    engine: Option<&ByEngine>,
) -> Attempt {
    let failed = |error: String, rerouted: bool| Attempt::Failed { error, rerouted };
    let page = match preloaded.and_then(|p| preloaded_page(p, progress)) {
        Some(page) => Ok(page),
        // Stopped while waiting for it: nothing more to do.
        None if progress.lock().unwrap().state == State::Stopped => return Attempt::Stopped,
        None => look_up(exe, url, browser),
    };
    let mut info = match page {
        Ok((v, without_cookies)) => {
            if without_cookies {
                note(
                    progress,
                    "The site offered no format with the browser's cookies; looked the page up without them".into(),
                );
            }
            v
        }
        Err(e) => return failed(e, false),
    };
    let changed = moved(&mut info, dead, reroute);
    for (gone, alt) in &changed.swaps {
        note(
            progress,
            format!("The server {gone} can't be reached; downloading from {alt}"),
        );
    }
    if !changed.left_out.is_empty() {
        note(
            progress,
            format!(
                "Not served by the servers this computer can reach, so left out: {}",
                changed.left_out.join(", ")
            ),
        );
    }
    let rerouted = !changed.is_empty();
    let file = info_file();
    if let Err(e) = std::fs::write(&file, info.to_string()) {
        return failed(format!("Cannot write the video information: {e}"), rerouted);
    }
    let _cleanup = Remove(file.clone());
    // DDM's engine downloads the streams (#769); yt-dlp then finds them in place.
    let mut by_engine = false;
    if let Some(e) = engine {
        match e.fetch(exe, url, browser, &file, progress, dead, reroute) {
            Engined::Ready => by_engine = true,
            Engined::Helper(why) => {
                note(
                    progress,
                    format!("yt-dlp downloads this video itself: {}", why.english()),
                );
                progress.lock().unwrap().helper = Some(why);
            }
            Engined::Stopped => return Attempt::Stopped,
            Engined::Failed(error) => return failed(error, rerouted),
        }
    }
    let mut meter = Meter::new(expected);
    // The streams are in place: what yt-dlp says of them isn't the download's progress.
    let meter = (!by_engine).then_some(&mut meter);
    // A machine translation asked for (YouTube's English of an Indonesian video's captions)
    // comes with the captions it translates, in the same run: YouTube refuses its translations
    // to programs, and those are then saved in its place (or into the video, as asked).
    let originals = translation_originals(ours, &info);
    let ours = with_originals(ours, &originals);
    let ours = ours.as_slice();
    let ran = match run(exe, ours, &file, progress, child, meter) {
        Ok(Some(r)) => r,
        Ok(None) => return Attempt::Stopped,
        Err(e) => return failed(e, rerouted),
    };
    let quality = ran.quality.clone();
    let video = match (&ran.status, ran.saved) {
        // `--ignore-errors` (subtitles asked) lets yt-dlp end well with no file: the file
        // itself is the proof.
        (Ok(s), Some(f)) if s.success() && f.is_file() => f,
        // It said where, in a form that names no file (#79), or didn't say: our own name for it.
        (Ok(s), _) if s.success() && saved_by_name(ours).is_some() => {
            saved_by_name(ours).expect("just found")
        }
        (Ok(s), Some(_)) if s.success() => {
            return failed(
                "yt-dlp said it saved the file, but it isn't there".into(),
                rerouted,
            );
        }
        (Ok(s), None) if s.success() => {
            return failed("yt-dlp didn't say where it saved the file".into(), rerouted);
        }
        _ => return failed(ran.failure, rerouted),
    };
    let mut refused = subtitle_failures(&ran.warnings);
    // The translations asked for: refused, the captions they translate stand in for them; given,
    // those captions go (they were only asked as a stand-in), unless they are inside the video.
    let mut problems = Vec::new();
    let stood_in = |original: &str| {
        originals
            .iter()
            .any(|o| o.original == original && refused.iter().any(|(c, _)| *c == o.translated))
    };
    let unneeded: Vec<String> = originals
        .iter()
        .filter(|o| !stood_in(&o.original))
        .map(|o| o.original.clone())
        .collect();
    if !retry.is_some_and(|r| r.embed) {
        for code in &unneeded {
            drop_side_file(&video, code);
        }
    }
    for o in &originals {
        let Some(k) = refused.iter().position(|(c, _)| *c == o.translated) else {
            continue;
        };
        refused.remove(k);
        let came = !refused.iter().any(|(c, _)| *c == o.original);
        let instead = came.then(|| sub_name(&info, &o.original));
        note(
            progress,
            match &instead {
                Some(_) => format!(
                    "The site refused its translation of the subtitles into {}; the ones in {} were saved instead",
                    o.translated, o.original
                ),
                None => format!(
                    "The site refused its translation of the subtitles into {}",
                    o.translated
                ),
            },
        );
        problems.push(SubProblem {
            code: o.translated.clone(),
            name: sub_name(&info, &o.translated),
            reason: SubReason::Translation,
            beside: false,
            instead,
        });
    }
    if refused.is_empty() {
        return Attempt::Done(video, problems, quality);
    }
    for (code, why) in &refused {
        note(
            progress,
            format!(
                "Subtitles in {code} couldn't be downloaded: {}",
                english(why)
            ),
        );
    }
    // Once more after a pause, for the ones refused (a site limiting requests often gives them
    // a little later). Not a translation, which the site refuses again (measured on YouTube).
    let mut still = refused.clone();
    if let Some(r) = retry {
        let delay = subs_retry_delay();
        note(
            progress,
            format!("Asking for the subtitles again in {} s", delay.as_secs()),
        );
        if !pause(delay, progress) {
            return Attempt::Done(video, problems, quality);
        }
        let codes: Vec<String> = refused.iter().map(|(c, _)| c.clone()).collect();
        match run(exe, &r.args(&codes), &file, progress, child, None) {
            Ok(Some(again)) if again.status.as_ref().is_ok_and(|s| s.success()) => {
                still = subtitle_failures(&again.warnings);
            }
            Ok(None) => return Attempt::Done(video, problems, quality.clone()),
            _ => {}
        }
    }
    for (code, why) in &refused {
        let gone = still.iter().find(|(c, _)| c == code);
        match gone {
            Some((_, why)) => problems.push(SubProblem {
                code: code.clone(),
                name: sub_name(&info, code),
                reason: why.clone(),
                beside: false,
                instead: None,
            }),
            None => {
                note(
                    progress,
                    format!("Subtitles in {code} came on the second try"),
                );
                // Asked into the video, they're beside it now.
                if retry.is_some_and(|r| r.embed) {
                    problems.push(SubProblem {
                        code: code.clone(),
                        name: sub_name(&info, code),
                        reason: why.clone(),
                        beside: true,
                        instead: None,
                    });
                }
            }
        }
    }
    Attempt::Done(video, problems, quality)
}

/// yt-dlp's choice (`-O "%()j"` after its format selection) on the page looked up into `file`,
/// with `args` (the choice's `-f`, `-o`, merge and conversion, the browser's context).
pub(super) fn choose(
    exe: &Path,
    args: &[String],
    file: &Path,
) -> Result<serde_json::Value, String> {
    let mut ours = args.to_vec();
    ours.extend(["-O".into(), CHOICE.into()]);
    let cmd = proc::ytdlp_info(exe, &[], &ours, file).map_err(|e| e.to_string())?;
    let out = proc::output(cmd, "yt-dlp", Duration::from_secs(90)).map_err(|e| e.to_string())?;
    String::from_utf8_lossy(&out)
        .lines()
        .find_map(|l| l.strip_prefix("MDMJ "))
        .and_then(|j| serde_json::from_str(j).ok())
        .ok_or_else(|| "yt-dlp gave no choice of formats".to_owned())
}

/// A line for the download's log (English, as yt-dlp's own).
pub(super) fn note(progress: &Mutex<Progress>, text: String) {
    progress
        .lock()
        .unwrap()
        .events
        .push((std::time::SystemTime::now(), text));
}

/// yt-dlp's speed when it downloads by itself (#708; DDM's engine downloads every other video,
/// #769): at most `rate` bytes/s.
pub fn limit_args(rate: Option<u64>) -> Vec<String> {
    rate.map(|r| vec!["--limit-rate".into(), r.to_string()])
        .unwrap_or_default()
}

/// A job's run on its own thread: yt-dlp supervised as `plan` says (`theirs`: what the browser's
/// request carried). A panic in it ends the download, never leaving it "running" (#79).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_job(
    exe: &Path,
    url: &str,
    plan: &Plan,
    theirs: &[String],
    progress: &Mutex<Progress>,
    child: &Child,
    preloaded: Option<Preloaded>,
    by_engine: Option<ByEngine>,
) {
    let supervised = std::panic::AssertUnwindSafe(|| {
        supervise(
            exe,
            url,
            &plan.ours,
            theirs,
            progress,
            child,
            plan.expected,
            plan.retry.as_ref(),
            preloaded,
            by_engine.as_ref(),
        )
    });
    if std::panic::catch_unwind(supervised).is_err() {
        // Given up: nothing may go on downloading unseen (#79).
        end(child);
        if let Some(b) = &by_engine {
            b.current.cancel();
        }
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        if p.state == State::Running {
            p.speed = 0.0;
            p.state = State::Failed("the video download stopped on an internal error".into());
        }
    }
}
