//! Download statistics (beyond IDM): what came in, day by day, which sites it came from and
//! what kinds of files it made, for Downloads → Statistics (`statsdlg.rs`).
//!
//! - [`Stats`]: the aggregates and every question asked of them. Pure: each call is given the
//!   day it is about, so the tests decide the dates.
//! - [`Recorder`]: the app's: it counts the bytes each pass of the app's reads added (on the day
//!   they came, so a big download counts where its bytes fell), each file finished, and the
//!   seconds bytes were coming; and keeps `stats.json` in the data folder, written atomically
//!   (craft-atomic) on a thread of its own, as the download list is.
//!
//! The file stays small: a row a day for two years ([`KEEP_DAYS`]), older days folded into
//! their month; the sites past the busiest [`KEEP_SITES`] folded into one row of their own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::model::{Category, Item};

/// Days kept a row of their own (two years); older ones are folded into their month.
pub const KEEP_DAYS: i64 = 731;
/// Sites kept by name, the busiest by bytes; the others are folded into [`OTHER_SITES`].
pub const KEEP_SITES: usize = 100;
/// How many sites there may be before the quietest are folded (not at every new site).
const SITES_SLACK: usize = 25;
/// The row the sites past [`KEEP_SITES`] are folded into (no site's name is empty of a dot).
pub const OTHER_SITES: &str = "*";
/// The row of downloads with no site of their own: torrents (their bytes come from peers).
pub const NO_SITE: &str = "";
/// The file's format.
const VERSION: u32 = 1;
/// A day's key and a month's (ISO 8601, so they sort as they follow each other).
const DAY: &str = "%Y-%m-%d";

/// What came in a day (or a month, a site, a kind of file).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Bytes that came.
    #[serde(default, rename = "b", skip_serializing_if = "is_zero")]
    pub bytes: u64,
    /// Downloads finished.
    #[serde(default, rename = "f", skip_serializing_if = "is_zero")]
    pub files: u64,
    /// Seconds in which bytes were coming (the average speed's time).
    #[serde(default, rename = "s", skip_serializing_if = "is_zero")]
    pub secs: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Tally {
    fn add(&mut self, o: Tally) {
        self.bytes = self.bytes.saturating_add(o.bytes);
        self.files = self.files.saturating_add(o.files);
        self.secs = self.secs.saturating_add(o.secs);
    }

    /// The average speed while bytes were coming (bytes a second), None without such a second.
    pub fn speed(&self) -> Option<f64> {
        (self.secs > 0).then(|| self.bytes as f64 / self.secs as f64)
    }

    pub fn is_empty(&self) -> bool {
        *self == Tally::default()
    }
}

/// Every statistic kept (`stats.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    #[serde(default)]
    version: u32,
    /// The first day counted: the first bytes recorded, or the day of the last reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    /// By day (`2026-11-18`), the last [`KEEP_DAYS`].
    #[serde(default)]
    days: BTreeMap<String, Tally>,
    /// By month (`2024-09`), the days older than that.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    months: BTreeMap<String, Tally>,
    /// By site (the address's host without `www.`; [`NO_SITE`], [`OTHER_SITES`]).
    #[serde(default)]
    sites: BTreeMap<String, Tally>,
    /// By kind of file (a category's key: `Video`, `Music`… or a category of the user's).
    #[serde(default)]
    kinds: BTreeMap<String, Tally>,
}

fn day_key(d: NaiveDate) -> String {
    d.format(DAY).to_string()
}

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, DAY).ok()
}

impl Stats {
    /// `bytes` came on `day` from `site` for a file of kind `kind`.
    pub fn add_bytes(&mut self, day: NaiveDate, site: &str, kind: &str, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let t = Tally {
            bytes,
            ..Tally::default()
        };
        self.add(day, site, kind, t);
    }

    /// A download from `site` of kind `kind` finished on `day`.
    pub fn add_file(&mut self, day: NaiveDate, site: &str, kind: &str) {
        let t = Tally {
            files: 1,
            ..Tally::default()
        };
        self.add(day, site, kind, t);
    }

    /// Bytes were coming for `secs` seconds on `day`.
    pub fn add_secs(&mut self, day: NaiveDate, secs: u64) {
        if secs == 0 {
            return;
        }
        self.started(day);
        self.days.entry(day_key(day)).or_default().add(Tally {
            secs,
            ..Tally::default()
        });
    }

