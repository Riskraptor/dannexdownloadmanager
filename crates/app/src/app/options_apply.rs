//! Options → OK and what follows from the settings: the changes acted on, the events' sounds and
//! the virus checks of finished downloads.

use super::*;

/// Options → Sounds' events, in their order there (`Settings::sounds`), as the log names them.
pub(super) const SOUND_EVENTS: [&str; 4] = [
    "download complete",
    "download failed",
    "queue started",
    "queue stopped",
];

impl App {
    /// Options → OK: keep the new settings and act on what changed.
    pub(super) fn apply_options(&mut self, new: Settings) {
        new.publish_categories();
        // Portable mode writes nothing outside its folder (the option is off there).
        if new.launch_on_startup != self.settings.launch_on_startup
            && mdm_ipc::portable_dir().is_none()
        {
            #[cfg(windows)]
            crate::win32::set_run_at_startup(new.launch_on_startup);
            #[cfg(unix)]
            crate::unix::set_run_at_startup(new.launch_on_startup);
        }
        self.settings = new;
        crate::regional::set(&self.settings.regional);
        // Options → Remote: listening started, stopped or moved.
        if let Some(r) = &self.remote {
            r.apply(crate::remote::config(&self.settings));
        }
        if self.settings.proxy_mode == 2 {
            crate::pacfetch::refresh(&self.settings.proxy_script, &self.rt);
        }
        self.apply_speed_limit();
        self.dirty = true;
    }

    /// Options → Sounds: play the sound for event `i` (complete, failed, queue started,
    /// queue stopped/finished) when it is on.
    pub(super) fn sound(&self, i: usize) {
        event_sound(&self.settings, i);
    }

    /// Options → Downloads → Virus checking settings: finished download `id`'s file, when it is
    /// one of the types listed, goes to the antivirus program beside the window (viruscheck.rs);
    /// its answer comes in `poll_virus_checks`. A torrent's or a gallery's folder isn't a file.
    pub(super) fn virus_check(&mut self, id: u64) {
        let Some(i) = self.items.iter().find(|i| i.id == id) else {
            return;
        };
        let name = i
            .dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if i.status != Status::Complete
            || i.torrent.is_some()
            || i.gallery.is_some()
            || !crate::viruscheck::applies(&self.settings, &name)
        {
            return;
        }
        crate::viruscheck::start(
            id,
            i.dest.clone(),
            self.settings.virus_program.clone(),
            self.settings.virus_args.clone(),
            self.virus.0.clone(),
            self.ui_ctx.clone(),
        );
    }

    /// The virus checks' answers: in the download's log, and a problem in its Download complete
    /// window, or in a message when that window isn't open.
    pub(super) fn poll_virus_checks(&mut self) {
        while let Ok(done) = self.virus.1.try_recv() {
            let text = crate::viruscheck::text(&done.verdict, &done.program);
            let Some(item) = self.item(done.id) else {
                continue;
            };
            item.note(text.clone());
            let name = item.file_name.clone();
            self.dirty = true;
            if done.verdict == crate::viruscheck::Verdict::Clean {
                continue;
            }
            match self.complete.iter_mut().find(|c| c.id == done.id) {
                Some(c) => c.notices.push(text),
                // i18n: title of the message when the antivirus program reported a problem with a finished download
                None => dialogs::tell(crate::i18n::mark("Virus check"), format!("{name}\n{text}")),
            }
        }
    }
}

/// Options → Sounds: the sound for event `i` (`SOUND_EVENTS`), when it is ticked and has a file.
/// Logged (an off-screen copy plays none: its checks read the line).
pub(super) fn event_sound(settings: &crate::settings::Settings, i: usize) {
    if let Some((true, file)) = settings.sounds.get(i)
        && !file.is_empty()
    {
        tracing::info!("sound: {}", SOUND_EVENTS.get(i).unwrap_or(&"?"));
        crate::options::play_sound(file);
    }
}
