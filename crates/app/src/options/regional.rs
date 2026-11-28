//! Options → Regional (ours): how dates and times are written in the list and the Scheduler.

use super::*;

impl OptionsDialog {
    /// Regional (ours): how dates and times are written in the list and the Scheduler, as the
    /// system's region writes them or the user's own way.
    pub(super) fn regional(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        header(ui, o, Icon::PageRegional, "Dates and times");
        let reg = &mut self.s.regional;
        group(ui, o, 16.0, 92.0, 392.0, 250.0, "Write dates and times");
        let system = winui::radio_light(
            ui,
            o + vec2(30.0, 112.0),
            !reg.custom,
            // i18n: Options → Regional: dates and times written the way of DDM's language (the system's region's own way when that is in the same language)
            "As the language and the system's region write them",
            true,
        );
        let own = winui::radio_light(ui, o + vec2(30.0, 140.0), reg.custom, "My own way:", true);
        match winui::radio_keys(ui, &[system.clone(), own.clone()]) {
            Some(i) => reg.custom = i == 1,
            None if system.clicked() => reg.custom = false,
            None if own.clicked() => reg.custom = true,
            None => {}
        }
        let on = reg.custom;
        // Each choice: its label right-aligned before its box.
        let row = |ui: &mut Ui, y: f32, label: &str, items: &[&str], at: usize, id: &str| {
            let label = crate::i18n::t(label);
            let color = if on { LIGHT_TEXT() } else { LIGHT_DISABLED() };
            let (rect, cut) =
                winui::text_right_within(&p, o.x + 186.0, o.y + y + 15.0, label, color, o.x + 48.0);
            if cut {
                winui::full_on_hover(ui, rect, ("regional-label", y as i32), label);
            }
            winui::combo_light(ui, r(o, 192.0, y, 200.0, 21.0), items, at, on, id)
        };
        let orders = [
            // i18n: a date's order: day first (30/10/2026)
            crate::i18n::mark("Day, month, year"),
            // i18n: a date's order: month first (10/30/2026)
            crate::i18n::mark("Month, day, year"),
            // i18n: a date's order: year first (2026-10-30)
            crate::i18n::mark("Year, month, day"),
        ];
        let order_at = match reg.order {
            DateOrder::Dmy => 0,
            DateOrder::Mdy => 1,
            DateOrder::Ymd => 2,
        };
        // i18n: label before the choice of a date's order
        if let Some(i) = row(
            ui,
            168.0,
            crate::i18n::mark("Date order:"),
            &orders,
            order_at,
            "regional-order",
        ) {
            reg.order = [DateOrder::Dmy, DateOrder::Mdy, DateOrder::Ymd][i];
        }
        let clocks = [
            // i18n: a 12-hour clock, with an example
            crate::i18n::mark("12-hour (9:41 PM)"),
            // i18n: a 24-hour clock, with an example
            crate::i18n::mark("24-hour (21:41)"),
        ];
        // i18n: label before the choice of a 12- or 24-hour clock
        if let Some(i) = row(
            ui,
            198.0,
            crate::i18n::mark("Clock:"),
            &clocks,
            reg.clock24 as usize,
            "regional-clock",
        ) {
            reg.clock24 = i == 1;
        }
        let days = [
            crate::i18n::mark("Sunday"),
            crate::i18n::mark("Monday"),
            crate::i18n::mark("Saturday"),
        ];
        let day_at = match reg.week_start {
            1 => 1,
            6 => 2,
            _ => 0,
        };
        // i18n: label before the choice of the week's first day (in the Scheduler's calendar)
        if let Some(i) = row(
            ui,
            228.0,
            crate::i18n::mark("First day of the week:"),
            &days,
            day_at,
            "regional-week",
        ) {
            reg.week_start = [0, 1, 6][i];
        }
        // What it looks like, now.
        let now = crate::clock::now().naive_local();
        let sample = crate::regional::sample(reg, now);
        // i18n: label before an example date and time written the chosen way
        let label = crate::i18n::t("Example:");
        text(&p, o.x + 48.0, o.y + 290.0, label, LIGHT_TEXT());
        let lw = winui::text_width(&p, label);
        winui::text_in_window(
            ui,
            &p,
            o.x + 48.0 + lw + 8.0,
            o.y + 290.0,
            &crate::i18n::ltr(&sample),
            LIGHT_TEXT(),
        );
        // i18n: a note under the date and time choices
        winui::text_in_window(
            ui,
            &p,
            o.x + 30.0,
            o.y + 320.0,
            crate::i18n::t("Used by the list of downloads and the Scheduler."),
            LIGHT_DISABLED(),
        );
    }
}
