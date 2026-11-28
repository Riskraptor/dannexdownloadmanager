//! Downloads → Statistics... (beyond IDM): what came today, this week, this month and since
//! counting began, the last 30 days' bars, the sites that sent the most and the kinds of files
//! by their share; Reset forgets it all, once asked. The figures are `stats.rs`'s.

use chrono::NaiveDate;
use egui::{Painter, Pos2, Ui};

use crate::dialogs::Outcome;
use crate::model::{self, Category};
use crate::speedgraph;
use crate::stats::{self, Stats, Tally};
use crate::winui::{self, ButtonKind, r};

/// The window's client size.
const SIZE: [f32; 2] = [560.0, 530.0];
/// Days in the bar chart.
const DAYS: usize = 30;
/// Rows in each list (top sites, file types).
const ROWS: usize = 5;

/// The Statistics window, and its question before a reset.
#[derive(Default)]
pub struct Statistics {
    confirm: Option<crate::dialogs::Confirm>,
}

/// Draws the window while it is open (`slot`); resets `rec` when Reset is asked and confirmed.
pub fn run(ctx: &egui::Context, slot: &mut Option<Statistics>, rec: &mut stats::Recorder) {
    let Some(w) = slot else { return };
    let (open, reset) = w.show(ctx, rec.stats());
    if reset {
        rec.reset();
    }
    if !open {
        *slot = None;
    }
}

impl Statistics {
    /// (still open, reset confirmed).
    fn show(&mut self, ctx: &egui::Context, s: &Stats) -> (bool, bool) {
        let today = crate::clock::now().date_naive();
        let mut ask = false;
        let out = crate::dialogs::window(
            ctx,
            "statistics",
            "Statistics",
            SIZE,
            true,
            winui::LIGHT_BG(),
            |ui, o| {
                summary(ui, o, s, today);
                days(ui, o, s, today);
                sites(ui, o, s);
                kinds(ui, o, s);
                footer(ui, o, s, &mut ask)
            },
        );
        if ask && self.confirm.is_none() {
            // i18n: title of the question asked before Statistics are reset
            let title = crate::i18n::mark("Reset statistics");
            let question = crate::i18n::t(
                // i18n: Statistics → Reset...: the question asked first (the downloads stay in the list)
                "Forget every statistic counted so far? This can't be undone.",
            );
            self.confirm = Some(crate::dialogs::Confirm::new(title, question.to_owned()));
        }
        let mut reset = false;
        if let Some(c) = &mut self.confirm {
            match c.show(ctx) {
                Outcome::Open => {}
                Outcome::Done(()) => {
                    reset = true;
                    self.confirm = None;
                }
                Outcome::Closed => self.confirm = None,
            }
        }
        let open = matches!(out, Outcome::Open);
        if !open {
            self.confirm = None;
        }
        (open, reset)
    }
}

/// A group box with its caption in its top line (Options' own look).
fn group(ui: &Ui, o: Pos2, x: f32, y: f32, w: f32, h: f32, caption: &str) {
    let p = ui.painter();
    let full = crate::i18n::t(caption);
    let caption = winui::ellipsize(p, &winui::tahoma(), full, w - 18.0);
    let cw = winui::text_width(p, &caption);
    winui::group_box(
        p,
        r(o, x, y, w, h),
        Some((o.x + x + 7.0, o.x + x + 9.0 + cw + 2.0)),
    );
    let rc = winui::text(
        p,
        o.x + x + 9.0,
        o.y + y + 4.0,
        &caption,
        winui::LIGHT_TEXT(),
    );
    if caption != full {
        winui::full_on_hover(ui, rc, ("stats-group", x as i32), full);
    }
}

/// A muted note in the middle of an empty box (English x range, baseline).
fn note_centred(p: &Painter, o: Pos2, x0: f32, x1: f32, baseline: f32, s: &str) {
    let shown = winui::ellipsize(p, &winui::tahoma(), s, x1 - x0 - 8.0);
    let sw = winui::text_width(p, &shown);
    winui::text(
        p,
        o.x + ((x0 + x1 - sw) / 2.0).round(),
        o.y + baseline,
        &shown,
        winui::MUTED(),
    );
}

