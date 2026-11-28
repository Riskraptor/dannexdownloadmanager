//! Turning the computer off when downloads are done: the countdown, and what is told when it
//! fails.

use super::*;

// ---- Turn off computer countdown (light) ---------------------------------------------------

/// After a queue with "Turn off computer when done": a 30 s countdown that can be cancelled
/// (or skipped) before the computer shuts down, sleeps or hibernates. IDM's own prompt is
/// [unknown] (lab); turning a computer off unannounced is not acceptable.
pub struct TurnOffCountdown {
    pub(super) mode: crate::queue::TurnOff,
    pub(super) force: bool,
    pub(super) deadline: std::time::Instant,
}

impl TurnOffCountdown {
    pub const SECONDS: u64 = 30;

    pub fn new(mode: crate::queue::TurnOff, force: bool) -> Self {
        TurnOffCountdown {
            mode,
            force,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(Self::SECONDS),
        }
    }

    /// `Done` when the time is up or "Turn off now" was pressed; `Closed` on Cancel.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        use crate::queue::TurnOff;
        let left = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        // Whole sentences: the verb's form and place differ between languages.
        let sentence = match self.mode {
            TurnOff::ShutDown => crate::i18n::t("The computer will shut down in {n} seconds."),
            TurnOff::Sleep => crate::i18n::t("The computer will go to sleep in {n} seconds."),
            TurnOff::Hibernate => crate::i18n::t("The computer will hibernate in {n} seconds."),
        };
        question(
            ctx,
            "turn-off",
            "Dannex Download Manager",
            [374.0, 100.0],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let black = winui::LIGHT_TEXT();
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 21.0,
                    crate::i18n::t("The download queue is done."),
                    black,
                );
                let line = crate::i18n::fmt(
                    sentence,
                    &[(
                        "n",
                        &crate::i18n::integer(left.as_secs() + u64::from(left.subsec_millis() > 0)),
                    )],
                );
                winui::text(&p, o.x + 11.0, o.y + 34.0, &line, black);
                // As wide as their labels in this language (#192): a longer one pushes Cancel
                // right.
                let [now_at, cancel_at] = winui::button_row_from_left(
                    &p,
                    &[("Turn off now", 94.0, 90.0), ("Cancel", 194.0, 73.0)],
                    10.0,
                    363.0,
                    200.0,
                )[..] else {
                    unreachable!("two buttons")
                };
                let now = winui::button_light(
                    ui,
                    r(o, now_at.0, 63.0, now_at.1, 21.0),
                    "Turn off now",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, cancel_at.0, 63.0, cancel_at.1, 21.0),
                    "Cancel",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                // Enter: Cancel, the default button.
                if cancel || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
                    Outcome::Closed
                } else if now || left.is_zero() {
                    Outcome::Done(())
                } else {
                    Outcome::Open
                }
            },
        )
    }

    /// Shut down, sleep or hibernate now.
    pub fn execute(&self) {
        use crate::queue::TurnOff;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new("shutdown");
            match self.mode {
                TurnOff::ShutDown => cmd.args(["/s", "/t", "0"]),
                TurnOff::Hibernate => cmd.arg("/h"),
                TurnOff::Sleep => {
                    if !crate::win32::sleep(self.force) {
                        // i18n: why the computer could not be put to sleep (fills {error})
                        tell_turn_off_failed(crate::i18n::t("Windows refused").to_owned());
                    }
                    return;
                }
            };
            // "Force processes to terminate": don't wait for programs that hold it up.
            if self.force {
                cmd.arg("/f");
            }
            // No console window flashing up (CREATE_NO_WINDOW).
            cmd.creation_flags(0x0800_0000);
            run_or_tell(cmd);
        }
        // Linux: systemd, as the desktop's own menu does; forcing ignores the programs that ask
        // to delay it (inhibitors).
        #[cfg(target_os = "linux")]
        {
            let verb = match self.mode {
                TurnOff::ShutDown => "poweroff",
                TurnOff::Hibernate => "hibernate",
                TurnOff::Sleep => "suspend",
            };
            let mut cmd = std::process::Command::new("systemctl");
            cmd.arg(verb);
            if self.force {
                cmd.arg("--ignore-inhibitors");
            }
            run_or_tell(cmd);
        }
        // macOS: through System Events, as the Apple menu does (macOS has no separate
        // hibernate: its sleep writes memory to disk as set up); apps can still object.
        #[cfg(target_os = "macos")]
        {
            let script = match self.mode {
                TurnOff::ShutDown => "tell application \"System Events\" to shut down",
                TurnOff::Hibernate | TurnOff::Sleep => {
                    "tell application \"System Events\" to sleep"
                }
            };
            let mut cmd = std::process::Command::new("osascript");
            cmd.args(["-e", script]);
            run_or_tell(cmd);
            let _ = self.force;
        }
    }
}

/// Run a shut down / sleep command and say when it fails (hibernation off, a program
/// holding it up): it did nothing, silently.
pub(super) fn run_or_tell(mut cmd: std::process::Command) {
    std::thread::spawn(move || match cmd.output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            tell_turn_off_failed(if why.is_empty() {
                out.status.to_string()
            } else {
                why
            });
        }
        Err(e) => tell_turn_off_failed(e.to_string()),
    });
}

pub(super) fn tell_turn_off_failed(why: String) {
    tell(
        // i18n: title of the message when shutting down / sleep / hibernate failed
        crate::i18n::mark("Turn off computer"),
        crate::i18n::fmt(
            crate::i18n::t("The computer could not be turned off: {error}"),
            &[("error", &why)],
        ),
    );
}
