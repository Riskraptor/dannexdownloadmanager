//! Live speed graphs (beyond IDM): a download's speed over the last two minutes in its progress
//! window, and every download's together over the last minute in the main window's status bar.
//!
//! Sampled from what the app already reads of each download at each pass (the bytes its
//! engine's figures added, `Item::arrived`, model.rs): no reading of its own. What came between
//! two passes is spread evenly over the time between them, in whole seconds; each second closed
//! is smoothed (an exponential average) and kept. The scale is a round speed above what is in
//! view: it grows at once and shrinks only once what is in view has stayed well under it for a
//! few seconds, so it doesn't jump with every second; the drawing eases to a new one.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use egui::{Color32, Painter, Pos2, Rect, Ui, pos2, vec2};

use crate::stats::Read;
use crate::winui::{self, r};

/// Seconds a history keeps: the progress window shows them all.
pub const KEEP: usize = 120;
/// The status bar's sparkline: the last minute.
pub const SPARK: usize = 60;
/// Passes further apart than this (seconds: the computer slept) aren't spread over the gap.
const MAX_GAP: f64 = 10.0;
/// Each closed second's weight in the smoothed speed.
const SMOOTH: f64 = 0.45;
/// Room above the highest speed in view (×), before the scale is rounded up.
const HEADROOM: f64 = 1.15;
/// The lowest scale (bytes a second): a trickle doesn't fill the graph.
const MIN_SCALE: f64 = 16.0 * 1024.0;
/// Seconds what is in view must stay under a lower scale before the graph takes it.
const SHRINK_AFTER: u32 = 5;
/// A download's history not running for this long (seconds) is let go.
const FORGET: f64 = 600.0;
/// How long the drawing takes to ease to a new scale (seconds).
const EASE: f32 = 0.35;

/// One download's speed (or every download's together), second by second.
#[derive(Clone, Debug, Default)]
pub struct History {
    /// The smoothed speed of each closed second (bytes a second), oldest first.
    seconds: VecDeque<f32>,
    /// The second under way (whole seconds on the app's clock) and what came in it so far.
    second: i64,
    pending: f64,
    /// The smoothed speed so far; None before the first second closed.
    ema: Option<f64>,
    /// The last pass (seconds); None before the first.
    last: Option<f64>,
    /// When it last ran (seconds).
    ran: f64,
    /// While running: bytes and seconds, for the average; the fastest smoothed second.
    bytes: f64,
    secs: f64,
    peak: f64,
    /// The scale (bytes a second at the graph's top), and how many seconds what is in view has
    /// stayed under a lower one.
    scale: f64,
    low_for: u32,
}

impl History {
    /// A pass at `now` (seconds): `bytes` came since the last one; `running`: its download (one
    /// at least, for the total) runs.
    pub fn add(&mut self, now: f64, bytes: u64, running: bool) {
        let sec = now.floor() as i64;
        let from = match self.last {
            Some(l) if now >= l && now - l <= MAX_GAP => l,
            _ => now,
        };
        if self.last.is_none() {
            self.second = sec;
        }
        // Further than the kept seconds since the last pass (a long sleep): only the seconds
        // that can still be seen are closed, empty.
        if sec - self.second > KEEP as i64 + 1 {
            self.pending = 0.0;
            self.second = sec - KEEP as i64 - 1;
        }
        let span = now - from;
        let b = bytes as f64;
        let mut t = from;
        while self.second < sec {
            let edge = (self.second + 1) as f64;
            if span > 0.0 && edge > t {
                self.pending += b * (edge - t) / span;
                t = edge;
            }
            self.close();
        }
        self.pending += if span > 0.0 {
            b * (now - t).max(0.0) / span
        } else {
            b
        };
        if running {
            // The average: bytes over the time they took (the first pass's came in a time not
            // known, since the download started).
            if span > 0.0 {
                self.bytes += b;
                self.secs += span;
            }
            self.ran = now;
        }
        self.last = Some(now);
    }