/// A row of the table at the top: its title, and what it shows of a period.
type SummaryRow = (&'static str, fn(&Tally) -> String);

/// The table at the top: what came today, this week, this month and in all, in bytes, files and
/// average speed.
fn summary(ui: &Ui, o: Pos2, s: &Stats, today: NaiveDate) {
    let p = ui.painter();
    let week = stats::week_start(today, crate::regional::style().week_start);
    let periods = [
        s.between(today, today),
        s.between(week, today),
        s.between(stats::month_start(today), today),
        s.all_time(),
    ];
    let heads = [
        // i18n: Statistics, column title: what came today
        crate::i18n::mark("Today"),
        // i18n: Statistics, column title: what came this week (from the week's first day, as Options → Regional sets it)
        crate::i18n::mark("This week"),
        // i18n: Statistics, column title: what came this month
        crate::i18n::mark("This month"),
        // i18n: Statistics, column title: everything since counting began
        crate::i18n::mark("All time"),
    ];
    let rows: [SummaryRow; 3] = [
        // i18n: Statistics, row title: the bytes that came
        (crate::i18n::mark("Downloaded"), |t: &Tally| {
            model::size(t.bytes)
        }),
        // i18n: Statistics, row title: how many downloads finished
        (crate::i18n::mark("Files finished"), |t: &Tally| {
            crate::i18n::integer(t.files)
        }),
        // i18n: Statistics, row title: the average speed while downloads were receiving data
        (crate::i18n::mark("Average speed"), |t: &Tally| {
            // No time counted: a dash, not a speed of nothing.
            t.speed()
                .map(speedgraph::rate2)
                .unwrap_or_else(|| "\u{2013}".to_owned())
        }),
    ];
    let labels: Vec<&str> = rows.iter().map(|(l, _)| crate::i18n::t(l)).collect();
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|(_, value)| periods.iter().map(value).collect())
        .collect();
    // The values' columns share what the row titles leave: from x 150, or further left when a
    // value or a column title needs the room (a long unit: "ميغابايت/ثانية"), never into a title.
    let right_end = 544.0;
    let widest = heads
        .iter()
        .map(|h| crate::i18n::t(h).to_owned())
        .chain(cells.iter().flatten().cloned())
        .map(|s| winui::text_width(p, &s))
        .fold(0.0_f32, f32::max);
    let titles_end = winui::label_column(p, 16.0, &labels, 0.0);
    let first = (right_end - 4.0 * (widest + 8.0).ceil())
        .min(150.0)
        .max(titles_end);
    let cw = ((right_end - first) / 4.0).floor();
    for (k, head) in heads.iter().enumerate() {
        let right = first + cw * (k + 1) as f32;
        let (rc, cut) = winui::text_right_within(
            p,
            o.x + right,
            o.y + 22.0,
            crate::i18n::t(head),
            winui::MUTED(),
            o.x + right - cw + 4.0,
        );
        if cut {
            winui::full_on_hover(ui, rc, ("stats-head", k), crate::i18n::t(head));
        }
    }
    winui::etched_h(p, o.x + 12.0, o.y + 30.0, 536.0);
    for (row, (values, label)) in cells.iter().zip(&labels).enumerate() {
        let base = 48.0 + 18.0 * row as f32;
        winui::text(p, o.x + 16.0, o.y + base, label, winui::LIGHT_TEXT());
        for (k, v) in values.iter().enumerate() {
            let right = first + cw * (k + 1) as f32;
            let (rc, cut) = winui::text_right_within(
                p,
                o.x + right,
                o.y + base,
                v,
                winui::LIGHT_TEXT(),
                o.x + right - cw + 4.0,
            );
            if cut {
                winui::full_on_hover(ui, rc, ("stats-value", row, k), v);
            }
        }
    }
}

