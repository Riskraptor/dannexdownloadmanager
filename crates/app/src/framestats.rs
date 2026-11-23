//! Frame diagnostics for developers (beyond IDM): what the main window's frames cost and why
//! they come, so "smooth" is measured rather than guessed.
//!
//! - `MDM_FRAME_STATS=1`: every second the log (the data folder's `logs\app.log`) gets a line
//!   with the main window's passes in it, how many came from input (a key, the pointer), how
//!   long their logic and drawing took (the dialogs and progress windows are drawn inside the
//!   main window's pass, so they count too), and which lines of code asked for the others.
//! - `MDM_SYNTHETIC_LIST=10000`: that many made-up finished, stopped, waiting and failed rows
//!   are added to the list at start, for scrolling and sorting a big list. Only with
//!   `MDM_DATA_DIR` set (a scratch data folder): never in someone's own list.
//! - `MDM_BENCH_SCROLL=10`: the list scrolls by itself for that many seconds, a row a frame from
//!   the top (after two seconds for the window to settle), and the log gets the frames' times.
//!
//! Nothing here runs unless its variable is set; no text of it is shown to users.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn env_flag(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty() && v != "0")
}

/// `MDM_FRAME_STATS` is set.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| env_flag("MDM_FRAME_STATS").is_some())
}

/// A pass taken this soon after the one before, without input, was asked for by code: those
/// are a repaint storm's frames (an animation's are too, while it runs).
const FAST: Duration = Duration::from_millis(150);

#[derive(Default)]
struct Second {
    passes: u32,
    input: u32,
    fast: u32,
    ui_ms: Vec<f32>,
    logic_ms: f64,
    /// Who asked for the passes that came without input: `file:line` and how many times; fast
    /// ones (within [`FAST`]) apart.
    asked: HashMap<String, u32>,
    asked_fast: HashMap<String, u32>,
    /// Time in named parts of the passes ([`span`]), ms.
    parts: Vec<(&'static str, f64)>,
}

/// A named part of the main window's pass, timed until it is dropped ([`span`]).
pub struct Span(Option<(&'static str, Instant)>);

/// Time a part of the pass ("main", "progress"…): the second's line says how long it took a
/// pass, on average.
pub fn span(name: &'static str) -> Span {
    Span(on().then(|| (name, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, t)) = self.0 {
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let mut s = lock();
            match s.now.parts.iter_mut().find(|(n, _)| *n == name) {
                Some((_, total)) => *total += ms,
                None => s.now.parts.push((name, ms)),
            }
        }
    }
}

struct Stats {
    since: Instant,
    last_pass: Option<Instant>,
    now: Second,
    /// The scroll benchmark's frames ([`bench_scroll`]): when it began and each frame's time.
    bench: Option<(Instant, Vec<f32>)>,
    bench_done: bool,
}

fn stats() -> &'static Mutex<Stats> {
    static S: OnceLock<Mutex<Stats>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Stats {
            since: Instant::now(),
            last_pass: None,
            now: Second::default(),
            bench: None,
            bench_done: false,
        })
    })
}

fn lock() -> std::sync::MutexGuard<'static, Stats> {
    stats()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where a repaint was asked for, as the log names it: the file's name and the line.
fn cause_name(c: &egui::RepaintCause) -> String {
    let file = c.file.rsplit(['/', '\\']).next().unwrap_or(c.file);
    if c.reason.is_empty() {
        format!("{file}:{}", c.line)
    } else {
        format!("{file}:{} ({})", c.line, c.reason)
    }
}

/// The time a part of the frame took, from `begin`'s instant (None when stats are off).
pub fn begin() -> Option<Instant> {
    on().then(Instant::now)
}

/// The main window's logic (`App::tick`) took the time since `start`.
pub fn logic_done(start: Option<Instant>) {
    if let Some(s) = start {
        lock().now.logic_ms += s.elapsed().as_secs_f64() * 1000.0;
    }
}

