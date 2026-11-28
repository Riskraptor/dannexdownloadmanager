//! The Speed Limiter's settings, and the notice a download limit shows when it is reached.

use super::*;

// ---- Speed Limiter settings (client 453 × 223 or wider, light) -----------------------------

// i18n: Speed Limiter settings: checkbox; the limiter is turned on each time the program starts
pub(super) const SPEED_ON_STARTUP: &str =
    crate::i18n::mark("Turn the limiter on whenever Dannex Download Manager starts");

/// Downloads → Speed Limiter → Settings... (notes/ui-spec/scheduler-and-misc.md §2).
pub struct SpeedSettings {
    pub kb: String,
    pub on_startup: bool,
    /// Beyond IDM (D-012): one limit for all downloads together.
    pub global_on: bool,
    pub global_kb: String,
    pub only_active: bool,
    /// How much taller the note is than IDM's three lines (a longer translation): the rows
    /// below it and the window move down by this much (from the frame before).
    pub(super) note_extra: f32,
}

/// What Speed Limiter settings → OK sets.
pub struct SpeedChoice {
    pub kb: u64,
    pub on_startup: bool,
    pub global_on: bool,
    pub global_kb: u64,
    pub only_active: bool,
}

impl SpeedSettings {
    pub fn new(s: &crate::settings::Settings) -> Self {
        SpeedSettings {
            kb: s.speed_limit_kb.to_string(),
            on_startup: s.limiter_on_startup,
            global_on: s.global_limit_on,
            global_kb: s.global_limit_kb.to_string(),
            only_active: s.global_only_active,
            note_extra: 0.0,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<SpeedChoice> {
        let extra = self.note_extra;
        // As wide as its longest row needs in this language (453 at least), 12 px margins.
        let w = {
            let p = ctx.layer_painter(egui::LayerId::background());
            let tw = |s: &str| winui::text_width(&p, crate::i18n::t(s)).ceil();
            let startup = 12.0 + 16.0 + tw(SPEED_ON_STARTUP) + 12.0;
            let global = 12.0
                + 16.0
                + tw("Limit all downloads together to")
                + 8.0
                + 60.0
                + 8.0
                + tw("KBytes/sec")
                + 12.0;
            startup.max(global).clamp(453.0, 640.0)
        };
        window(
            ctx,
            "speed-settings",
            "Speed Limiter settings",
            [w, 223.0 + extra],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let black = winui::LIGHT_TEXT();
                // i18n: Speed Limiter settings: the limit each download is held to while the limiter is on; {kb} is an input box (kilobytes per second), drawn where it stands in the translation
                let limit = crate::i18n::t("Limit each download to {kb} KBytes/sec");
                // From the window's left margin, the box right after the words before it.
                let lead_w = match winui::sentence_pieces(limit, &["kb"])
                    .first()
                    .and_then(|l| l.first())
                {
                    Some(winui::Piece::Text(s)) if !s.is_empty() => {
                        winui::text_width(&p, s).ceil() + 6.0
                    }
                    _ => 0.0,
                };
                let kb_box = winui::sentence(
                    ui,
                    &p,
                    limit,
                    &[winui::SentenceLine {
                        lead: winui::Lead::Left(o.x + 12.0),
                        lead_baseline: o.y + 26.0,
                        boxes: o + vec2(12.0 + lead_w, 12.0),
                        gap: 8.0,
                        baseline: o.y + 26.0,
                    }],
                    &[("kb", vec2(50.0, 23.0))],
                    |p, x, baseline, s| {
                        winui::text(p, x, baseline, s, black);
                    },
                );
                winui::number_field(
                    ui,
                    kb_box[0],
                    &mut self.kb,
                    1,
                    9_999_999,
                    true,
                    winui::LIGHT_BG(),
                );
                let note = crate::i18n::t(
                    // i18n: Speed Limiter settings: a note under the limit (wrapped to the window's width)
                    "A very low limit can make some servers drop the connection. From a server that can't resume downloads, the file may then have to start over.",
                );
                // Wrapped inside the window's margins; each line more or less than three moves
                // the rest and the window's foot by a line.
                let lines = winui::paragraph(
                    &p,
                    o.x + 12.0,
                    o.y + 54.0,
                    note,
                    black,
                    o.x + w - 12.0,
                    14.0,
                );
                self.note_extra = (lines.max(1) as f32 - 3.0) * 14.0;
                let o = o + vec2(0.0, self.note_extra);
                winui::checkbox_light(
                    ui,
                    o + vec2(12.0, 96.0),
                    &mut self.on_startup,
                    SPEED_ON_STARTUP,
                    true,
                );
                // Beyond IDM: a limit for all downloads together.
                winui::etched_h(&p, o.x + 12.0, o.y + 121.0, w - 24.0);
                // The box after the caption, however long it is in this language.
                let global_x =
                    (winui::checkbox_caption_end(&p, 12.0, "Limit all downloads together to")
                        + 8.0)
                        .clamp(
                            196.0,
                            w - 12.0
                                - 60.0
                                - 8.0
                                - winui::text_width(&p, crate::i18n::t("KBytes/sec")),
                        );
                winui::checkbox_light_until(
                    ui,
                    o + vec2(12.0, 135.0),
                    &mut self.global_on,
                    "Limit all downloads together to",
                    true,
                    Some(o.x + global_x),
                );
                let on = self.global_on;
                winui::number_field(
                    ui,
                    r(o, global_x, 130.0, 60.0, 23.0),
                    &mut self.global_kb,
                    1,
                    9_999_999,
                    on,
                    winui::LIGHT_BG(),
                );
                let c = if on { black } else { winui::LIGHT_DISABLED() };
                // i18n: unit after an input box: kilobytes per second
                winui::text(
                    &p,
                    o.x + global_x + 68.0,
                    o.y + 145.0,
                    crate::i18n::t("KBytes/sec"),
                    c,
                );
                // Whether someone is at the computer is Windows' to say (its last input,
                // win32::idle_seconds); Linux and macOS have no such question DDM asks yet, so
                // the box is greyed there and the limit applies all the time.
                let can_tell = cfg!(windows);
                let only = winui::checkbox_light(
                    ui,
                    o + vec2(30.0, 159.0),
                    &mut self.only_active,
                    "Only while I'm using the computer",
                    on && can_tell,
                );
                if !can_tell {
                    // i18n: tooltip of the greyed "Only while I'm using the computer" (Speed Limiter settings) on Linux and macOS
                    let why = crate::i18n::t(
                        "Not on this system yet: DDM can't tell here whether you are using the computer, so the limit applies all the time",
                    );
                    only.on_hover_text(why).on_disabled_hover_text(why);
                }
                // The buttons at the window's foot, their right edge at the margin.
                let ok = winui::button_light(
                    ui,
                    r(o, w - 167.0, 190.0, 73.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    true,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, w - 85.0, 190.0, 73.0, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if ok || winui::enter_pressed(ui) {
                    Outcome::Done(SpeedChoice {
                        kb: winui::number_in(&self.kb, 1, 9_999_999),
                        on_startup: self.on_startup,
                        global_on: self.global_on,
                        global_kb: winui::number_in(&self.global_kb, 1, 9_999_999),
                        only_active: self.only_active,
                    })
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

// ---- Download limit reached (light) ---------------------------------------------------------

/// "Show warning before stopping downloads" (Scheduler → Download limits): says the limit was
/// reached and when downloads go on. IDM's own wording is [unknown] (lab).
pub struct LimitNotice {
    pub(super) lines: [String; 2],
}

impl LimitNotice {
    pub fn new(mb: u32, hours: u32, wait: std::time::Duration) -> Self {
        let at = crate::clock::now()
            + chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::zero());
        LimitNotice {
            lines: [
                crate::i18n::fmt(
                    crate::i18n::t(
                        "The download limit ({mb} MBytes every {hours} hours) has been reached.",
                    ),
                    &[
                        ("mb", &crate::i18n::integer(u64::from(mb))),
                        ("hours", &crate::i18n::integer(u64::from(hours))),
                    ],
                ),
                crate::i18n::fmt(
                    crate::i18n::t("Downloads stopped and will resume automatically at {time}."),
                    &[(
                        "time",
                        &at.format(if crate::i18n::code() == "en" {
                            "%-I:%M %p"
                        } else {
                            "%H:%M"
                        })
                        .to_string(),
                    )],
                ),
            ],
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        window(
            ctx,
            "limit-notice",
            "Download limits",
            [420.0, 90.0],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                for (i, line) in self.lines.iter().enumerate() {
                    winui::fit(
                        ui,
                        &p,
                        o.x + 11.0,
                        o.y + 21.0 + 13.0 * i as f32,
                        line,
                        winui::LIGHT_TEXT(),
                        o.x + 409.0,
                        ("notice-line", i),
                    );
                }
                let ok = winui::button_light(
                    ui,
                    r(o, 173.0, 57.0, 73.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let keys = ui
                    .input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
                if ok || keys {
                    Outcome::Closed
                } else {
                    Outcome::Open
                }
            },
        )
    }
}