/// A day as the chart's axis writes it, its month and day as the dates in use write them:
/// `11/18` (English), `18.11.` (German), `18/11`, `11-18`.
fn day_label(d: NaiveDate) -> String {
    crate::regional::month_day(d)
}

/// The bar chart of the last 30 days, today last (at the end of the reading direction), each
/// day's bytes and files on hover.
fn days(ui: &Ui, o: Pos2, s: &Stats, today: NaiveDate) {
    let p = ui.painter();
    // i18n: Statistics, group box title: a bar chart of the bytes downloaded each day
    let caption = crate::i18n::mark("Downloaded each day, the last 30 days");
    group(ui, o, 12.0, 100.0, 536.0, 192.0, caption);
    let days = s.last_days(today, DAYS);
    let most = days.iter().map(|(_, t)| t.bytes).max().unwrap_or(0);
    // The chart: x 70..538, y 126..262 (English), the axis' sizes left of it, the busiest day
    // over its end.
    let (cx, cy, cw, ch) = (70.0, 126.0, 468.0, 136.0);
    if let Some((day, t)) = days
        .iter()
        .filter(|(_, t)| t.bytes > 0)
        .max_by(|a, b| a.1.bytes.cmp(&b.1.bytes).then_with(|| a.0.cmp(&b.0)))
    {
        let busiest = crate::i18n::fmt(
            // i18n: Statistics, over the bar chart: the day of the last 30 that the most came; {date} = a date like 31/10/2026, {size} = like "2.10 GB"
            crate::i18n::t("Busiest day: {date}, {size}"),
            &[
                (
                    "date",
                    &crate::i18n::ltr(&crate::regional::short_date(*day)),
                ),
                // A size reads in the language's direction (an Arabic unit after its number,
                // at its left): fmt isolates it as it is, first strong letter first.
                ("size", &model::size(t.bytes)),
            ],
        );
        let (rc, cut) = winui::text_right_within(
            p,
            o.x + cx + cw,
            o.y + cy - 9.0,
            &busiest,
            winui::MUTED(),
            o.x + cx + cw / 3.0,
        );
        if cut {
            winui::full_on_hover(ui, rc, "stats-busiest", &busiest);
        }
    }
    let rc = r(o, cx, cy, cw, ch);
    let inner = speedgraph::chart_box(p, rc, 1);
    let border = rc.height() - inner.height();
    let c = crate::theme::colors();
    if most == 0 {
        // i18n: Statistics: inside the empty bar chart
        let none = crate::i18n::t("Nothing downloaded in the last 30 days");
        note_centred(p, o, cx, cx + cw, cy + ch / 2.0 + 4.0, none);
    } else {
        let scale = speedgraph::nice_ceil(most as f64 * 1.05);
        // The axis: the top line's size and the middle one's, ending before the chart.
        for (frac, y) in [(1.0, cy + 1.0), (0.5, cy + ch / 2.0)] {
            winui::text_right(
                p,
                o.x + cx - 6.0,
                o.y + y + 4.0,
                &speedgraph::size_short(scale * frac),
                winui::MUTED(),
            );
        }
        let slot = cw / DAYS as f32;
        let bar = (slot - 4.0).max(2.0).floor();
        let usable = ch - border - 2.0;
        for (i, (day, t)) in days.iter().enumerate() {
            let x = cx + slot * i as f32;
            let hover = ui
                .interact(
                    r(o, x, cy, slot, ch),
                    egui::Id::unique(("stats-day", i)),
                    egui::Sense::hover(),
                )
                .on_hover_text(day_tip(*day, t));
            if t.bytes == 0 {
                continue;
            }
            let h = ((t.bytes as f64 / scale) as f32 * usable)
                .clamp(1.0, usable)
                .round();
            let colour = if hover.hovered() {
                c.graph_bar_hot
            } else {
                c.graph_bar
            };
            let top = cy + ch - border / 2.0 - h;
            p.rect_filled(
                r(o, (x + (slot - bar) / 2.0).round(), top, bar, h),
                1.0,
                colour,
            );
        }
        // Every seventh day's date under its bar, today's last.
        for i in (0..DAYS).rev().step_by(7) {
            let label = crate::i18n::ltr(&day_label(days[i].0));
            let w = winui::text_width(p, &label);
            let centre = cx + slot * (i as f32 + 0.5);
            winui::text(
                p,
                o.x + (centre - w / 2.0).round(),
                o.y + cy + ch + 14.0,
                &label,
                winui::MUTED(),
            );
        }
    }
    speedgraph::chart_frame(p, rc);
}