    fn add(&mut self, day: NaiveDate, site: &str, kind: &str, t: Tally) {
        self.started(day);
        self.days.entry(day_key(day)).or_default().add(t);
        // Looked up before an owned key is made: most calls add to a row already there.
        match self.sites.get_mut(site) {
            Some(s) => s.add(t),
            None => {
                self.sites.insert(site.to_owned(), t);
            }
        }
        match self.kinds.get_mut(kind) {
            Some(k) => k.add(t),
            None => {
                self.kinds.insert(kind.to_owned(), t);
            }
        }
        if self.sites.len() > KEEP_SITES + SITES_SLACK {
            self.fold_sites();
        }
    }

    fn started(&mut self, day: NaiveDate) {
        self.version = VERSION;
        if self.since.is_none() {
            self.since = Some(day_key(day));
        }
    }

    /// The first day counted (None: nothing yet).
    pub fn since(&self) -> Option<NaiveDate> {
        self.since.as_deref().and_then(parse_day)
    }

    /// Everything forgotten; counted again from `today`.
    pub fn reset(&mut self, today: NaiveDate) {
        *self = Stats {
            version: VERSION,
            since: Some(day_key(today)),
            ..Stats::default()
        };
    }

    /// Days older than [`KEEP_DAYS`] before `today` folded into their months, and the sites past
    /// the busiest [`KEEP_SITES`] into [`OTHER_SITES`]: the file stays small however long DDM
    /// runs.
    pub fn compact(&mut self, today: NaiveDate) {
        let first_kept = day_key(today - chrono::Duration::days(KEEP_DAYS - 1));
        let old: Vec<String> = self
            .days
            .range(..first_kept)
            .map(|(k, _)| k.clone())
            .collect();
        for k in old {
            if let Some(t) = self.days.remove(&k) {
                // `2024-09-30` → `2024-09`.
                let month = k.get(..7).unwrap_or(&k).to_owned();
                self.months.entry(month).or_default().add(t);
            }
        }
        if self.sites.len() > KEEP_SITES {
            self.fold_sites();
        }
    }

    /// The busiest [`KEEP_SITES`] sites kept by name, the others added to [`OTHER_SITES`].
    fn fold_sites(&mut self) {
        let mut other = self.sites.remove(OTHER_SITES).unwrap_or_default();
        let mut named: Vec<(String, Tally)> = std::mem::take(&mut self.sites).into_iter().collect();
        named.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then_with(|| a.0.cmp(&b.0)));
        if named.len() > KEEP_SITES {
            for (_, t) in named.drain(KEEP_SITES..) {
                other.add(t);
            }
        }
        self.sites = named.into_iter().collect();
        if !other.is_empty() {
            self.sites.insert(OTHER_SITES.to_owned(), other);
        }
    }

    /// The days from `from` to `to`, both counted.
    pub fn between(&self, from: NaiveDate, to: NaiveDate) -> Tally {
        let mut t = Tally::default();
        if from > to {
            return t;
        }
        for (_, d) in self.days.range(day_key(from)..=day_key(to)) {
            t.add(*d);
        }
        t
    }

    /// Everything since the first day counted.
    pub fn all_time(&self) -> Tally {
        let mut t = Tally::default();
        for d in self.days.values().chain(self.months.values()) {
            t.add(*d);
        }
        t
    }

    /// The last `n` days up to `today`, oldest first, each with what came (nothing on a quiet
    /// day).
    pub fn last_days(&self, today: NaiveDate, n: usize) -> Vec<(NaiveDate, Tally)> {
        (0..n as i64)
            .rev()
            .map(|back| {
                let d = today - chrono::Duration::days(back);
                (d, self.days.get(&day_key(d)).copied().unwrap_or_default())
            })
            .collect()
    }

    /// The `n` sites that sent the most, busiest first ([`OTHER_SITES`] left out: it is no
    /// site).
    pub fn top_sites(&self, n: usize) -> Vec<(&str, Tally)> {
        let mut v: Vec<(&str, Tally)> = self
            .sites
            .iter()
            .filter(|(k, t)| k.as_str() != OTHER_SITES && t.bytes > 0)
            .map(|(k, t)| (k.as_str(), *t))
            .collect();
        v.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then_with(|| a.0.cmp(b.0)));
        v.truncate(n);
        v
    }

    /// Every kind of file, the most bytes first.
    pub fn kinds(&self) -> Vec<(&str, Tally)> {
        let mut v: Vec<(&str, Tally)> = self
            .kinds
            .iter()
            .filter(|(_, t)| t.bytes > 0 || t.files > 0)
            .map(|(k, t)| (k.as_str(), *t))
            .collect();
        v.sort_by(|a, b| {
            b.1.bytes
                .cmp(&a.1.bytes)
                .then_with(|| b.1.files.cmp(&a.1.files))
                .then_with(|| a.0.cmp(b.0))
        });
        v
    }

    /// Bytes counted by kind of file, all together (the kinds' shares are of this).
    pub fn kinds_bytes(&self) -> u64 {
        self.kinds.values().map(|t| t.bytes).sum()
    }

    /// Bytes counted by site, all together, the folded row included (the sites' shares are of
    /// this).
    pub fn sites_bytes(&self) -> u64 {
        self.sites.values().map(|t| t.bytes).sum()
    }

    /// Rows that don't read (a hand-edited file): left out, rather than the whole file refused.
    fn sanitize(&mut self) {
        self.days.retain(|k, _| parse_day(k).is_some());
        self.months
            .retain(|k, _| parse_day(&format!("{k}-01")).is_some() && k.len() == 7);
        if self
            .since
            .as_deref()
            .is_some_and(|s| parse_day(s).is_none())
        {
            self.since = None;
        }
        self.version = VERSION;
    }
}