    /// The second under way is over: its speed, smoothed, goes into the history.
    fn close(&mut self) {
        let v = self.pending;
        let ema = match self.ema {
            Some(e) => e + SMOOTH * (v - e),
            None => v,
        };
        self.ema = Some(ema);
        if self.seconds.len() >= KEEP {
            self.seconds.pop_front();
        }
        self.seconds.push_back(ema as f32);
        self.peak = self.peak.max(ema);
        self.pending = 0.0;
        self.second += 1;
        self.rescale();
    }

    fn rescale(&mut self) {
        let top = self.seconds.iter().fold(0.0_f32, |m, v| m.max(*v)) as f64;
        let want = nice_ceil(top * HEADROOM).max(MIN_SCALE);
        if want >= self.scale {
            self.scale = want;
            self.low_for = 0;
        } else {
            self.low_for += 1;
            if self.low_for >= SHRINK_AFTER {
                self.scale = want;
                self.low_for = 0;
            }
        }
    }

    /// The smoothed speed of each second closed, oldest first (the last [`KEEP`]).
    pub fn seconds(&self) -> &VecDeque<f32> {
        &self.seconds
    }

    /// The speed at the graph's top (bytes a second): a round one.
    pub fn scale(&self) -> f64 {
        self.scale.max(MIN_SCALE)
    }

    /// The average speed while it ran (bytes a second), once it has run a second.
    pub fn average(&self) -> Option<f64> {
        (self.secs >= 1.0).then(|| self.bytes / self.secs)
    }

    /// The fastest second (smoothed), once a second has closed.
    pub fn peak(&self) -> Option<f64> {
        (!self.seconds.is_empty()).then_some(self.peak)
    }

    /// The last second's speed (smoothed): where the graph's line ends, never over the peak.
    pub fn current(&self) -> Option<f64> {
        self.seconds.back().map(|v| f64::from(*v))
    }
}

/// The smallest round speed or size ([`STEPS`] of a power of ten, in KB, MB or GB: 1024 bytes
/// to the KB) at least `v`.
pub fn nice_ceil(v: f64) -> f64 {
    const K: f64 = 1024.0;
    if v.is_nan() || v <= K {
        return K;
    }
    let unit = if v > 500.0 * K * K {
        K * K * K
    } else if v > 500.0 * K {
        K * K
    } else {
        K
    };
    let x = (v / unit).max(1.0);
    let mut decade = 1.0;
    while decade * 10.0 < x {
        decade *= 10.0;
    }
    for m in STEPS {
        if m * decade >= x {
            return m * decade * unit;
        }
    }
    10.0 * decade * unit
}

/// The round values a scale takes in each power of ten: close enough together that what is in
/// view fills most of the height (a steady 2 MB/sec under 2.5, not 5).
const STEPS: [f64; 9] = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0];

/// `v` in units of `template` (`{n} MB/sec`…), whole when it is (a round scale), else with one
/// decimal: in the interface language, IDM's way in English.
pub fn unit_text(v: f64, template: &str) -> String {
    let decimals = if v >= 100.0 || (v - v.round()).abs() < 0.05 {
        0
    } else {
        1
    };
    if crate::i18n::code() == "en" {
        let v = format!("{v:.decimals$}");
        template.replace("{n}", &v) // not translated: English path
    } else {
        crate::i18n::i18n().measure(crate::i18n::lang(), v, decimals, template)
    }
}

/// A speed with two decimals, where room is short (Statistics' table): `7.81 MB/sec`.
pub fn rate2(bps: f64) -> String {
    const K: f64 = 1024.0;
    let (v, template) = if bps >= K * K {
        (bps / (K * K), "{n} MB/sec")
    } else {
        (bps / K, "{n} KB/sec")
    };
    if crate::i18n::code() == "en" {
        template.replace("{n}", &format!("{v:.2}")) // not translated: English path
    } else {
        crate::i18n::i18n().measure(crate::i18n::lang(), v, 2, template)
    }
}

