//! The progress bar's glide: the bar follows the figures a little behind, at the pace of the
//! window's frames, and shows a finished download whole for a moment before its window closes.

/// The window's frames while its bar glides ([`Glide`]): as often as the bar moves a pixel, 25 a
/// second at most, only while its download runs and its figures move ([`glide_frame`]).
pub(super) const GLIDE_FRAME: std::time::Duration = std::time::Duration::from_millis(40);

/// The same while DDM's main window is shown: each frame draws it again too, about three times a
/// progress window's work (measured: 25 a second then took 5 % of a core more, 12.5 half that).
pub(super) const GLIDE_FRAME_MAIN: std::time::Duration = std::time::Duration::from_millis(80);

/// The app's own frames while a download runs (app.rs): a bar slower than a pixel in this long
/// needs no frames of its own.
pub(super) const APP_FRAME: f64 = 0.25;

/// The bar's width (513 points less its frame) and the segment map's (511).
pub(super) const BAR_WIDTH: f64 = 511.0;

/// A figure reached in about the time the last one took to come, a quarter more (seconds): the
/// next one comes before the bar has stopped, so it never halts between figures that keep
/// coming. Within these bounds (a figure after a long pause glides in half a second).
pub(super) const GLIDE_SLACK: f64 = 1.25;

pub(super) const GLIDE_MIN: f64 = 0.05;

pub(super) const GLIDE_MAX: f64 = 0.5;

/// Figures that moved this recently (seconds) keep the frames coming, so the next one is drawn
/// as it comes, not at the app's next quarter second.
pub(super) const GLIDE_HOLD: f64 = 1.0;

/// How long a finished download's window shows it whole before it gives way to Download
/// complete (app.rs): its bar, a glide behind the figures, would otherwise close short of the
/// end.
pub const SHOWN_WHOLE: std::time::Duration = std::time::Duration::from_millis(300);

/// The bar's fill and the segment map between the download's figures, which come about four
/// times a second: each part moves from where it was drawn to its new figure over about the
/// time between figures, so the bar glides where it jumped (the text keeps the figures as they
/// are). Never past a figure, never back: a figure lower than what is drawn (a download started
/// over) is drawn at once. The bar and the map move on the same clock, so they agree as their
/// figures do (a video's map is its bar; a file's map counts what is written, its bar also what
/// has come and waits to be, as before).
#[derive(Default)]
pub(super) struct Glide {
    /// The figures last read: bytes done, each segment's start and bytes done.
    pub(super) done: u64,
    pub(super) segments: Vec<(u64, u64)>,
    /// Where the drawing set off from towards them (the same segments, in order).
    pub(super) from_done: f64,
    pub(super) from_segments: Vec<f64>,
    /// When they were read (egui's clock, seconds) and how long it takes to reach them.
    pub(super) at: f64,
    pub(super) span: f64,
    pub(super) seen: bool,
    /// The bar's speed on its way to them (bytes a second).
    pub(super) rate: f64,
}

/// What a frame draws ([`Glide::step`]).
pub(super) struct Drawn {
    pub(super) done: f64,
    pub(super) segments: Vec<(u64, f64)>,
    /// How fast the bar moves (bytes a second), while it is on its way to the figures or they
    /// moved lately; else 0: no frames of its own wanted.
    pub(super) rate: f64,
}

impl Glide {
    /// How far the drawing is from `from` to the figures at `now` (0 to 1).
    pub(super) fn part(&self, now: f64) -> f64 {
        if self.span <= 0.0 {
            1.0
        } else {
            ((now - self.at) / self.span).clamp(0.0, 1.0)
        }
    }

    /// The figures read at `now` (`done` bytes; the segments' starts and bytes done): what to
    /// draw. `finished`: the download is done, its figures drawn as they are at once (the
    /// window shows it whole before it gives way to Download complete, not a glide behind).
    pub(super) fn step(
        &mut self,
        now: f64,
        done: u64,
        segments: &[(u64, u64)],
        finished: bool,
    ) -> Drawn {
        if !self.seen || finished {
            self.seen = true;
            self.at = now;
            self.span = 0.0;
            self.rate = 0.0;
            self.from_done = done as f64;
            self.from_segments = segments.iter().map(|&(_, d)| d as f64).collect();
            self.done = done;
            self.segments = segments.to_vec();
        } else if done != self.done || segments != self.segments.as_slice() {
            // Off from where it is drawn now; a part whose figure went below that, at once.
            let u = self.part(now);
            let at = |from: f64, to: u64| from + (to as f64 - from) * u;
            let drawn = at(self.from_done, self.done);
            let drawn_segments: Vec<(u64, f64)> = self
                .segments
                .iter()
                .zip(&self.from_segments)
                .map(|(&(start, d), &from)| (start, at(from, d)))
                .collect();
            // A segment that comes (a connection split one) grows from nothing, as the bar's
            // part of it does: the map's segments together stay what the bar shows.
            self.from_segments = segments
                .iter()
                .map(|&(start, d)| {
                    drawn_segments
                        .iter()
                        .find(|(s, _)| *s == start)
                        .map_or(0.0, |&(_, x)| x.min(d as f64))
                })
                .collect();
            let done_f = done as f64;
            self.from_done = drawn.min(done_f);
            self.span = ((now - self.at) * GLIDE_SLACK).clamp(GLIDE_MIN, GLIDE_MAX);
            self.rate = (done_f - self.from_done) / self.span;
            self.at = now;
            self.done = done;
            self.segments = segments.to_vec();
        }
        let u = self.part(now);
        let at = |from: f64, to: u64| from + (to as f64 - from) * u;
        let busy = u < 1.0 || now - self.at < GLIDE_HOLD;
        Drawn {
            done: at(self.from_done, self.done),
            segments: self
                .segments
                .iter()
                .zip(&self.from_segments)
                .map(|(&(start, d), &from)| (start, at(from, d)))
                .collect(),
            rate: if busy { self.rate } else { 0.0 },
        }
    }
}