/// A day's bar's tooltip: its date, its bytes and the downloads finished.
fn day_tip(day: NaiveDate, t: &Tally) -> String {
    format!(
        "{}\n{}\n{}",
        crate::i18n::ltr(&crate::regional::short_date(day)),
        model::size(t.bytes),
        files_finished(t.files)
    )
}

fn files_finished(n: u64) -> String {
    // i18n: Statistics, in a tooltip: how many downloads finished
    crate::i18n::tn(n, "{n} file finished", "{n} files finished")
}

/// One row of a list: its name, its figure at the end, and a bar under them its share long; its
/// details on hover.
#[allow(clippy::too_many_arguments)]
fn share_row(
    ui: &Ui,
    o: Pos2,
    x: f32,
    w: f32,
    top: f32,
    name: &str,
    figure: &str,
    share: f64,
    tip: &str,
    salt: (&str, usize),
) {
    let p = ui.painter();
    let c = crate::theme::colors();
    let base = top + 12.0;
    let fw = winui::text_width(p, figure).ceil();
    winui::text_right(p, o.x + x + w, o.y + base, figure, winui::LIGHT_TEXT());
    let shown = winui::ellipsize(p, &winui::tahoma(), name, (w - fw - 10.0).max(0.0));
    winui::text(p, o.x + x, o.y + base, &shown, winui::LIGHT_TEXT());
    // The bar: a track the row's width, its share filled from the start.
    let track = r(o, x, top + 17.0, w, 4.0);
    p.rect_filled(track, 2.0, c.graph_grid);
    let hover = ui
        .interact(
            r(o, x, top, w, 24.0),
            egui::Id::unique(salt),
            egui::Sense::hover(),
        )
        .on_hover_text(if shown == name {
            tip.to_owned()
        } else {
            format!("{name}\n{tip}")
        });
    let filled = (share.clamp(0.0, 1.0) as f32 * w).round();
    if filled > 0.0 {
        let colour = if hover.hovered() {
            c.graph_bar_hot
        } else {
            c.graph_bar
        };
        p.rect_filled(r(o, x, top + 17.0, filled.max(2.0), 4.0), 2.0, colour);
    }
}

/// `share` of a whole, as the tooltip says it.
fn share_text(share: f64) -> String {
    crate::i18n::fmt(
        // i18n: Statistics, in a tooltip: {percent} = a site's or a kind of file's share, like "12.5%"
        crate::i18n::t("{percent} of everything downloaded"),
        &[("percent", &crate::i18n::percent(share, 1))],
    )
}

/// The sites that sent the most, by bytes.
fn sites(ui: &Ui, o: Pos2, s: &Stats) {
    let (x, y, w, h) = (12.0, 304.0, 262.0, 182.0);
    // i18n: Statistics, group box title: the web sites downloaded from the most, by bytes
    group(ui, o, x, y, w, h, "Top sites");
    let top = s.top_sites(ROWS);
    let all = s.sites_bytes().max(1) as f64;
    if top.is_empty() {
        // i18n: Statistics: inside an empty list (top sites, file types)
        let none = crate::i18n::t("Nothing downloaded yet");
        note_centred(ui.painter(), o, x, x + w, y + h / 2.0 + 6.0, none);
        return;
    }
    for (k, (site, t)) in top.iter().enumerate() {
        let name = if *site == stats::NO_SITE {
            // A torrent's bytes come from its peers: their row is named as their category.
            Category::Torrents.shown()
        } else {
            crate::i18n::ltr(site)
        };
        let share = t.bytes as f64 / all;
        share_row(
            ui,
            o,
            x + 10.0,
            w - 20.0,
            y + 18.0 + 31.0 * k as f32,
            &name,
            &model::size(t.bytes),
            share,
            &format!("{}\n{}", share_text(share), files_finished(t.files)),
            ("stats-site", k),
        );
    }
}