/// A speed for a graph's scale: `2 MB/sec`, `500 KB/sec`, `1.5 MB/sec`.
pub fn rate_short(bps: f64) -> String {
    const K: f64 = 1024.0;
    if bps >= K * K {
        unit_text(bps / (K * K), "{n} MB/sec")
    } else {
        unit_text(bps / K, "{n} KB/sec")
    }
}

/// A size for a chart's scale: `2 GB`, `500 MB`, `1.5 GB`.
pub fn size_short(b: f64) -> String {
    const K: f64 = 1024.0;
    if b >= K * K * K {
        unit_text(b / (K * K * K), "{n} GB")
    } else if b >= K * K {
        unit_text(b / (K * K), "{n} MB")
    } else {
        unit_text(b / K, "{n} KB")
    }
}

/// Every download's history, and all of them together: what the app's passes read.
pub struct Speeds {
    start: Instant,
    total: History,
    each: HashMap<u64, History>,
}

impl Default for Speeds {
    fn default() -> Self {
        Speeds {
            start: Instant::now(),
            total: History::default(),
            each: HashMap::new(),
        }
    }
}

impl Speeds {
    /// One pass of the app: each download read (`reads`, `stats::take_reads`).
    pub fn tick(&mut self, reads: &[Read]) {
        let now = self.start.elapsed().as_secs_f64();
        self.tick_at(now, reads);
    }

    fn tick_at(&mut self, now: f64, reads: &[Read]) {
        let mut sum = 0;
        let mut running = false;
        for r in reads {
            sum += r.bytes;
            running |= r.running;
            self.each
                .entry(r.id)
                .or_default()
                .add(now, r.bytes, r.running);
        }
        // The others go on in time (a paused download's graph shows the pause), and those that
        // stopped long ago are let go.
        for (id, h) in &mut self.each {
            if !reads.iter().any(|r| r.id == *id) {
                h.add(now, 0, false);
            }
        }
        self.each.retain(|_, h| now - h.ran <= FORGET);
        self.total.add(now, sum, running);
    }

    /// A download's history (none before it has run in this session).
    pub fn of(&self, id: u64) -> Option<&History> {
        self.each.get(&id)
    }

    /// Every download's together.
    pub fn total(&self) -> &History {
        &self.total
    }
}

// ---- drawing -----------------------------------------------------------------------------------

/// `values` (bytes a second, oldest first) as a filled area under a line in `rc` (on screen), the
/// last `span` seconds across its width, the newest at its end: the right, or the left when
/// `mirrored` (a right-to-left interface, as its progress bars fill from the right); `scale` at
/// the top.
fn plot(
    p: &Painter,
    rc: Rect,
    values: &[f32],
    span: usize,
    scale: f32,
    mirrored: bool,
    width: f32,
) {
    let n = values.len().min(span);
    if n < 2 || span < 2 || scale <= 0.0 || rc.width() <= 0.0 {
        return;
    }
    let c = crate::theme::colors();
    let vals = &values[values.len() - n..];
    let step = rc.width() / (span - 1) as f32;
    let points: Vec<Pos2> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let back = (n - 1 - i) as f32 * step;
            let x = if mirrored {
                rc.left() + back
            } else {
                rc.right() - back
            };
            let y = rc.bottom() - (v / scale).clamp(0.0, 1.0) * rc.height();
            pos2(x, y)
        })
        .collect();
    // The area: a strip down to the bottom, fading toward it.
    let top = c.graph_fill;
    let bottom = top.gamma_multiply(0.2);
    let mut mesh = egui::Mesh::default();
    for w in points.windows(2) {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(w[0], top);
        mesh.colored_vertex(w[1], top);
        mesh.colored_vertex(pos2(w[0].x, rc.bottom()), bottom);
        mesh.colored_vertex(pos2(w[1].x, rc.bottom()), bottom);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 3, i + 2);
    }
    p.add(egui::Shape::mesh(mesh));
    p.add(egui::Shape::line(
        points,
        egui::Stroke::new(width, c.graph_line),
    ));
}