/// When the progress window wants its next frame while its bar moves at `rate` bytes a second
/// of `total`, on a screen of `ppp` pixels a point: when the bar has moved a pixel, 25 frames a
/// second at most (12.5 while the main window is shown, `main`); none when it moves less than a
/// pixel between the app's own frames (a slow download, an unknown size, a bar standing still).
pub(super) fn glide_frame(
    rate: f64,
    total: Option<u64>,
    ppp: f32,
    main: bool,
) -> Option<std::time::Duration> {
    let total = total.filter(|t| *t > 0)? as f64;
    let pixels = rate / total * BAR_WIDTH * f64::from(ppp);
    let every = 1.0 / pixels;
    let cap = if main { GLIDE_FRAME_MAIN } else { GLIDE_FRAME };
    (pixels > 0.0 && every < APP_FRAME).then(|| std::time::Duration::from_secs_f64(every).max(cap))
}

#[cfg(test)]
mod glide_tests {
    use super::{GLIDE_HOLD, GLIDE_MAX, Glide, glide_frame};

    /// Figures every quarter second (`delta` bytes more each), read by frames every 40 ms that
    /// don't fall on them, as the app reads them: (time, the figure, what is drawn) a frame.
    fn run(frames: usize, delta: u64) -> Vec<(f64, u64, f64)> {
        let mut g = Glide::default();
        (0..frames)
            .map(|k| {
                let now = k as f64 * 0.04;
                let real = (now / 0.25).floor() as u64 * delta;
                (now, real, g.step(now, real, &[(0, real)], false).done)
            })
            .collect()
    }

    /// The first figures are drawn as they are.
    #[test]
    fn the_first_figures_are_drawn_as_they_are() {
        let mut g = Glide::default();
        let d = g.step(3.0, 700, &[(0, 300), (1000, 400)], false);
        assert_eq!(d.done, 700.0);
        assert_eq!(d.segments, [(0, 300.0), (1000, 400.0)]);
    }

    /// Between figures that keep coming the bar never stops, never goes back and never passes
    /// the figure read; it stays less than two figures behind.
    #[test]
    fn steady_figures_glide_without_a_stop() {
        let frames = run(200, 500_000);
        let mut last = 0.0;
        for (k, &(now, real, drawn)) in frames.iter().enumerate() {
            assert!(drawn <= real as f64 + 1e-6, "past the figure at {now}");
            assert!(drawn >= last - 1e-6, "back at {now}");
            if now > 0.3 {
                assert!(drawn > last, "stopped at frame {k} ({now:.2} s)");
                assert!(drawn >= real as f64 - 2.0 * 500_000.0, "behind at {now}");
            }
            last = drawn;
        }
    }

    /// A figure below what is drawn (the download started over) is drawn at once.
    #[test]
    fn a_figure_that_goes_back_is_drawn_at_once() {
        let mut g = Glide::default();
        g.step(0.0, 0, &[(0, 0)], false);
        g.step(0.25, 1000, &[(0, 1000)], false);
        let d = g.step(0.40, 1000, &[(0, 1000)], false);
        assert!(d.done > 0.0 && d.done < 1000.0);
        let d = g.step(0.41, 10, &[(0, 10)], false);
        assert_eq!(d.done, 10.0);
        assert_eq!(d.segments, [(0, 10.0)]);
    }