/// The first day of the week `today` is in, the week starting on `first` (0 = Sunday … 6 =
/// Saturday, as Options → Regional has it).
pub fn week_start(today: NaiveDate, first: u8) -> NaiveDate {
    let into = (today.weekday().num_days_from_sunday() + 7 - u32::from(first.min(6))) % 7;
    today - chrono::Duration::days(i64::from(into))
}

/// The first day of `today`'s month.
pub fn month_start(today: NaiveDate) -> NaiveDate {
    today.with_day(1).unwrap_or(today)
}

/// A download's site: its address's host without `www.` ([`NO_SITE`] for a magnet link: a
/// torrent's bytes come from its peers, a debrid service's from the service).
pub fn site_of(item: &Item) -> String {
    if item.torrent.is_some() || item.remote.is_some() {
        return NO_SITE.to_owned();
    }
    item.url
        .host_str()
        .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
        .unwrap_or_default()
}

/// A download's kind of file: its category by its name (torrents: Torrents), by key.
pub fn kind_of(item: &Item) -> String {
    let c = if item.torrent.is_some() || item.remote.is_some() {
        Category::Torrents
    } else {
        Category::of(&item.file_name)
    };
    c.key().into_owned()
}

/// What one pass of the app's reads found of a download ([`take_reads`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub id: u64,
    /// Bytes that came since the last pass.
    pub bytes: u64,
    /// It runs (it may have just ended, with the last bytes it got).
    pub running: bool,
    /// Its site and kind ([`site_of`], [`kind_of`]) when bytes came, else empty.
    pub site: String,
    pub kind: String,
}

/// Every running download's bytes since the last pass, taken from the rows (`Item::arrived`),
/// and those of the downloads that ended in this one.
pub fn take_reads(items: &mut [Item]) -> Vec<Read> {
    items
        .iter_mut()
        // Not one waiting for Download File Info's answer (`Item::listed`): its bytes are
        // counted once it is in the list, and never after Cancel.
        .filter(|i| i.listed() && (i.live.is_some() || i.arrived > 0))
        .map(|i| {
            let bytes = std::mem::take(&mut i.arrived);
            let (site, kind) = if bytes > 0 {
                (site_of(i), kind_of(i))
            } else {
                (String::new(), String::new())
            };
            Read {
                id: i.id,
                bytes,
                running: i.live.is_some(),
                site,
                kind,
            }
        })
        .collect()
}

// ---- the app's recorder ------------------------------------------------------------------------

/// Reads further apart than this (the computer slept) don't count as time bytes were coming.
const MAX_GAP: Duration = Duration::from_secs(10);
/// How often the file is written while statistics change.
const SAVE_EVERY: Duration = Duration::from_secs(60);

