//! The progress of a run as the window shows it: yt-dlp's lines and the engine's streams read into
//! one size, speed and time left.

use super::*;

/// yt-dlp's progress lines (`MDMP <downloaded> <total> <estimate> <speed> <format id>`) made
/// into one steady figure. Each stream of a merged download (video, then audio) reports from 0
/// with its own total, and a fragmented stream's total (HLS, DASH) is an estimate that moves with
/// every fragment: the streams are added up, and the fraction done goes back only when a stream's
/// end had looked like the whole (no `expected`, the next stream starts).
///
/// The size the video window showed (`expected`, every stream) holds the total up until the last
/// stream: a stream's end before it never looks like the whole download (#344). That size is
/// often an estimate (an HLS stream's bitrate, a peak, times its length) well above what comes:
/// in the last stream, once yt-dlp knows that stream's own total, the total is what came before
/// and that stream's, so the bar reaches the end. Which streams there are, yt-dlp says before it
/// downloads ([`Meter::quality`]), and each line names the one it is for ([`Meter::named`]).
///
/// A line named otherwise (no `MDMQ`, an id it didn't name, `NA`) is read as before: back near 0
/// is the next stream, never counted as one of `MDMQ`'s, and the window's size holds the total.
/// Subtitle files come before the video with lines of their own, `NA` (#68): what they brought
/// leaves the bar when the first stream's line comes.
pub(super) struct Meter {
    pub(super) expected: Option<u64>,
    /// `MDMQ`'s format id and resolution (`137+140 1920x1080`), once yt-dlp has said.
    pub(super) quality: Option<String>,
    /// The stream the lines are for now (its format id), how many of `MDMQ`'s streams have
    /// started, and whether a line was for all of them at once (`137+140`).
    pub(super) current: Option<String>,
    pub(super) started: usize,
    pub(super) all_at_once: bool,
    /// Every line so far said `NA` for its format id (subtitle files).
    pub(super) only_na: bool,
    /// The streams already finished, and the current one's last total and bytes.
    pub(super) finished: u64,
    pub(super) last_total: u64,
    pub(super) last_got: u64,
    /// The highest fraction shown so far.
    pub(super) frac: f64,
    /// A line gave only yt-dlp's estimate of its stream's size (a fragmented stream's): the total
    /// isn't the real size (#2).
    pub(super) estimated: bool,
}

impl Meter {
    pub(super) fn new(expected: Option<u64>) -> Self {
        Meter {
            expected: expected.filter(|e| *e > 0),
            quality: None,
            current: None,
            started: 0,
            all_at_once: false,
            only_na: true,
            finished: 0,
            last_total: 0,
            last_got: 0,
            frac: 0.0,
            estimated: false,
        }
    }

    /// yt-dlp's `MDMQ <format id> <resolution>` ([`QUALITY`]): a merged choice's id names each
    /// stream (`137+140`).
    pub(super) fn quality(&mut self, rest: &str) {
        let rest = rest.trim();
        self.quality = (!rest.is_empty()).then(|| rest.to_owned());
    }

    /// How many streams `MDMQ` named.
    pub(super) fn streams(&self) -> Option<usize> {
        self.quality.as_ref().map(|q| q.split('+').count())
    }