/// A one-pixel line across `rc` (on screen) at `y`, or down it at `x`.
fn rule(p: &Painter, from: Pos2, to: Pos2, c: Color32) {
    let rc = Rect::from_min_max(
        pos2(from.x.round(), from.y.round()),
        pos2(
            to.x.round().max(from.x.round() + 1.0),
            to.y.round().max(from.y.round() + 1.0),
        ),
    );
    winui::solid(p, rc, c);
}

/// A chart's box: its background, its frame, and the grid lines halfway up and at each quarter
/// across. Its inside, on screen.
pub fn chart_box(p: &Painter, rc: Rect, columns: usize) -> Rect {
    let c = crate::theme::colors();
    let m = crate::theme::metrics();
    let radius = winui::control_radius().min(4.0);
    p.rect_filled(rc, radius, c.graph);
    let inner = rc.shrink(m.border);
    let mid = inner.center().y;
    rule(
        p,
        pos2(inner.left() + 1.0, mid),
        pos2(inner.right() - 1.0, mid),
        c.graph_grid,
    );
    for k in 1..columns {
        let x = inner.left() + inner.width() * k as f32 / columns as f32;
        rule(
            p,
            pos2(x, inner.top() + 1.0),
            pos2(x, inner.bottom() - 1.0),
            c.graph_grid,
        );
    }
    inner
}

/// The chart box's frame, drawn over what is plotted in it.
pub fn chart_frame(p: &Painter, rc: Rect) {
    let radius = winui::control_radius().min(4.0);
    p.rect_stroke(
        rc,
        radius,
        egui::Stroke::new(crate::theme::metrics().border, winui::FIELD_EDGE()),
        egui::StrokeKind::Inside,
    );
}

/// A label at a chart's top corner, at its start (English pen `x`, `baseline`), on a backdrop
/// of the chart's colour so the line under it doesn't cross the text.
pub fn chart_label(p: &Painter, o: Pos2, x: f32, baseline: f32, s: &str) {
    let c = crate::theme::colors();
    let w = winui::text_width(p, s).ceil();
    let g = c.graph;
    let backdrop = Color32::from_rgba_unmultiplied(g.r(), g.g(), g.b(), 0xd8);
    p.rect_filled(r(o, x - 2.0, baseline - 10.0, w + 4.0, 13.0), 2.0, backdrop);
    winui::text(p, o.x + x, o.y + baseline, s, winui::MUTED());
}

/// How tall the band a progress window gives its speed graph is (English pixels): the graph and
/// its figures, between the tab page and the progress bar, which move down by this much.
pub const BAND: f32 = 70.0;