/// The app's statistics: recorded from each pass's reads, kept in `stats.json`.
pub struct Recorder {
    stats: Stats,
    path: Option<PathBuf>,
    /// Its own writer thread, made at the first write.
    saver: Option<crate::store::Saver>,
    dirty: bool,
    saved: Instant,
    /// The last pass, and the part of a second bytes were coming not counted yet.
    last: Option<Instant>,
    secs: f64,
    /// The day of the last pass (a new one folds the old days: [`Stats::compact`]).
    day: Option<NaiveDate>,
}

impl Recorder {
    /// The statistics kept in the data folder (none there, or none that read: empty; a file
    /// that doesn't read is kept aside as `stats.json.bad`, not written over).
    pub fn load() -> Recorder {
        let path = mdm_ipc::data_dir().map(|d| d.join("stats.json"));
        let stats = path.as_deref().map(load_from).unwrap_or_default();
        Recorder {
            stats,
            path,
            saver: None,
            dirty: false,
            saved: Instant::now(),
            last: None,
            secs: 0.0,
            day: None,
        }
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// One pass of the app: the bytes its reads found (`reads`, [`take_reads`]) and the
    /// downloads that finished in it (`finished`, by id, in `items`).
    pub fn record(&mut self, items: &[Item], reads: &[Read], finished: &[u64]) {
        let now = Instant::now();
        let today = crate::clock::now().date_naive();
        if self.day != Some(today) {
            self.day = Some(today);
            self.stats.compact(today);
        }
        let coming = reads.iter().any(|r| r.bytes > 0);
        if coming {
            for r in reads.iter().filter(|r| r.bytes > 0) {
                self.stats.add_bytes(today, &r.site, &r.kind, r.bytes);
            }
            if let Some(last) = self.last {
                let dt = now.saturating_duration_since(last);
                if dt <= MAX_GAP {
                    self.secs += dt.as_secs_f64();
                }
            }
            let whole = self.secs.floor();
            if whole >= 1.0 {
                self.stats.add_secs(today, whole as u64);
                self.secs -= whole;
            }
            self.dirty = true;
        }
        for id in finished {
            if let Some(i) = items.iter().find(|i| i.id == *id) {
                self.stats.add_file(today, &site_of(i), &kind_of(i));
                self.dirty = true;
            }
        }
        self.last = Some(now);
        if self.dirty && self.saved.elapsed() >= SAVE_EVERY {
            self.save();
        }
    }

    /// Everything forgotten (Statistics → Reset), and written so at once.
    pub fn reset(&mut self) {
        self.stats.reset(crate::clock::now().date_naive());
        self.secs = 0.0;
        self.dirty = true;
        self.save();
    }

    /// Hands the statistics to their writer thread.
    fn save(&mut self) {
        self.dirty = false;
        self.saved = Instant::now();
        let Some(p) = self.path.clone() else { return };
        let Ok(json) = serde_json::to_vec(&self.stats) else {
            return;
        };
        self.saver
            .get_or_insert_with(|| crate::store::Saver::writing("stats saver", write))
            .queue(p, json);
    }

    /// Written before DDM closes: what changed since the last write, and what the writer has in
    /// hand.
    pub fn flush(&mut self) {
        if self.dirty {
            self.save();
        }
        if let Some(s) = &self.saver {
            s.flush();
            if let Some(Err(e)) = s.outcome() {
                tracing::warn!("statistics not saved: {e}");
            }
        }
    }
}

/// The statistics at `p`: none there (or unreadable) is empty; a file that doesn't parse is kept
/// aside as `stats.json.bad` (an earlier one replaced) and counting starts again.
fn load_from(p: &Path) -> Stats {
    let Ok(bytes) = std::fs::read(p) else {
        return Stats::default();
    };
    let json = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    match serde_json::from_slice::<Stats>(json) {
        Ok(mut s) => {
            s.sanitize();
            s
        }
        Err(e) => {
            tracing::warn!("statistics file unreadable ({e}): kept aside, counting again");
            let _ = std::fs::rename(p, p.with_extension("json.bad"));
            Stats::default()
        }
    }
}

/// `stats.json` written atomically (craft-atomic), its folder made first; the user's only, as
/// the list is (it names the sites downloaded from).
fn write(p: &Path, json: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        mdm_ipc::private_dir(dir)?;
    }
    mdm_ipc::private_write(p, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn bytes_count_on_the_day_they_came() {
        let mut s = Stats::default();
        // A big download over midnight: its bytes on both days, the file on the day it ended.
        s.add_bytes(d(2026, 11, 17), "example.com", "Video", 700);
        s.add_bytes(d(2026, 11, 18), "example.com", "Video", 300);
        s.add_file(d(2026, 11, 18), "example.com", "Video");
        assert_eq!(s.between(d(2026, 11, 17), d(2026, 11, 17)).bytes, 700);
        let today = s.between(d(2026, 11, 18), d(2026, 11, 18));
        assert_eq!((today.bytes, today.files), (300, 1));
        assert_eq!(s.all_time().bytes, 1000);
        assert_eq!(s.since(), Some(d(2026, 11, 17)));
    }

    #[test]
    fn periods_add_up_their_days() {
        let mut s = Stats::default();
        for (day, b) in [(1, 10), (15, 20), (16, 30), (18, 40)] {
            s.add_bytes(d(2026, 11, day), "a.org", "General", b);
        }
        s.add_bytes(d(2026, 10, 31), "a.org", "General", 5);
        let today = d(2026, 11, 18); // a Wednesday
        assert_eq!(s.between(month_start(today), today).bytes, 100);
        // The week from Monday the 16th; from Sunday the 15th.
        assert_eq!(s.between(week_start(today, 1), today).bytes, 70);
        assert_eq!(s.between(week_start(today, 0), today).bytes, 90);
        assert_eq!(s.all_time().bytes, 105);
        assert_eq!(
            s.between(today, d(2026, 11, 1)),
            Tally::default(),
            "an empty range"
        );
    }

    #[test]
    fn weeks_start_on_the_day_asked() {
        let wed = d(2026, 11, 18);
        assert_eq!(week_start(wed, 0), d(2026, 11, 15), "Sunday");
        assert_eq!(week_start(wed, 1), d(2026, 11, 16), "Monday");
        assert_eq!(week_start(wed, 3), wed, "Wednesday: today");
        assert_eq!(week_start(wed, 4), d(2026, 11, 12), "Thursday: last week's");
        assert_eq!(
            week_start(wed, 9),
            d(2026, 11, 14),
            "past Saturday: Saturday"
        );
        assert_eq!(month_start(wed), d(2026, 11, 1));
    }

    #[test]
    fn the_last_days_include_quiet_ones() {
        let mut s = Stats::default();
        s.add_bytes(d(2026, 11, 16), "a.org", "General", 7);
        let days = s.last_days(d(2026, 11, 18), 30);
        assert_eq!(days.len(), 30);
        assert_eq!(days[0].0, d(2026, 10, 20), "oldest first");
        assert_eq!(days[29].0, d(2026, 11, 18), "today last");
        assert_eq!(days[27].1.bytes, 7);
        assert_eq!(days.iter().filter(|(_, t)| t.bytes > 0).count(), 1);
    }

    #[test]
    fn the_average_speed_is_over_the_seconds_bytes_came() {
        let mut s = Stats::default();
        let day = d(2026, 11, 18);
        assert_eq!(s.between(day, day).speed(), None, "no time, no speed");
        s.add_bytes(day, "a.org", "General", 1000);
        s.add_secs(day, 4);
        assert_eq!(s.between(day, day).speed(), Some(250.0));
    }

    #[test]
    fn sites_and_kinds_rank_by_bytes() {
        let mut s = Stats::default();
        let day = d(2026, 11, 18);
        s.add_bytes(day, "small.org", "Documents", 10);
        s.add_bytes(day, "big.org", "Video", 500);
        s.add_bytes(day, NO_SITE, "Torrents", 200);
        s.add_file(day, "big.org", "Video");
        s.add_file(day, "big.org", "Video");
        let top = s.top_sites(2);
        assert_eq!(top.len(), 2, "cut to what was asked");
        assert_eq!(top[0].0, "big.org");
        assert_eq!((top[0].1.bytes, top[0].1.files), (500, 2));
        assert_eq!(top[1].0, NO_SITE, "torrents have their own row");
        let kinds: Vec<&str> = s.kinds().iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, ["Video", "Torrents", "Documents"]);
        assert_eq!(s.kinds_bytes(), 710);
        assert_eq!(s.sites_bytes(), 710);
    }