    /// Whether a line's format `id` is one of the streams `MDMQ` named (`137` of `137+140`;
    /// false), or all of them at once (`137+140`, as ffmpeg downloads and merges them; true).
    /// None for any other: none named, `NA`, an id `MDMQ` didn't name, no `MDMQ`.
    pub(super) fn named<'a>(&self, id: Option<&'a str>) -> Option<(&'a str, bool)> {
        let (q, id) = (self.quality.as_deref()?, id.filter(|id| *id != "NA")?);
        // The id, then the resolution after the last one (one word, or two: `audio only`).
        let is = |s: &str| {
            s.strip_prefix(id)
                .is_some_and(|r| r.is_empty() || r.starts_with(' '))
        };
        if q.split('+').any(is) {
            Some((id, false))
        } else {
            is(q).then_some((id, true))
        }
    }

    /// The next stream started: the one before counts in full.
    pub(super) fn next_stream(&mut self) {
        self.finished += self.last_total.max(self.last_got);
        self.last_total = 0;
        self.last_got = 0;
        // Without the whole size from the start, the first stream's end looked like the
        // whole download: the next stream's total pulls the bar back (#344).
        if self.expected.is_none() {
            self.frac = 0.0;
        }
    }

    pub(super) fn line(&mut self, rest: &str, p: &mut Progress) {
        let fields: Vec<&str> = rest.trim().splitn(5, ' ').collect();
        let f: Vec<Option<f64>> = fields.iter().take(4).map(|v| v.parse().ok()).collect();
        let mut got = f.first().copied().flatten().unwrap_or(0.0).max(0.0) as u64;
        let total = f
            .get(1)
            .copied()
            .flatten()
            .or(f.get(2).copied().flatten())
            .map(|t| t.max(0.0) as u64);
        let id = fields
            .get(4)
            .map(|id| id.trim())
            .filter(|id| !id.is_empty());
        if total.is_some() && f.get(1).copied().flatten().is_none() {
            self.estimated = true;
        }
        let named = self.named(id);
        match named {
            // Each stream by its own id: a count from 0 again within one (a retry from the
            // start) is not the next stream, and the bar waits there until it is back where it
            // was.
            Some((id, _)) if self.current.as_deref() == Some(id) => {
                got = got.max(self.last_got);
            }
            Some((id, all)) => {
                if self.only_na {
                    // Subtitle files came first (#68): theirs are not the video's bytes (the bar
                    // gives back their share of the whole, a sliver beside a video's).
                    self.finished = 0;
                    self.last_total = 0;
                    self.last_got = 0;
                    self.frac = 0.0;
                } else if self.current.is_some() || self.last_got > 0 {
                    self.next_stream();
                }
                if all {
                    self.all_at_once = true;
                } else {
                    self.started += 1;
                }
                self.current = Some(id.to_owned());
                self.only_na = false;
            }
            // As before: back near 0 is the next stream.
            None => {
                let back = got < self.last_got;
                if back && self.last_got > 0 {
                    self.next_stream();
                }
                self.only_na &= id == Some("NA");
            }
        }
        self.last_got = got;
        if let Some(t) = total {
            self.last_total = t;
        }
        let downloaded = self.finished + got;
        p.downloaded = downloaded;
        p.speed = f.get(3).copied().flatten().unwrap_or(0.0);
        // Neither yt-dlp nor the video window knows how much is coming (a live stream, some
        // HLS): no percentage rather than what came so far as the whole, 100 % (#344).
        if self.last_total == 0 && self.expected.is_none() {
            p.total = None;
            return;
        }
        let mut whole = self.finished + self.last_total.max(got);
        // The window's size stands for the streams still to come (less what is known so far);
        // in the last stream, once its own total is known, nothing is left to come. When that
        // size was an estimate above what came, the bar lurches forward there (a merged
        // download's audio, after its video: from about the video's share of the estimate to
        // its share of what came); never back, and it glides there in the progress window.
        let last = named.is_some()
            && (self.all_at_once || self.streams().is_some_and(|n| self.started >= n));
        // The streams' own sizes are the whole once the last one's is known: exact unless one was
        // only estimated; before that the window's size stands (its own exactness), or with none
        // a part of the whole.
        p.exact = if last && self.last_total > 0 {
            Some(!self.estimated)
        } else if self.expected.is_some() {
            None
        } else {
            Some(false)
        };
        if let Some(e) = self.expected
            && !(last && self.last_total > 0)
        {
            whole = whole.max(e);
        }
        if whole == 0 {
            p.total = None;
            return;
        }
        let frac = (downloaded as f64 / whole as f64).min(MAX_WHILE_RUNNING);
        // When the estimate grew, the bar stays where it was rather than move back. Otherwise
        // the total is the whole, as big as it is: at the end it isn't inflated to read 99.9 %.
        p.total = Some(if frac >= self.frac {
            self.frac = frac;
            whole
        } else {
            ((downloaded as f64 / self.frac)
                .round()
                .max(whole.min(downloaded + 1) as f64)) as u64
        });
    }
}
