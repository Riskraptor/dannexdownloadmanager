use super::*;

/// Every connection is numbered, whole, however many there are (up to the 128 allowed): at 50
/// connections, rows 10 and on read "..." (#1). Drawn as the table draws them, through a
/// painter clipped to the table's body, at the interface sizes offered.
#[test]
fn every_connection_number_is_drawn_whole() {
    for zoom in [1.0f32, 1.25, 1.5, 2.0] {
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        winui::install_fonts(&mut defs, None);
        ctx.set_fonts(defs);
        ctx.set_zoom_factor(zoom);
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let o = egui::pos2(0.0, 0.0);
            let body = r(o, 19.0, 299.0, 492.0, 100.0);
            let clip = ui.painter().with_clip_rect(body);
            for n in [1u32, 8, 16, 32, 50, 128] {
                let widest = winui::text_width(&clip, &n.to_string());
                let right = conn_number_right(o.x, widest);
                // The numbers stay left of the column's line.
                assert!(right < o.x + 47.0, "{n} connections: ends at {right}");
                for id in 0..n {
                    assert!(
                        !conn_number(&clip, o.x, right, 311.0, id),
                        "connection {} of {n} cut at zoom {zoom}",
                        id + 1
                    );
                }
            }
            // IDM's place while the numbers fit there.
            let one = winui::text_width(&clip, "9");
            assert_eq!(conn_number_right(o.x, one), o.x + 33.0);
            // What the table drew before: right-aligned at 33 within the clip's 6 px
            // margin, "9" whole and "10" cut to "..." (the owner's 50 connections).
            let before = |s: &str| winui::text_right(&clip, o.x + 33.0, 311.0, s, winui::TEXT()).1;
            assert!(
                !before("9") && before("10"),
                "the old drawing at zoom {zoom}"
            );
        });
        out.textures_delta.clear();
    }
}

/// A download of unknown size has no percentage in its title (from #384).
#[test]
fn the_title_has_a_percentage_only_with_a_size() {
    assert!(!window_title(None, "clip.mp4").contains('%'));
    assert!(window_title(Some(0.445), "clip.mp4").starts_with("44% "));
}

/// The start view (Options → Downloads → Customize): hidden only where a tray icon can bring
/// the window back, minimized otherwise; never out of reach.
#[test]
fn a_progress_window_opens_as_its_start_view_says() {
    use ProgressView::*;
    assert_eq!(opens(Normal, true), Opens::Shown);
    assert_eq!(opens(Normal, false), Opens::Shown);
    assert_eq!(opens(Minimized, true), Opens::Minimized);
    assert_eq!(opens(Hidden, true), Opens::Hidden);
    assert_eq!(
        opens(Hidden, false),
        Opens::Minimized,
        "no tray to come back from"
    );
    let mut p = Progress::new(1, true);
    p.open_as(Opens::Hidden);
    assert!(p.hidden && !p.start_minimized);
    p.open_as(Opens::Minimized);
    assert!(!p.hidden && p.start_minimized);
    p.open_as(Opens::Shown);
    assert!(!p.hidden && !p.start_minimized);
    // Saved by name; a list from before has none and opens them normally.
    assert_eq!(serde_json::to_string(&Hidden).unwrap(), "\"hidden\"");
    assert_eq!(crate::settings::Settings::default().progress_view, Normal);
}

/// The × (and the system's close) stops a download under way, as IDM's; it only closes the
/// window of one that isn't running, or of a finished torrent still sharing.
#[test]
fn the_x_stops_a_running_download_and_closes_the_others() {
    let url = url::Url::parse("https://files.test/a.zip").unwrap();
    let mut it = Item::new(1, url, "a.zip".into(), crate::model::Category::Compressed);
    it.status = Status::Stopped;
    assert!(matches!(closing_action(&it), Action::Close), "stopped");
    it.status = Status::Complete;
    assert!(matches!(closing_action(&it), Action::Close), "finished");
    it.status = Status::Error;
    assert!(matches!(closing_action(&it), Action::Close), "failed");
}

#[test]
fn hide_tab_takes_the_tab_off_the_strip() {
    let mut p = Progress::new(1, true);
    p.tab = Tab::Limiter;
    p.hide(Tab::Limiter);
    assert_eq!(p.tab, Tab::Status, "back on Download status");
    assert_eq!(p.hidden_tabs, [Tab::Limiter], "Speed Limiter off the strip");
    p.hide(Tab::Limiter);
    p.hide(Tab::Status);
    assert_eq!(
        p.hidden_tabs,
        [Tab::Limiter],
        "once each, never Download status"
    );
    p.hide(Tab::Completion);
    assert_eq!(p.hidden_tabs, [Tab::Limiter, Tab::Completion]);
    p.show_tab("limit");
    assert_eq!(p.tab, Tab::Limiter, "asked for, it comes back");
    assert_eq!(p.hidden_tabs, [Tab::Completion], "the other stays hidden");
}

#[test]
fn show_hidden_tabs_brings_them_all_back() {
    let mut p = Progress::new(1, true);
    p.hide(Tab::Limiter);
    p.hide(Tab::Completion);
    p.show_hidden_tabs();
    assert!(p.hidden_tabs.is_empty(), "every tab back on the strip");
    assert_eq!(p.tab, Tab::Status, "the page shown stays as it was");
}

#[test]
fn syncs_saved_rename_autosaves_and_ignores_unconfirmed_typing() {
    let mut item = Item::new(
        7,
        "https://example.com/unknown".parse().unwrap(),
        std::path::PathBuf::from("/tmp/unknown"),
        crate::model::Category::General,
    );
    item.rename_on_complete = Some("ArchivePack".into());

    let mut p = Progress::new(7, true);
    p.sync_rename_from_item(&item);
    assert_eq!(p.rename_text, "ArchivePack");
    assert!(p.rename_show_saved, "saved rename shows Saved");

    let t0 = std::time::Instant::now();
    p.rename_text = "Episode 02".into();
    p.on_rename_text_changed(t0);
    assert!(!p.rename_show_saved, "editing hides Saved until confirmed");
    assert_eq!(
        p.take_confirmed_rename(),
        None,
        "half-typed unconfirmed text before 3s is never promoted"
    );
    assert!(
        p.poll_rename_autosave(t0 + std::time::Duration::from_secs(2))
            .is_some(),
        "still waiting before 3 seconds"
    );
    assert!(
        p.poll_rename_autosave(t0 + std::time::Duration::from_secs(3))
            .is_none(),
        "3 seconds elapsed: auto-saves"
    );
    assert!(p.rename_show_saved, "Saved turns green after 3s idle");
    assert_eq!(p.take_confirmed_rename(), Some(Some("Episode 02".into())));

    p.rename_text = "bad/name".into();
    p.on_rename_text_changed(t0);
    assert_eq!(
        p.poll_rename_autosave(t0 + std::time::Duration::from_secs(5)),
        None
    );
    assert_eq!(p.take_confirmed_rename(), None);
}

#[test]
fn switching_tabs_keeps_window_height_constant() {
    let mut p = Progress::new(1, true);
    assert_eq!(p.status_drop(), RENAME_DROP);
    p.tab = Tab::Limiter;
    assert_eq!(p.status_drop(), RENAME_DROP);
    p.tab = Tab::Completion;
    assert_eq!(p.status_drop(), RENAME_DROP);
}