    #[test]
    fn old_days_fold_into_their_months() {
        let mut s = Stats::default();
        let today = d(2026, 11, 18);
        let long_ago = today - chrono::Duration::days(KEEP_DAYS + 40);
        let kept = today - chrono::Duration::days(KEEP_DAYS - 1);
        s.add_bytes(long_ago, "a.org", "General", 100);
        s.add_bytes(long_ago + chrono::Duration::days(1), "a.org", "General", 50);
        s.add_file(long_ago, "a.org", "General");
        s.add_bytes(kept, "a.org", "General", 7);
        s.add_bytes(today, "a.org", "General", 3);
        let before = s.all_time();
        s.compact(today);
        assert_eq!(s.all_time(), before, "nothing lost");
        assert_eq!(s.days.len(), 2, "the two last years' days stay days");
        assert!(
            s.days.contains_key(&day_key(kept)),
            "the oldest day kept stays"
        );
        let month = &day_key(long_ago)[..7];
        assert_eq!(s.months[month].bytes, 150);
        assert_eq!(s.months[month].files, 1);
        // Folding again changes nothing.
        let once = s.clone();
        s.compact(today);
        assert_eq!(s, once);
    }

    #[test]
    fn the_file_stays_bounded() {
        let mut s = Stats::default();
        let start = d(2020, 1, 1);
        // Six years of daily downloads from a new site each day.
        for n in 0..(6 * 366) {
            let day = start + chrono::Duration::days(n);
            s.add_bytes(day, &format!("site{n}.example"), "General", 1000 + n as u64);
            s.add_file(day, &format!("site{n}.example"), "General");
            s.add_secs(day, 10);
            s.compact(day);
        }
        assert!(s.days.len() <= KEEP_DAYS as usize);
        assert!(s.sites.len() <= KEEP_SITES + SITES_SLACK + 1);
        // The folded sites still add up to everything.
        assert_eq!(s.sites_bytes(), s.all_time().bytes);
        let files: u64 = s.sites.values().map(|t| t.files).sum();
        assert_eq!(files, s.all_time().files);
        assert!(
            s.top_sites(KEEP_SITES * 2)
                .iter()
                .all(|(k, _)| *k != OTHER_SITES),
            "the folded row is no site"
        );
        let json = serde_json::to_vec(&s).unwrap();
        assert!(json.len() < 64 * 1024, "{} bytes", json.len());
    }