/// The progress window's speed graph in its band (beyond IDM): `o` is the window's layout origin
/// (English, its Rename row's drop included); the graph sits at y 180, the width of the progress
/// bar, its figures under it: the speed now (where the line ends) while it `runs`, its average
/// and its peak; `salt` tells the windows apart.
pub fn progress_band(
    ui: &Ui,
    p: &Painter,
    o: Pos2,
    history: Option<&History>,
    runs: bool,
    salt: u64,
) {
    let current = history.filter(|_| runs).and_then(History::current);
    let rc = r(o, 17.0, 180.0, 513.0, 46.0);
    let inner = chart_box(p, rc, 4);
    if let Some(h) = history.filter(|h| h.seconds().len() >= 2) {
        let values: Vec<f32> = h.seconds().iter().copied().collect();
        let scale = ui.ctx().animate_value_with_time(
            egui::Id::unique(("speed-graph-scale", salt)),
            h.scale() as f32,
            EASE,
        );
        let clip = p.with_clip_rect(inner.intersect(p.clip_rect()));
        plot(
            &clip,
            inner.shrink2(vec2(0.0, 1.0)),
            &values,
            KEEP,
            scale,
            winui::rtl(),
            1.5,
        );
        chart_label(p, o, 22.0, 192.0, &rate_short(h.scale()));
    } else {
        // i18n: progress window: inside the empty speed graph, before the download has sent anything in this session
        let s = crate::i18n::t("No speed data yet");
        let w = winui::text_width(p, s);
        winui::text_at(
            p,
            winui::tahoma(),
            (inner.center().x - w / 2.0).round(),
            inner.center().y + 4.0,
            s,
            winui::MUTED(),
        );
    }
    chart_frame(p, rc);
    // i18n: tooltip of the speed graph in a download's progress window
    let tip = crate::i18n::t("This download's speed over the last two minutes");
    ui.interact(
        rc,
        egui::Id::unique(("speed-graph", salt)),
        egui::Sense::hover(),
    )
    .on_hover_text(tip);
    // Its figures, in three columns under it: a label, its value right after it (a long label
    // gives way first, cut with "...", whole on hover).
    let figures = [
        // i18n: progress window, under the speed graph: label before the download's speed now
        (crate::i18n::mark("Current"), current),
        (
            // i18n: progress window, under the speed graph: label before its average speed while it ran
            crate::i18n::mark("Average"),
            history.and_then(History::average),
        ),
        // i18n: progress window, under the speed graph: label before its fastest speed (noun: the highest speed reached)
        (crate::i18n::mark("Peak"), history.and_then(History::peak)),
    ];
    let base = o.y + 240.0;
    for (k, (label, value)) in figures.into_iter().enumerate() {
        let x = 17.0 + k as f32 * 171.0;
        let end = x + 165.0;
        let value = value.map(crate::model::rate).unwrap_or_default();
        let vw = winui::text_width(p, &value).ceil();
        let label = crate::i18n::t(label);
        let room = (end - x - vw - 5.0).max(0.0);
        let shown = winui::ellipsize(p, &winui::tahoma(), label, room);
        let drawn = winui::text(p, o.x + x, base, &shown, winui::MUTED());
        if shown != label {
            winui::full_on_hover(ui, drawn, ("speed-figure", salt, k), label);
        }
        let lw = winui::text_width(p, &shown).ceil();
        winui::text(p, o.x + x + lw + 5.0, base, &value, winui::TEXT());
    }
}