/// The main window's UI pass that began at `start` is done: counted, with why it came, and the
/// second's line written once a second has gone by.
pub fn pass_done(ctx: &egui::Context, start: Option<Instant>) {
    let Some(start) = start else {
        return;
    };
    let now = Instant::now();
    let ui_ms = now.duration_since(start).as_secs_f32() * 1000.0;
    let input = ctx.input(|i| {
        i.events.iter().any(|e| {
            !matches!(
                e,
                egui::Event::WindowFocused(_) | egui::Event::AccessKitActionRequest(_)
            )
        }) || i.pointer.delta() != egui::Vec2::ZERO
    });
    let causes: Vec<String> = if input {
        Vec::new()
    } else {
        let mut v: Vec<String> = ctx.repaint_causes().iter().map(cause_name).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut s = lock();
    let fast = !input && s.last_pass.is_some_and(|l| start.duration_since(l) < FAST);
    s.last_pass = Some(now);
    let sec = &mut s.now;
    sec.passes += 1;
    sec.ui_ms.push(ui_ms);
    if input {
        sec.input += 1;
    }
    if fast {
        sec.fast += 1;
    }
    for c in causes {
        if fast {
            *sec.asked_fast.entry(c.clone()).or_default() += 1;
        }
        *sec.asked.entry(c).or_default() += 1;
    }
    if let Some((_, frames)) = s.bench.as_mut() {
        frames.push(ui_ms);
    }
    if now.duration_since(s.since) >= Duration::from_secs(1) {
        let span = now.duration_since(s.since).as_secs_f64();
        let sec = std::mem::take(&mut s.now);
        s.since = now;
        drop(s);
        tracing::info!("frame stats: {}", second_line(&sec, span));
    }
}

/// Percentile `p` (0 to 1) of `v`, sorted in place.
fn percentile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    let at = ((v.len() - 1) as f32 * p).round() as usize;
    v[at.min(v.len() - 1)]
}

