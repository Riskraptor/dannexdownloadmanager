//! The mouse wheel: rows per notch, smooth travel and its glide, and a wheel a control takes.

use super::*;

/// How long a wheel's glide takes to cover about two thirds of its way (seconds): it is
/// there, to the pixel, in about a sixth of a second, as Windows' own smooth scrolling.
pub(super) const GLIDE_TAU: f32 = 0.045;

/// A list's smooth wheel (beyond IDM, whose lists jump a notch's rows at once): the notches
/// add up to a target, and the list's position (px) moves to it over a few frames. Anything
/// else that moves the list (the keys, Find, a scroll bar, a selection box) ends the glide:
/// the list is where that put it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelGlide {
    pub(super) target: Option<f32>,
    /// Where the glide left the list last frame (moved since: by something else).
    pub(super) left_at: f32,
}

impl WheelGlide {
    /// This frame's step: `wheel` px more to go (from [`wheel_travel`]'s glide part), the list
    /// at `pos` (px, kept in 0..=`max`). Frames are asked for while it moves.
    pub fn step(&mut self, ctx: &egui::Context, pos: &mut f32, wheel: f32, max: f32) {
        if *pos != self.left_at {
            self.target = None;
        }
        if wheel != 0.0 {
            let to = self.target.unwrap_or(*pos) + wheel;
            self.target = Some(to.clamp(0.0, max.max(0.0)));
        }
        if let Some(to) = self.target {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            *pos = glide_toward(*pos, to, dt);
            if *pos == to {
                self.target = None;
            } else {
                ctx.request_repaint();
            }
        }
        *pos = pos.clamp(0.0, max.max(0.0));
        self.left_at = *pos;
    }

    /// The list was put at `pos` by something else this frame: no glide from here.
    pub fn stop(&mut self, pos: f32) {
        self.target = None;
        self.left_at = pos;
    }
}

/// The wheel's travel this frame in rows of `row_h` points, down positive (the caller checks
/// the pointer is over it): a notch moves the system's "lines per notch" rows, a page setting
/// `page` rows, a trackpad its own travel. A Ctrl wheel (zoom) or a Shift wheel (sideways)
/// isn't counted.
pub fn wheel_rows(ui: &Ui, row_h: f32, page: f32) -> f32 {
    let (glide, direct) = wheel_travel(ui, row_h, page);
    glide + direct
}

/// [`wheel_rows`] in two parts: the travel of a mouse wheel's notches and pages (lines; on
/// Windows a precision touchpad's too, in parts of a notch), which a list glides through
/// ([`WheelGlide`]), and a touchpad's travel in points, already as smooth as the fingers.
/// Taken: a window scrolled as a whole (scale::pane) leaves this turn to the list under the
/// pointer ([`wheel_taken`]).
pub fn wheel_travel(ui: &Ui, row_h: f32, page: f32) -> (f32, f32) {
    #[cfg(windows)]
    let lines = crate::win32::wheel_lines();
    #[cfg(unix)]
    let lines = crate::unix::wheel_lines();
    let notch = lines.unwrap_or(page);
    let (glide, direct) = ui.input(|i| {
        i.events.iter().fold((0.0, 0.0), |(g, d), e| match e {
            egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                source,
                ..
            } if !(modifiers.ctrl || modifiers.command || modifiers.shift) => {
                let rows = match unit {
                    egui::MouseWheelUnit::Line => -delta.y * notch,
                    egui::MouseWheelUnit::Page => -delta.y * page,
                    egui::MouseWheelUnit::Point => -delta.y / row_h,
                };
                // Fingers on a touchpad, and the system's momentum after them, are followed as
                // they come; a wheel's notches (or a source not said) glide.
                let fingers = matches!(
                    source,
                    egui::MouseWheelSource::Trackpad | egui::MouseWheelSource::Momentum
                );
                if fingers || *unit == egui::MouseWheelUnit::Point {
                    (g, d + rows)
                } else {
                    (g + rows, d)
                }
            }
            _ => (g, d),
        })
    });
    if glide != 0.0 || direct != 0.0 {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("wheel-taken"), pass));
    }
    (glide, direct)
}

/// Where a glide from `pos` towards `target` is after `dt` seconds: most of the way at first,
/// slowing as it nears (as a flick of the wheel feels), never past it.
pub fn glide_toward(pos: f32, target: f32, dt: f32) -> f32 {
    let k = 1.0 - (-dt.max(0.0) / GLIDE_TAU).exp();
    let next = pos + (target - pos) * k;
    if (target - next).abs() < 0.5 {
        target
    } else {
        next
    }
}

/// Whether a list or a box under the pointer took this frame's wheel ([`wheel_rows`]).
pub fn wheel_taken(ui: &Ui) -> bool {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx()
        .data(|d| d.get_temp::<u64>(egui::Id::unique("wheel-taken")))
        == Some(pass)
}
