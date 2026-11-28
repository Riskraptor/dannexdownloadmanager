//! The system's file and folder choosers, opened beside the window and answered later.

/// A system file or folder dialog's answer, from its thread.
pub(super) type PickSlot = std::sync::Arc<std::sync::Mutex<Option<Option<std::path::PathBuf>>>>;

/// Show a system Open / Save / folder dialog (`ask` builds and runs it) on a thread of its
/// own: it blocked the app's windows while open (progress windows stood still). One per
/// `key` at a time; [`picked`] gives its answer on a later frame.
pub fn start_pick(
    ctx: &egui::Context,
    key: &str,
    ask: impl FnOnce() -> Option<std::path::PathBuf> + Send + 'static,
) {
    let id = egui::Id::unique(("pick", key));
    if ctx.data(|d| d.get_temp::<PickSlot>(id)).is_some() {
        return;
    }
    let slot: PickSlot = Default::default();
    ctx.data_mut(|d| d.insert_temp(id, slot.clone()));
    let wake = ctx.clone();
    std::thread::spawn(move || {
        let answer = ask();
        *slot.lock().unwrap() = Some(answer);
        wake.request_repaint();
    });
}

/// Whether [`start_pick`]'s dialog `key` is still waiting for its answer to be taken: open, or
/// answered and not yet read by [`picked`].
pub fn picking(ctx: &egui::Context, key: &str) -> bool {
    let id = egui::Id::unique(("pick", key));
    ctx.data(|d| d.get_temp::<PickSlot>(id)).is_some()
}

/// The path chosen in [`start_pick`]'s dialog `key`, once (None while it is open, or when it
/// was cancelled).
pub fn picked(ctx: &egui::Context, key: &str) -> Option<std::path::PathBuf> {
    let id = egui::Id::unique(("pick", key));
    let slot = ctx.data(|d| d.get_temp::<PickSlot>(id))?;
    let answer = slot.lock().unwrap().take()?;
    ctx.data_mut(|d| d.remove::<PickSlot>(id));
    answer
}