    /// A finished download is drawn whole at once, not a glide behind its last figures: its
    /// window shows it whole before it gives way to Download complete; no frames of its own.
    #[test]
    fn a_finished_download_is_drawn_whole_at_once() {
        let mut g = Glide::default();
        g.step(0.0, 0, &[(0, 0)], false);
        g.step(0.25, 999, &[(0, 999)], false);
        let d = g.step(0.30, 999, &[(0, 999)], false);
        assert!(d.done < 999.0, "{}", d.done);
        let d = g.step(0.31, 1000, &[(0, 1000)], true);
        assert_eq!(d.done, 1000.0);
        assert_eq!(d.segments, [(0, 1000.0)]);
        assert_eq!(d.rate, 0.0);
        let d = g.step(0.35, 1000, &[(0, 1000)], true);
        assert_eq!(d.done, 1000.0);
    }

    /// A file's bar counts what has come before it is written (the engine's map moves later):
    /// it glides on its own figure all the same.
    #[test]
    fn the_bar_glides_on_its_own_figure() {
        let mut g = Glide::default();
        g.step(0.0, 0, &[(0, 0)], false);
        let d = g.step(0.25, 1000, &[(0, 0)], false);
        assert!(d.rate > 0.0);
        let d = g.step(0.40, 1000, &[(0, 0)], false);
        assert!(d.done > 0.0 && d.done < 1000.0, "{}", d.done);
        assert_eq!(d.segments, [(0, 0.0)]);
    }

    /// The segment map moves with the bar: together its segments are always what the bar shows,
    /// and a segment that comes (a connection split one) grows from nothing.
    #[test]
    fn the_segment_map_moves_with_the_bar() {
        let mut g = Glide::default();
        let mut now = 0.0;
        let mut segs = vec![(0u64, 0u64), (5_000, 0)];
        for k in 0..60 {
            if k % 6 == 0 {
                segs[0].1 += 300;
                segs[1].1 += 100;
                if k == 30 {
                    segs.push((8_000, 50));
                }
            }
            let done: u64 = segs.iter().map(|s| s.1).sum();
            let d = g.step(now, done, &segs, false);
            let sum: f64 = d.segments.iter().map(|s| s.1).sum();
            assert!(
                (sum - d.done).abs() < 1e-6,
                "map {sum} vs bar {} at {now}",
                d.done
            );
            for ((_, shown), (_, real)) in d.segments.iter().zip(&segs) {
                assert!(*shown <= *real as f64 + 1e-6);
            }
            now += 0.04;
        }
    }

    /// Once the figures stop moving the bar reaches them and the frames stop being asked for.
    #[test]
    fn frames_are_asked_for_only_while_the_figures_move() {
        let mut g = Glide::default();
        assert_eq!(g.step(0.0, 0, &[(0, 0)], false).rate, 0.0);
        let d = g.step(0.25, 100, &[(0, 100)], false);
        assert!((d.rate - 100.0 / (0.25 * 1.25)).abs() < 1e-9, "{}", d.rate);
        let d = g.step(0.25 + GLIDE_MAX, 100, &[(0, 100)], false);
        assert_eq!(d.done, 100.0);
        assert!(d.rate > 0.0, "moved lately");
        let d = g.step(0.25 + GLIDE_HOLD + 0.01, 100, &[(0, 100)], false);
        assert_eq!(d.done, 100.0);
        assert_eq!(d.rate, 0.0, "still: the app's own pace");
    }

    /// The window is drawn as often as its bar moves a pixel, 25 times a second at most (12.5
    /// while the main window is shown); a bar that moves less than a pixel between the app's own
    /// frames asks for none.
    #[test]
    fn frames_come_as_often_as_the_bar_moves_a_pixel() {
        use std::time::Duration;
        let mb = 1024.0 * 1024.0;
        let total = Some(100 * 1024 * 1024);
        // 100 MB at 10 MB/s: 51 pixels a second, at the cap.
        assert_eq!(
            glide_frame(10.0 * mb, total, 1.0, false),
            Some(Duration::from_millis(40))
        );
        assert_eq!(
            glide_frame(10.0 * mb, total, 1.0, true),
            Some(Duration::from_millis(80))
        );
        // At 2 MB/s: 10 pixels a second (15 at 150 %).
        let every = glide_frame(2.0 * mb, total, 1.0, false).unwrap();
        assert!((every.as_secs_f64() - 1.0 / (0.02 * 511.0)).abs() < 1e-3);
        assert_eq!(glide_frame(2.0 * mb, total, 1.0, true), Some(every));
        let every = glide_frame(2.0 * mb, total, 1.5, false).unwrap();
        assert!((every.as_secs_f64() - 1.0 / (0.03 * 511.0)).abs() < 1e-3);
        // At 0.5 MB/s: 2.6 pixels a second, the app's 4 frames do.
        for main in [false, true] {
            assert_eq!(glide_frame(0.5 * mb, total, 1.0, main), None);
            assert_eq!(glide_frame(0.0, total, 1.0, main), None);
            assert_eq!(glide_frame(10.0 * mb, None, 1.0, main), None);
            assert_eq!(glide_frame(10.0 * mb, Some(0), 1.0, main), None);
        }
    }
}