/// The status bar's sparkline (beyond IDM): every download's speed together over the last
/// minute, ending at English x `end` (before the status text), its baseline at the text's;
/// not drawn where it would start before English x `start_min` (a narrow window), nor before
/// two seconds have closed.
pub fn status_sparkline(
    ui: &Ui,
    p: &Painter,
    end: f32,
    baseline: f32,
    start_min: f32,
    h: &History,
) {
    const W: f32 = 60.0;
    let start = (end - W).round();
    let n = h.seconds().len();
    if start < start_min || n < 2 {
        return;
    }
    let values: Vec<f32> = h
        .seconds()
        .iter()
        .skip(n.saturating_sub(SPARK))
        .copied()
        .collect();
    let english = Rect::from_min_max(
        pos2(start, baseline - 10.0),
        pos2(end.round(), baseline + 2.0),
    );
    let rc = winui::rect(english);
    let peak = values.iter().fold(0.0_f32, |m, v| m.max(*v));
    // A round scale, as the graphs' (nice_ceil): the minute's peak moves every second, and a
    // scale following it eased anew each second, a third of a second of full-speed frames each
    // time (14 frames a second more while anything downloaded, MDM_FRAME_STATS); a round one
    // changes only when the peak crosses a step.
    let target = nice_ceil(f64::from(peak) * HEADROOM).max(MIN_SCALE) as f32;
    let scale =
        ui.ctx()
            .animate_value_with_time(egui::Id::unique("status-sparkline-scale"), target, EASE);
    let c = crate::theme::colors();
    rule(
        p,
        pos2(rc.left(), rc.bottom()),
        pos2(rc.right(), rc.bottom()),
        c.graph_grid,
    );
    let clip = p.with_clip_rect(rc.expand(1.0).intersect(p.clip_rect()));
    plot(&clip, rc, &values, SPARK, scale, winui::rtl(), 1.0);
    let tip = crate::i18n::fmt(
        // i18n: tooltip of the small graph in the main window's status bar: every download's speed together over the last minute; {rate} = its highest, like "1.250 MB/sec"
        crate::i18n::t("All downloads together, the last minute: up to {rate}"),
        &[("rate", &crate::model::rate(f64::from(peak)))],
    );
    ui.interact(
        rc,
        egui::Id::unique("status-sparkline"),
        egui::Sense::hover(),
    )
    .on_hover_text(tip);
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: f64 = 1024.0 * 1024.0;

    /// Passes every quarter second at `bps` bytes a second, from `from` for `secs` seconds.
    fn feed(h: &mut History, from: f64, secs: f64, bps: f64) {
        let mut t = from;
        while t < from + secs - 1e-9 {
            t += 0.25;
            h.add(t, (bps * 0.25) as u64, true);
        }
    }

    #[test]
    fn a_steady_speed_reads_steady() {
        let mut h = History::default();
        feed(&mut h, 0.0, 20.0, 2.0 * M);
        let s = h.seconds();
        assert!(s.len() >= 19, "{} seconds", s.len());
        for v in s.iter().skip(2) {
            assert!((*v as f64 - 2.0 * M).abs() < 0.01 * M, "{v}");
        }
        let avg = h.average().unwrap();
        assert!((avg - 2.0 * M).abs() < 0.01 * M, "{avg}");
        assert!((h.peak().unwrap() - 2.0 * M).abs() < 0.01 * M);
        assert!(
            h.current().unwrap() <= h.peak().unwrap(),
            "now is never over the peak"
        );
        assert_eq!(h.scale(), 2.5 * M, "2 MB/sec with room: 2.5 MB/sec");
    }

    #[test]
    fn bytes_between_far_apart_passes_are_spread_over_the_seconds() {
        // A minimized window's passes come seconds apart: 3 MB over 3 s is 1 MB/sec in each, not
        // 3 MB in one second.
        let mut h = History::default();
        h.add(10.0, 0, true);
        h.add(13.0, (3.0 * M) as u64, true);
        h.add(14.0, 0, true);
        let raw: Vec<f32> = h.seconds().iter().copied().collect();
        assert_eq!(raw.len(), 4);
        // The first closed second is the smoothing's start: 1 MB/sec, and it stays there.
        for v in &raw[..3] {
            assert!((*v as f64 - M).abs() < 1.0, "{raw:?}");
        }
        assert!((h.average().unwrap() - 0.75 * M).abs() < 1.0);
    }

    #[test]
    fn nothing_is_lost_or_made_up() {
        let mut h = History::default();
        let mut sent = 0u64;
        let mut t = 0.0;
        for k in 0..400u64 {
            t += 0.1 + (k % 7) as f64 * 0.05;
            let b = (k * 7919) % 50_000;
            sent += b;
            h.add(t, b, true);
        }
        assert!((h.bytes - sent as f64).abs() < 1.0);
    }

    #[test]
    fn the_scale_grows_at_once_and_shrinks_after_a_while() {
        let mut h = History::default();
        feed(&mut h, 0.0, 10.0, 0.8 * M);
        assert_eq!(h.scale(), M, "0.8 MB/sec fits under 1 MB/sec");
        feed(&mut h, 10.0, 1.0, 3.0 * M);
        assert_eq!(h.scale(), 2.5 * M, "grows as soon as a second goes over");
        feed(&mut h, 11.0, 3.0, 3.0 * M);
        assert_eq!(h.scale(), 4.0 * M);
        // Slower again: the fast seconds stay in view two minutes, and the scale with them.
        feed(&mut h, 14.0, 60.0, 0.8 * M);
        assert_eq!(h.scale(), 4.0 * M);
        // Once they have left the view, and a few seconds more, it comes down.
        feed(&mut h, 74.0, 80.0, 0.8 * M);
        assert_eq!(h.scale(), M);
    }

    #[test]
    fn a_scale_doesnt_flicker_around_a_step() {
        let mut h = History::default();
        let mut scales = Vec::new();
        let mut t = 0.0;
        for k in 0..240 {
            // Around 0.85 MB/sec, ±10 %: across the 1 MB/sec step once its room is added.
            let bps = M * (0.85 + 0.1 * ((k as f64) * 0.7).sin());
            for _ in 0..4 {
                t += 0.25;
                h.add(t, (bps * 0.25) as u64, true);
            }
            scales.push(h.scale());
        }
        scales.dedup();
        assert!(scales.len() <= 3, "{scales:?}");
    }

    #[test]
    fn a_long_sleep_doesnt_spread_or_hang() {
        let mut h = History::default();
        feed(&mut h, 0.0, 5.0, M);
        // Eight hours later: the seconds that can be seen are closed, empty (not 28,800 of them).
        h.add(8.0 * 3600.0, 1000, true);
        assert!(h.seconds().len() <= KEEP);
        assert!(*h.seconds().back().unwrap() < 1.0);
        assert_eq!(h.second, 8 * 3600, "on time");
    }

    #[test]
    fn round_scales() {
        let k = 1024.0;
        assert_eq!(nice_ceil(0.0), k);
        assert_eq!(nice_ceil(f64::NAN), k);
        assert_eq!(nice_ceil(1.5 * k), 1.5 * k);
        assert_eq!(nice_ceil(1.6 * k), 2.0 * k);
        assert_eq!(nice_ceil(300.0 * k), 300.0 * k);
        assert_eq!(nice_ceil(350.0 * k), 400.0 * k);
        assert_eq!(nice_ceil(600.0 * k), M, "past 500 KB: in MB");
        assert_eq!(nice_ceil(1.1 * M), 1.5 * M);
        assert_eq!(nice_ceil(9.0 * M), 10.0 * M);
        assert_eq!(nice_ceil(10.0 * M), 10.0 * M);
        assert_eq!(nice_ceil(11.0 * M), 15.0 * M);
        assert_eq!(nice_ceil(700.0 * M), M * k, "past 500 MB: in GB");
        assert_eq!(nice_ceil(3.0 * M * k), 3.0 * M * k);
        assert_eq!(nice_ceil(3.2 * M * k), 4.0 * M * k);
    }

    #[test]
    fn histories_follow_the_reads() {
        let mut s = Speeds::default();
        let read = |id, bytes, running| Read {
            id,
            bytes,
            running,
            site: String::new(),
            kind: String::new(),
        };
        for k in 0..40 {
            let t = k as f64 * 0.25;
            s.tick_at(t, &[read(1, 250_000, true), read(2, 50_000, true)]);
        }
        let (a, b) = (s.of(1).unwrap(), s.of(2).unwrap());
        assert!(a.seconds().len() >= 8 && b.seconds().len() >= 8);
        let last = |h: &History| *h.seconds().back().unwrap() as f64;
        assert!(
            (last(s.total()) - (last(a) + last(b))).abs() < 1.0,
            "the total is their sum"
        );
        // 2 stops (it ran at 200,000 bytes a second): its graph goes on in time, falling toward
        // nothing, and is let go much later.
        let seen = s.of(2).unwrap().seconds().len();
        s.tick_at(10.25, &[read(1, 250_000, true)]);
        s.tick_at(12.0, &[read(1, 500_000, true)]);
        let two = s.of(2).unwrap();
        assert!(two.seconds().len() > seen, "it goes on in time");
        assert!(*two.seconds().back().unwrap() < 100_000.0);
        s.tick_at(12.0 + FORGET + 20.0, &[read(1, 0, true)]);
        assert!(s.of(2).is_none(), "let go");
        assert!(s.of(1).is_some(), "still running");
    }
}