    #[test]
    fn reset_forgets_everything_and_counts_from_today() {
        let mut s = Stats::default();
        s.add_bytes(d(2026, 1, 2), "a.org", "General", 5);
        s.reset(d(2026, 11, 18));
        assert_eq!(s.all_time(), Tally::default());
        assert!(s.top_sites(5).is_empty() && s.kinds().is_empty());
        assert_eq!(s.since(), Some(d(2026, 11, 18)));
    }

    #[test]
    fn the_file_round_trips_and_skips_rows_that_dont_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("stats.json");
        let mut s = Stats::default();
        s.add_bytes(d(2026, 11, 18), "a.org", "Video", 42);
        s.add_file(d(2026, 11, 18), "a.org", "Video");
        write(&p, &serde_json::to_vec(&s).unwrap()).unwrap();
        let back = load_from(&p);
        assert_eq!(back.all_time(), s.all_time());
        assert_eq!(back.top_sites(1)[0].0, "a.org");
        // Compact: zero counts aren't written.
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains(r#""2026-11-18":{"b":42,"f":1}"#), "{text}");
        // A hand-edited row that isn't a day is left out; the rest stays.
        std::fs::write(
            &p,
            r#"{"days":{"2026-11-18":{"b":1},"yesterday":{"b":9}},"months":{"2024-13":{"b":3}}}"#,
        )
        .unwrap();
        let s = load_from(&p);
        assert_eq!(s.all_time().bytes, 1);
        // One that doesn't parse is kept aside, not written over.
        std::fs::write(&p, b"{ not json").unwrap();
        assert_eq!(load_from(&p), Stats::default());
        assert!(p.with_extension("json.bad").exists());
        assert!(!p.exists());
    }

    #[test]
    fn reads_take_the_bytes_once() {
        let mut items = vec![Item::new(
            1,
            "https://www.Example.com/a/clip.mp4".parse().unwrap(),
            PathBuf::from("clip.mp4"),
            Category::Video,
        )];
        items[0].arrived = 1234;
        let reads = take_reads(&mut items);
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].bytes, 1234);
        assert_eq!(reads[0].site, "example.com");
        assert!(!reads[0].running);
        assert_eq!(items[0].arrived, 0, "taken");
        assert!(
            take_reads(&mut items).is_empty(),
            "nothing new, not running"
        );
    }
}