fn top(map: &HashMap<String, u32>, n: usize) -> String {
    let mut v: Vec<(&String, &u32)> = map.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    v.iter()
        .take(n)
        .map(|(k, c)| format!("{k} x{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn second_line(sec: &Second, span: f64) -> String {
    let mut ui = sec.ui_ms.clone();
    let avg = if ui.is_empty() {
        0.0
    } else {
        ui.iter().sum::<f32>() / ui.len() as f32
    };
    let p95 = percentile(&mut ui, 0.95);
    let max = ui.last().copied().unwrap_or(0.0);
    let logic = if sec.passes == 0 {
        0.0
    } else {
        sec.logic_ms / f64::from(sec.passes)
    };
    let parts = sec
        .parts
        .iter()
        .map(|(n, ms)| format!("{n} {:.2}", ms / f64::from(sec.passes.max(1))))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{:.1} passes/s ({} input, {} fast asked), ui avg {avg:.2} ms p95 {p95:.2} max {max:.2} \
         [{parts}], logic avg {logic:.2} ms; fast asked by: [{}]; all asked by: [{}]",
        f64::from(sec.passes) / span,
        sec.input,
        sec.fast,
        top(&sec.asked_fast, 4),
        top(&sec.asked, 4),
    )
}

/// How many rows the list scrolls this frame in the scroll benchmark (`MDM_BENCH_SCROLL`
/// seconds), None when it isn't running. It starts two seconds after the first frame, keeps the
/// frames coming while it runs and logs their times at its end.
pub fn bench_scroll(ctx: &egui::Context) -> Option<f32> {
    static SECONDS: OnceLock<Option<f64>> = OnceLock::new();
    static FIRST: OnceLock<Instant> = OnceLock::new();
    let seconds = (*SECONDS
        .get_or_init(|| env_flag("MDM_BENCH_SCROLL").and_then(|v| v.trim().parse::<f64>().ok())))?;
    let first = *FIRST.get_or_init(Instant::now);
    let mut s = lock();
    if s.bench_done {
        return None;
    }
    ctx.request_repaint();
    match s.bench.as_mut() {
        None if first.elapsed() >= Duration::from_secs(2) => {
            s.bench = Some((Instant::now(), Vec::new()));
            Some(0.0)
        }
        None => None,
        Some((began, frames)) if began.elapsed().as_secs_f64() >= seconds => {
            let span = began.elapsed().as_secs_f64();
            let mut f = std::mem::take(frames);
            s.bench = None;
            s.bench_done = true;
            drop(s);
            let n = f.len();
            let avg = f.iter().sum::<f32>() / n.max(1) as f32;
            let p50 = percentile(&mut f, 0.5);
            let p95 = percentile(&mut f, 0.95);
            let p99 = percentile(&mut f, 0.99);
            let max = f.last().copied().unwrap_or(0.0);
            tracing::info!(
                "scroll bench: {n} frames in {span:.1} s ({:.1}/s), ui avg {avg:.2} ms p50 \
                 {p50:.2} p95 {p95:.2} p99 {p99:.2} max {max:.2}",
                n as f64 / span
            );
            None
        }
        Some(_) => Some(1.0),
    }
}

/// `MDM_SYNTHETIC_LIST`'s rows to add (made-up downloads), when it is set together with
/// `MDM_DATA_DIR`: as many as make the list hold that many of them (a list that already has them
/// from the last start gets none again).
pub fn synthetic_wanted(items: &[crate::model::Item]) -> usize {
    let Some(n) = env_flag("MDM_SYNTHETIC_LIST").and_then(|v| v.trim().parse::<usize>().ok())
    else {
        return 0;
    };
    if mdm_ipc::env_var_os("DATA_DIR").is_none() {
        tracing::warn!("MDM_SYNTHETIC_LIST needs MDM_DATA_DIR (a scratch data folder): ignored");
        return 0;
    }
    let there = items.iter().filter(|i| is_synthetic(&i.url)).count();
    n.saturating_sub(there)
}

/// The made-up rows' host: an address nothing answers (`.invalid` is reserved for that).
const HOST: &str = "synthetic.invalid";

fn is_synthetic(url: &url::Url) -> bool {
    url.host_str() == Some(HOST)
}

/// `n` made-up downloads numbered from `first_id`, into `dir`: three in four finished, the rest
/// stopped part-way, waiting or failed, of every kind of file, with sizes and dates.
pub fn synthetic_items(first_id: u64, n: usize, dir: &std::path::Path) -> Vec<crate::model::Item> {
    use crate::model::{Category, Item, Status};
    const KINDS: [&str; 8] = ["zip", "mp4", "mp3", "pdf", "exe", "iso", "jpg", "docx"];
    const WORDS: [&str; 10] = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet",
    ];
    // A small fixed generator: the same list every start.
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let stamp = crate::model::now_stamp();
    (0..n)
        .map(|k| {
            let id = first_id + k as u64;
            let r = next();
            let ext = KINDS[(r % KINDS.len() as u64) as usize];
            let word = WORDS[((r >> 8) % WORDS.len() as u64) as usize];
            let name = format!("{word}-{id:05}.{ext}");
            let url: url::Url = format!("http://{HOST}/files/{name}")
                .parse()
                .expect("a valid address");
            let mut it = Item::new(id, url, dir.join(&name), Category::of(&name));
            let size = 1024 + (r >> 16) % (4 << 30);
            it.size = Some(size);
            it.last_try = Some(stamp.clone());
            match (r >> 40) % 20 {
                0..=14 => {
                    it.status = Status::Complete;
                    it.downloaded = size;
                }
                15..=17 => {
                    it.status = Status::Stopped;
                    it.downloaded = size / 3;
                    it.resumable = Some(true);
                }
                18 => it.status = Status::Queued,
                _ => it.status = Status::Error,
            }
            it
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The made-up list is the same every time, has every status and only its own host, and
    /// is recognised as made up (so a second start doesn't add it again).
    #[test]
    fn synthetic_rows_are_stable_and_recognised() {
        let dir = std::path::Path::new("scratch");
        let a = synthetic_items(10, 400, dir);
        let b = synthetic_items(10, 400, dir);
        assert_eq!(a.len(), 400);
        assert_eq!(a[0].id, 10);
        assert_eq!(a[399].id, 409);
        assert!(
            a.iter()
                .zip(&b)
                .all(|(x, y)| x.file_name == y.file_name && x.size == y.size)
        );
        assert!(a.iter().all(|i| is_synthetic(&i.url)));
        use crate::model::Status;
        for s in [
            Status::Complete,
            Status::Stopped,
            Status::Queued,
            Status::Error,
        ] {
            assert!(a.iter().any(|i| i.status == s), "{s:?}");
        }
        let finished = a.iter().filter(|i| i.status == Status::Complete).count();
        assert!(finished > 400 / 2, "mostly finished: {finished}");
    }

    #[test]
    fn percentiles_and_tops() {
        let mut v = vec![5.0, 1.0, 3.0, 2.0, 4.0];
        assert_eq!(percentile(&mut v, 0.5), 3.0);
        assert_eq!(percentile(&mut v, 1.0), 5.0);
        assert_eq!(percentile(&mut [], 0.5), 0.0);
        let mut m = HashMap::new();
        m.insert("a.rs:1".to_owned(), 2);
        m.insert("b.rs:2".to_owned(), 5);
        m.insert("c.rs:3".to_owned(), 1);
        assert_eq!(top(&m, 2), "b.rs:2 x5, a.rs:1 x2");
    }
}