/// A kind of file's name: its category's, as the Categories tree shows it (a category of the
/// user's since deleted keeps the name it had).
fn kind_name(key: &str) -> String {
    Category::BUILT_IN
        .into_iter()
        .chain([Category::General])
        .chain(Category::named())
        .find(|c| c.key() == key)
        .map_or_else(|| crate::i18n::isolate(key), Category::shown)
}

/// The kinds of files, by their share of the bytes.
fn kinds(ui: &Ui, o: Pos2, s: &Stats) {
    let (x, y, w, h) = (286.0, 304.0, 262.0, 182.0);
    // i18n: Statistics, group box title: the kinds of files downloaded (Video, Music…), by their share
    group(ui, o, x, y, w, h, "File types");
    let kinds = s.kinds();
    let all = s.kinds_bytes().max(1) as f64;
    if kinds.is_empty() {
        // i18n: Statistics: inside an empty list (top sites, file types)
        let none = crate::i18n::t("Nothing downloaded yet");
        note_centred(ui.painter(), o, x, x + w, y + h / 2.0 + 6.0, none);
        return;
    }
    for (k, (kind, t)) in kinds.iter().take(ROWS).enumerate() {
        let share = t.bytes as f64 / all;
        share_row(
            ui,
            o,
            x + 10.0,
            w - 20.0,
            y + 18.0 + 31.0 * k as f32,
            &kind_name(kind),
            &crate::i18n::percent(share, 1),
            share,
            &format!("{}\n{}", model::size(t.bytes), files_finished(t.files)),
            ("stats-kind", k),
        );
    }
}

/// Since when it counts, Reset... and Close. Sets `ask` when Reset... is pressed.
fn footer(ui: &mut Ui, o: Pos2, s: &Stats, ask: &mut bool) -> Outcome<()> {
    let p = ui.painter().clone();
    let since = match s.since() {
        Some(d) => crate::i18n::fmt(
            // i18n: Statistics, at the bottom: since when downloads are counted (the first one, or the last reset); {date} = a date like 31/10/2026
            crate::i18n::t("Counted since {date}"),
            &[("date", &crate::i18n::ltr(&crate::regional::short_date(d)))],
        ),
        // i18n: Statistics, at the bottom: nothing has been counted yet
        None => crate::i18n::t("Nothing counted yet").to_owned(),
    };
    let [reset_at, close_at] = winui::button_row(
        &p,
        &[("Reset...", 389.0, 80.0), ("Close", 475.0, 73.0)],
        8.0,
        200.0,
        200.0,
    )[..] else {
        unreachable!("two buttons")
    };
    winui::fit(
        ui,
        &p,
        o.x + 12.0,
        o.y + 512.0,
        &since,
        winui::MUTED(),
        o.x + reset_at.0 - 10.0,
        "stats-since",
    );
    let empty = s.all_time().is_empty();
    let reset = winui::button_light(
        ui,
        r(o, reset_at.0, 498.0, reset_at.1, 21.0),
        // i18n: Statistics: button that forgets every statistic (after a question)
        "Reset...",
        if empty {
            ButtonKind::Disabled
        } else {
            ButtonKind::Normal
        },
        false,
    )
    .clicked();
    let close = winui::button_light(
        ui,
        r(o, close_at.0, 498.0, close_at.1, 21.0),
        "Close",
        ButtonKind::Default,
        false,
    )
    .clicked();
    if reset && !empty {
        *ask = true;
    }
    if close || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
        Outcome::Closed
    } else {
        Outcome::Open
    }
}
