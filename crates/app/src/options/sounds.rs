//! Options → Sounds: a sound for each event, played to try it.

use super::*;

pub(super) const SOUND_EVENTS: [&str; 4] = [
    crate::i18n::mark("Download complete"),
    crate::i18n::mark("Download failed"),
    // i18n: Options → Sounds: an event a sound can be played for: a queue starts
    crate::i18n::mark("A queue starts"),
    // i18n: Options → Sounds: an event a sound can be played for: a queue is stopped, or has nothing left to download
    crate::i18n::mark("A queue stops or is done"),
];

impl OptionsDialog {
    pub(super) fn sounds(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        header(ui, o, Icon::PageSounds, "Sound settings");
        group(
            ui,
            o,
            16.0,
            102.0,
            392.0,
            299.0,
            // i18n: Options → Sounds: group box over the events and the sound each plays
            "Sounds Dannex Download Manager plays",
        );
        let list_on_screen = r(o, 33.0, 112.0, 366.0, 250.0);
        list_frame(&p, list_on_screen);
        let list = winui::rect(list_on_screen);
        let head_y = list.top() + 1.0;
        // i18n: column title: the event a sound plays for
        text(
            &p,
            list.left() + 8.0,
            head_y + 16.0,
            crate::i18n::t("Event"),
            LIGHT_TEXT(),
        );
        fill(
            &p,
            o.x + 224.0,
            head_y + 1.0,
            o.x + 224.0,
            head_y + 23.0,
            winui::HEADER_LINE(),
        );
        text(
            &p,
            o.x + 231.0,
            head_y + 16.0,
            crate::i18n::t("Sound file"),
            LIGHT_TEXT(),
        );
        // The events: the shared row list under the header (its frame on the list's). Space
        // ticks one, a double-click or Enter chooses its file, Delete clears it.
        let body = Rect::from_min_max(
            pos2(list.left() + 1.0, head_y + 25.0),
            pos2(list.right() - 1.0, list.bottom() - 1.0),
        );
        let grid = winui::GRID();
        let sounds = &mut self.s.sounds;
        self.sound_rows.name = crate::i18n::t("Sounds");
        let ev = winui::row_list(
            ui,
            "sounds",
            winui::rect(body.expand(1.0)),
            SOUND_EVENTS.len(),
            17.0,
            &mut self.sound_rows,
            |ui, clip, i, row, look| {
                let top = row.top();
                fill(clip, row.left(), top + 16.0, row.right(), top + 16.0, grid);
                fill(clip, o.x + 225.0, top, o.x + 225.0, top + 16.0, grid);
                let (on, file) = &mut sounds[i];
                let event = crate::i18n::t(SOUND_EVENTS[i]);
                look.name = event.to_owned();
                winui::row_check(
                    ui,
                    clip,
                    pos2(o.x + 39.0, top + 2.0),
                    on,
                    event,
                    ("sound", i),
                );
                winui::fit(
                    ui,
                    clip,
                    o.x + 57.0,
                    top + 12.0,
                    event,
                    look.ink,
                    o.x + 221.0,
                    ("sound-event", i),
                );
                let name = std::path::Path::new(file.as_str())
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let drawn = winui::fit(
                    ui,
                    clip,
                    o.x + 231.0,
                    top + 12.0,
                    &name,
                    look.ink,
                    row.right() - 4.0,
                    ("sound-file", i),
                );
                // The whole path on hover (the name alone says little).
                if !file.is_empty() {
                    winui::copyable(ui, drawn, ("sound-path", i), file, true);
                }
            },
        );
        if let Some(i) = ev.space {
            sounds[i].0 = !sounds[i].0;
        }
        if ev.focused {
            self.sound_rows
                .type_to(ui, "sounds", SOUND_EVENTS.len(), |i| {
                    crate::i18n::t(SOUND_EVENTS[i]).to_owned()
                });
        }
        if let Some(i) = ev.delete {
            sounds[i] = (false, String::new());
        }
        let pick_file = ev.double_clicked.or(ev.entered);
        if pick_file.is_some() {
            winui::take_key(ui, egui::Key::Enter);
        }
        let sel = self.sound_rows.selected;
        let kind = if sel.is_some() {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        // Both as wide as their labels in this language (#192), each around its English centre.
        let browse_at = winui::button_around(&p, "Browse...", 106.0, 73.0, 21.0, 233.0);
        let play_at = winui::button_around(
            &p,
            "Play",
            241.0,
            73.0,
            browse_at.0 + browse_at.1 + 8.0,
            404.0,
        );
        let browse = winui::button_light(
            ui,
            r(o, browse_at.0, 371.0, browse_at.1, 21.0),
            "Browse...",
            kind,
            false,
        )
        .clicked();
        let has_file = sel.is_some_and(|i| !self.s.sounds[i].1.is_empty());
        let play_kind = if has_file {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        let play =
            // i18n: verb: plays the sound
            winui::button_light(ui, r(o, play_at.0, 371.0, play_at.1, 21.0), "Play", play_kind, false)
                .clicked();
        if let Some(i) = sel {
            if browse || pick_file == Some(i) {
                let title = crate::i18n::t(SOUND_EVENTS[i]).to_owned();
                // i18n: the kind of file the Open dialog lists (sound files)
                let kind = crate::i18n::t("Sounds").to_owned();
                winui::start_pick(ui.ctx(), &format!("sound-{i}"), move || {
                    rfd::FileDialog::new()
                        .set_title(title)
                        .add_filter(kind, &["wav"])
                        .pick_file()
                });
            }
            if let Some(f) = winui::picked(ui.ctx(), &format!("sound-{i}")) {
                self.s.sounds[i] = (true, f.display().to_string());
            }
            if play && has_file {
                play_sound(&self.s.sounds[i].1);
            }
        }
    }
}

pub fn play_sound(file: &str) {
    if crate::offscreen() {
        return;
    }
    #[cfg(windows)]
    crate::win32::play_sound(file);
    #[cfg(unix)]
    crate::unix::play_sound(file);
}
