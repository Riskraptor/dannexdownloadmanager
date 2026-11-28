//! The dialogs' fonts: Tahoma and its bold face (or the interface font standing in for them),
//! Segoe UI, installed into egui's font definitions.

use super::*;

/// Whether the "tahoma-bold" family is a bold face (Windows' Tahoma Bold), not the interface
/// font standing in for it (macOS, Linux).
pub(super) static BOLD_FACE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Tahoma −11 ("MS Shell Dlg 2", 8 pt): every dialog body text.
pub fn tahoma() -> FontId {
    FontId::new(
        crate::theme::metrics().font_dialog,
        FontFamily::Name("tahoma".into()),
    )
}

/// Tahoma Bold −13: the Options pages' header titles.
pub fn tahoma_bold() -> FontId {
    FontId::new(
        crate::theme::metrics().font_dialog_bold,
        FontFamily::Name("tahoma-bold".into()),
    )
}

/// Tahoma Bold −16: a window's headline (the browser extension's, the helpers').
pub fn tahoma_title() -> FontId {
    FontId::new(
        crate::theme::metrics().font_title,
        FontFamily::Name("tahoma-bold".into()),
    )
}

/// Tahoma (IDM's dialogs) and Tahoma Bold from Windows' fonts folder. A family
/// whose file isn't there (macOS, Linux) is the interface font's, so drawing with it never fails.
pub fn install_fonts(defs: &mut egui::FontDefinitions, font_dir: Option<&std::path::Path>) {
    let ui = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (family, file) in [("tahoma", "tahoma.ttf"), ("tahoma-bold", "tahomabd.ttf")] {
        let fonts = match font_dir.and_then(|d| std::fs::read(d.join(file)).ok()) {
            Some(bytes) => {
                defs.font_data.insert(
                    family.into(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                vec![family.to_owned()]
            }
            _ => ui.clone(),
        };
        if family == "tahoma-bold" {
            BOLD_FACE.store(
                fonts.first().is_some_and(|f| f == family),
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        defs.families.insert(FontFamily::Name(family.into()), fonts);
    }
}

/// Text drawn in [`tahoma_bold`] is bold: its file was there ([`install_fonts`]). Where it isn't,
/// a text that must read bold is drawn twice, a pixel apart (the download list's names).
pub fn bold_face() -> bool {
    BOLD_FACE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Segoe UI −12: menu bar, toolbar labels.
pub fn segoe() -> FontId {
    FontId::proportional(crate::theme::metrics().font_ui)
}
