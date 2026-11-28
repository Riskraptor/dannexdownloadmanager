//! DDM's engine for a video's streams (#769): the formats yt-dlp chose downloaded into the files it
//! looks for, and what the engine's part of a run ended with.

use super::*;

/// DDM's engine for a video's streams (#769): the engine itself, the arguments yt-dlp chooses
/// the formats with (`choose`), and the download's engine part under way.
pub(crate) struct ByEngine {
    pub(super) engine: crate::streams::Engine,
    pub(super) choose: Vec<String>,
    pub(super) current: Arc<crate::streams::Current>,
}

/// How the engine's part of a run ended.
pub(super) enum Engined {
    /// Every stream is in place for yt-dlp.
    Ready,
    /// yt-dlp downloads this choice itself.
    Helper(crate::streams::Helper),
    Stopped,
    Failed(String),
}

/// The engine's progress into the download's (`Progress`).
pub(super) struct Reporter<'a>(pub(super) &'a Mutex<Progress>);

impl crate::streams::Report for Reporter<'_> {
    fn progress(
        &self,
        downloaded: u64,
        total: Option<u64>,
        exact: bool,
        speed: f64,
        view: crate::streams::View,
    ) {
        let mut p = self.0.lock().unwrap();
        p.downloaded = downloaded;
        p.total = total;
        p.exact = total.map(|_| exact);
        p.speed = speed;
        p.engine = Some(view);
    }

    fn note(&self, text: String) {
        note(self.0, text);
    }

    fn stopped(&self) -> bool {
        self.0.lock().unwrap().state == State::Stopped
    }
}

impl ByEngine {
    /// Choose the formats on the page looked up into `file` and download their streams with the
    /// engine; a stream's address that dies is asked for again with a new look-up (`url`, as
    /// the browser asked, `browser`; servers moved as this run moved them).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fetch(
        &self,
        exe: &Path,
        url: &str,
        browser: &[String],
        file: &Path,
        progress: &Mutex<Progress>,
        dead: &std::collections::BTreeSet<String>,
        reroute: bool,
    ) -> Engined {
        let chosen = match choose(exe, &self.choose, file) {
            Ok(c) => c,
            Err(e) => return Engined::Failed(e),
        };
        if progress.lock().unwrap().state == State::Stopped {
            return Engined::Stopped;
        }
        let plan = match crate::streams::plan(&chosen) {
            Ok(p) => p,
            Err(why) => return Engined::Helper(why),
        };
        let Ok(page) = url::Url::parse(url) else {
            return Engined::Failed("not a web address".into());
        };
        {
            let mut p = progress.lock().unwrap();
            p.helper = None;
            if let Some(total) = plan.total() {
                p.total = Some(total);
                p.exact = Some(plan.streams.iter().all(|s| s.exact));
            }
        }
        let relink: crate::streams::Relink = {
            let (exe, url, browser) = (exe.to_owned(), url.to_owned(), browser.to_vec());
            let (choose, dead) = (self.choose.clone(), dead.clone());
            Arc::new(move |id: &str| relink(&exe, &url, &browser, &choose, &dead, reroute, id))
        };
        let out = crate::streams::fetch(
            &plan,
            &self.engine,
            &page,
            &relink,
            &self.current,
            &Reporter(progress),
        );
        progress.lock().unwrap().speed = 0.0;
        match out {
            crate::streams::Fetched::Done => Engined::Ready,
            crate::streams::Fetched::Stopped => Engined::Stopped,
            crate::streams::Fetched::Failed(e) => Engined::Failed(e),
            crate::streams::Fetched::Helper(why) => Engined::Helper(why),
        }
    }
}
