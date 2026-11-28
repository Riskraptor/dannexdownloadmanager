use super::*;

/// Which way a glyph's bitmap points: the side its middle row is inked on (`<` and `(` on
/// the left, `>` and `)` on the right).
fn points_left(image: &egui::ColorImage, g: &egui::epaint::text::Glyph) -> bool {
    let [x0, y0] = g.uv_rect.min.map(usize::from);
    let [x1, y1] = g.uv_rect.max.map(usize::from);
    let y = usize::midpoint(y0, y1);
    let ink: Vec<f32> = (x0..x1).map(|x| f32::from(image[(x, y)].a())).collect();
    let weighted: f32 = ink.iter().enumerate().map(|(i, a)| i as f32 * a).sum();
    weighted / ink.iter().sum::<f32>() < (ink.len() - 1) as f32 / 2.0
}

/// In a right-to-left line the paired marks are drawn mirrored, so a "< Back" arrow points
/// the way the line reads (to the right in Arabic) and brackets open towards their text.
#[test]
fn paired_marks_mirror_in_right_to_left_text() {
    use egui::epaint::text::{Fonts, LayoutJob, TextDirection, TextOptions};
    let mut fonts = Fonts::new(TextOptions::default(), egui::FontDefinitions::default());
    for (text, direction, left) in [
        ("< Back", TextDirection::LeftToRight, true),
        (
            "<< \u{625}\u{62e}\u{641}\u{627}\u{621}",
            TextDirection::RightToLeft,
            false,
        ),
        (
            "\u{62a}\u{627}\u{644}\u{64a} >",
            TextDirection::RightToLeft,
            true,
        ),
        ("( \u{645}\u{644}\u{641}", TextDirection::RightToLeft, false),
        (
            "\u{ab} \u{645}\u{644}\u{641}",
            TextDirection::RightToLeft,
            false,
        ),
    ] {
        let mut job = LayoutJob::simple(
            text.to_owned(),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
            f32::INFINITY,
        );
        job.direction = direction;
        let galley = fonts.with_pixels_per_point(1.0).layout_job(job);
        let image = fonts.image();
        let mark = galley.rows[0]
            .row
            .glyphs
            .iter()
            .find(|g| "<>()\u{ab}\u{bb}".contains(g.chr))
            .unwrap();
        assert_eq!(points_left(&image, mark), left, "{text:?}");
    }
}

/// The same with Windows' fonts and the classic text's rasterizer (win32::text_rasterizer),
/// as the windows draw: told the interface is right to left, it leaves every paired mark to
/// the shaper, which mirrors it (arrows, brackets, guillemets alike).
#[cfg(windows)]
#[test]
fn the_classic_text_leaves_paired_marks_to_the_shaper() {
    use egui::epaint::text::{Fonts, LayoutJob, TextDirection, TextOptions};
    let dir = std::path::PathBuf::from(
        std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into()),
    )
    .join("Fonts");
    let mut defs = egui::FontDefinitions::default();
    install_fonts(&mut defs, Some(&dir));
    let mut fonts = Fonts::new(TextOptions::default(), defs);
    fonts.set_glyph_rasterizers(
        crate::win32::text_rasterizer(dir, true, false)
            .into_iter()
            .collect(),
    );
    let arabic = "\u{625}\u{62e}\u{641}\u{627}\u{621}";
    for (mark, left) in [('<', false), ('>', true), ('(', false), ('\u{ab}', false)] {
        let text = format!("{mark}{mark} {arabic}");
        let mut job = LayoutJob::simple(text.clone(), tahoma(), Color32::WHITE, f32::INFINITY);
        job.direction = TextDirection::RightToLeft;
        let galley = fonts.with_pixels_per_point(1.25).layout_job(job);
        let image = fonts.image();
        let glyph = galley.rows[0]
            .row
            .glyphs
            .iter()
            .find(|g| g.chr == mark)
            .unwrap();
        assert_eq!(points_left(&image, glyph), left, "{text:?}");
    }
}

/// `r` already gives the screen rect (mirrored in RTL); `rect` around it mirrors it back,
/// so the thing stays at its English place in Arabic (#170: the browser offer window's logo
/// over its subline). No window does that.
#[test]
fn no_rect_is_mirrored_twice() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let twice = ["rect(r(", "rect(winui::r("];
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if twice.iter().any(|t| code.contains(t)) && !code.contains("\"rect(") {
                found.push(format!("{}:{}", path.display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "rect(r(...)) mirrors twice: {found:?}");
}

/// `f` with a painter whose Tahoma is the interface font (as on macOS and Linux).
fn with_painter(f: impl FnOnce(&Painter)) {
    let ctx = egui::Context::default();
    let mut defs = egui::FontDefinitions::default();
    install_fonts(&mut defs, None);
    ctx.set_fonts(defs);
    let mut f = Some(f);
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        if let Some(f) = f.take() {
            f(ui.painter());
        }
    });
    out.textures_delta.clear();
}

#[test]
fn a_row_of_buttons_widens_for_long_labels_only() {
    with_painter(|p| {
        // Short labels keep their English places (Download video's bottom row).
        let row = [("Later", 73.0), ("Cancel", 73.0)];
        assert_eq!(
            buttons_to_right(p, 489.0, 8.0, &row, 160.0),
            vec![(335.0, 73.0), (416.0, 73.0)]
        );
        assert_eq!(
            buttons_from_left(p, 11.0, 8.0, &row, 160.0),
            vec![(11.0, 73.0), (92.0, 73.0)]
        );
        // A long one is as wide as its text and the room inside the border, up to the most
        // allowed; the row keeps its end and its gaps.
        let long = "A label much longer than its English button";
        let want = (text_width(p, long) + 16.0).ceil();
        let row = [(long, 90.0), ("Cancel", 73.0)];
        let placed = buttons_to_right(p, 489.0, 8.0, &row, 400.0);
        assert_eq!(placed[0], (489.0 - 73.0 - 8.0 - want, want));
        assert_eq!(placed[1], (416.0, 73.0));
        let placed = buttons_from_left(p, 11.0, 8.0, &row, 400.0);
        assert_eq!(placed[1].0, 11.0 + want + 8.0);
        assert_eq!(buttons_to_right(p, 489.0, 8.0, &row, 120.0)[0].1, 120.0);
        // Its label then fits whole (no "...").
        let rc = Rect::from_min_size(pos2(0.0, 0.0), vec2(want, 21.0));
        assert!(!fit_label(p, long, rc).1);
    });
}

#[test]
fn a_row_keeps_its_english_places_and_uneven_gaps() {
    // Download File Info's row: gaps of 19 and 18 px, ending at 461.
    let row = [
        (97.0, 109.0, 90.0),
        (225.0, 109.0, 95.0),
        (352.0, 109.0, 50.0),
    ];
    assert_eq!(
        place_row(&row, 19.0, 11.0, 200.0),
        vec![(97.0, 109.0), (225.0, 109.0), (352.0, 109.0)]
    );
    // The middle one grows: it widens to the left and pushes the first, the gaps kept.
    let row = [
        (97.0, 109.0, 90.0),
        (225.0, 109.0, 150.0),
        (352.0, 109.0, 50.0),
    ];
    assert_eq!(
        place_row(&row, 19.0, 11.0, 200.0),
        vec![(56.0, 109.0), (184.0, 150.0), (352.0, 109.0)]
    );
    // A button with room before it grows into it without moving the others.
    let row = [
        (30.0, 138.0, 100.0),
        (311.0, 82.0, 120.0),
        (422.0, 73.0, 60.0),
    ];
    assert_eq!(
        place_row(&row, 8.0, 30.0, 200.0),
        vec![(30.0, 138.0), (273.0, 120.0), (422.0, 73.0)]
    );
    // The last one grows into the room before it, then pushes, `gap` apart.
    let row = [(311.0, 82.0, 60.0), (422.0, 73.0, 120.0)];
    assert_eq!(
        place_row(&row, 8.0, 0.0, 200.0),
        vec![(285.0, 82.0), (375.0, 120.0)]
    );
    // At most `max`.
    let row = [(311.0, 82.0, 60.0), (422.0, 73.0, 300.0)];
    assert_eq!(place_row(&row, 8.0, 0.0, 150.0)[1], (345.0, 150.0));
    // The Scheduler's row: the first, at the row's start, grows; those it reaches move right
    // as little as they must, the rest keep their places.
    let english = [21.0, 102.0, 216.0, 297.0, 390.0, 484.0, 565.0];
    let row: Vec<(f32, f32, f32)> = english
        .iter()
        .enumerate()
        .map(|(i, &x)| (x, 73.0, if i == 0 { 116.0 } else { 60.0 }))
        .collect();
    let xs: Vec<f32> = place_row(&row, 8.0, 21.0, 200.0)
        .iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(xs, [21.0, 145.0, 226.0, 307.0, 390.0, 484.0, 565.0]);
}

#[test]
fn a_full_row_takes_room_from_buttons_wider_than_their_label() {
    // The crash report window's row: 588 px from 16 to 604, gaps of 6, no room to spare.
    let english = [
        (16.0, 170.0),
        (192.0, 110.0),
        (308.0, 110.0),
        (424.0, 112.0),
        (542.0, 62.0),
    ];
    let with = |fits: [f32; 5]| -> Vec<(f32, f32, f32)> {
        english
            .iter()
            .zip(fits)
            .map(|(&(x, w), f)| (x, w, f))
            .collect()
    };
    // English: as it is.
    let row = with([100.0, 80.0, 75.0, 110.0, 50.0]);
    assert_eq!(place_row(&row, 6.0, 16.0, 260.0), english.to_vec());
    // The fourth needs 166: the room is taken from those with the most to spare (the first,
    // then the third); the row keeps both its ends and its gaps.
    let row = with([110.0, 100.0, 95.0, 166.0, 50.0]);
    let placed = place_row(&row, 6.0, 16.0, 260.0);
    assert_eq!(placed[0], (16.0, 121.0));
    assert_eq!(placed[1].1, 110.0);
    assert_eq!(placed[2].1, 105.0);
    assert_eq!(placed[3].1, 166.0);
    assert_eq!(placed[4], (542.0, 62.0));
    for w in placed.windows(2) {
        assert_eq!(w[1].0 - (w[0].0 + w[0].1), 6.0);
    }
    // Not even then: no button goes under 73 px or its label, and the widest widened ones
    // are narrowed (back to their English width at most).
    let row = with([200.0, 150.0, 150.0, 250.0, 50.0]);
    let placed = place_row(&row, 6.0, 16.0, 260.0);
    let total: f32 = placed.iter().map(|p| p.1).sum::<f32>() + 4.0 * 6.0;
    assert_eq!(placed[0].0, 16.0);
    assert_eq!(total, 588.0);
    assert!(placed.iter().zip(&english).all(|(p, e)| p.1 >= e.1));
    assert!(
        placed
            .iter()
            .zip(&row)
            .all(|(p, r)| p.1 < r.2.max(r.1 + 1.0))
    );
}

#[test]
fn a_label_that_fits_keeps_its_button_as_it_is() {
    with_painter(|p| {
        // Whole with less than the usual room around it (Download a gallery's English
        // Install button): not widened, alone or in a row.
        let label = "Install gallery-dl";
        let text = text_width(p, label);
        let tight = (text + 6.0).ceil();
        assert_eq!(button_fit_width(p, label, tight, 300.0), tight);
        assert_eq!(
            button_row(p, &[(label, 100.0, tight)], 8.0, 0.0, 300.0),
            vec![(100.0, tight)]
        );
        // Cut: as wide as the label and the usual room.
        let cut = (text + 2.0).floor();
        assert_eq!(button_fit_width(p, label, cut, 300.0), (text + 16.0).ceil());
    });
}

#[test]
fn a_title_takes_a_second_line_only_where_one_is_too_long() {
    with_painter(|p| {
        let font = tahoma_bold();
        let s = "Proxy socks configuration page";
        let whole = text_width_font(p, font.clone(), s);
        // Room for it: one line, as it is.
        assert_eq!(fit_lines(p, font.clone(), s, whole), vec![s]);
        // Short of room: two lines, broken where the wider is narrowest.
        let two = fit_lines(p, font.clone(), s, whole - 1.0);
        assert_eq!(two, vec!["Proxy socks", "configuration page"]);
        // After a slash or a hyphen, the mark kept on the first line.
        let two = fit_lines(p, font.clone(), "Proxyserver/SOCKS-konfiguration", 10.0);
        assert_eq!(two.concat(), "Proxyserver/SOCKS-konfiguration");
        assert!(two[0].ends_with('/') || two[0].ends_with('-'), "{two:?}");
        // A word with no place to break stays whole.
        assert_eq!(
            fit_lines(p, font.clone(), "Konfiguration", 10.0),
            vec!["Konfiguration"]
        );
        // Chinese breaks between its characters.
        assert_eq!(fit_lines(p, font, "代理服务器配置", 10.0).len(), 2);
    });
}

#[test]
fn a_drop_down_is_as_wide_as_its_widest_choice() {
    with_painter(|p| {
        // Short choices: its English width.
        assert_eq!(
            combo_fit_width(p, &["Sleep", "Hibernate"], 119.0, 300.0),
            119.0
        );
        // A long one: it and the chevron's room, whichever is chosen.
        let long = "A choice much longer than its English box";
        let want = (text_width(p, long) + 24.0).ceil();
        assert_eq!(combo_fit_width(p, &["Sleep", long], 119.0, 300.0), want);
        // No room for it: as wide as there is (it ends in "..."), never below English.
        assert_eq!(combo_fit_width(p, &[long], 119.0, 150.0), 150.0);
        assert_eq!(combo_fit_width(p, &[long], 119.0, 80.0), 119.0);
    });
}

#[test]
fn a_row_from_the_left_widens_to_the_right() {
    with_painter(|p| {
        let long = "A label much longer than its English button";
        let want = (text_width(p, long) + 16.0).ceil();
        // Short labels: their English places.
        let row = [("OK", 11.0, 90.0), ("Cancel", 107.0, 90.0)];
        assert_eq!(
            button_row_from_left(p, &row, 6.0, 400.0, 300.0),
            vec![(11.0, 90.0), (107.0, 90.0)]
        );
        assert_eq!(
            button_row(p, &row, 6.0, 0.0, 300.0),
            vec![(11.0, 90.0), (107.0, 90.0)]
        );
        // A long first one pushes the second right, the gap kept.
        let row = [(long, 11.0, 90.0), ("Cancel", 107.0, 90.0)];
        let placed = button_row_from_left(p, &row, 6.0, 600.0, 400.0);
        assert_eq!(placed, vec![(11.0, want), (11.0 + want + 6.0, 90.0)]);
        // The same as a row ending at the right: the first grows to the left.
        let placed = button_row(p, &row, 6.0, -400.0, 400.0);
        assert_eq!(placed, vec![(101.0 - want, want), (107.0, 90.0)]);
        // Toggling labels: laid out for the widest translation.
        assert_eq!(widest_label(p, &["OK", long]), long);
        // A lone button widens around its centre, inside its room. (Labels from variables:
        // the catalog's extractor takes literal ones as texts to translate.)
        let short = "OK";
        assert_eq!(
            button_around(p, short, 480.0, 73.0, 460.0, 562.0),
            (480.0, 73.0)
        );
        assert_eq!(
            button_around(p, long, 480.0, 73.0, 460.0, 562.0),
            (460.0, 102.0)
        );
        let label = "Preview the part downloaded";
        let w = (text_width(p, label) + 16.0).ceil();
        let (x, got) = button_around(p, label, 480.0, 73.0, 400.0, 600.0);
        assert_eq!(got, w);
        assert!((x + w / 2.0 - 516.5).abs() <= 1.0);
    });
}

#[test]
fn data_keeps_its_own_lines_and_reads_left_to_right() {
    with_painter(|p| {
        // A report's lines (Windows' line ends too) stay lines; only a long one wraps.
        let report = "Kind: freeze\r\nTime: 2026-11-06 10:00:01\n\n  at mdm::app::update\nend";
        assert_eq!(
            wrap_lines(p, report, 1000.0),
            vec![
                "Kind: freeze",
                "Time: 2026-11-06 10:00:01",
                "",
                "  at mdm::app::update",
                "end"
            ]
        );
        let long = "one two three four five six seven eight nine ten";
        let lines = wrap_lines(p, long, text_width(p, "one two three") + 1.0);
        assert_eq!(lines[0], "one two three");
        assert!(
            lines
                .iter()
                .all(|l| text_width(p, l) <= text_width(p, "one two three") + 1.0)
        );
        // Drawn: as many lines (a translation's paragraph is flowed again, data never).
        assert_eq!(
            data_paragraph(p, 24.0, 100.0, report, TEXT(), 400.0, 14.0),
            5
        );
    });
}

#[test]
fn data_starts_at_the_left_of_its_room_in_rtl_too() {
    assert_eq!(data_left(24.0, 400.0), 24.0);
    // English 24..400 in a 600 px window, mirrored: on screen 200..576, read from 200.
    let _s = Surface::enter_if(true, 0.0, 600.0);
    assert_eq!(data_left(24.0, 400.0), 200.0);
}

#[test]
fn a_widened_button_grows_into_the_room_before_it_in_rtl_too() {
    with_painter(|p| {
        let long = "A label much longer than its English button";
        let english = Rect::from_min_size(pos2(401.0, 135.0), vec2(96.0, 21.0));
        let ltr = button_rect_for(p, english, long, 300.0);
        assert_eq!(ltr.right(), 497.0);
        assert!(ltr.left() < 401.0);
        // Mirrored: the button is at the window's left, and grows to the right (inwards).
        let _s = Surface::enter_if(true, 0.0, 600.0);
        let shown = button_rect_for(p, rect(english), long, 300.0);
        assert_eq!(shown.left(), 600.0 - 497.0);
        assert_eq!(shown.width(), ltr.width());
        assert_eq!(rect(shown), ltr);
    });
}

#[test]
fn a_path_is_shortened_in_the_middle_and_keeps_its_file_name() {
    let ctx = egui::Context::default();
    let font = FontId::proportional(11.0);
    let path = r"C:\Users\Someone\Downloads\Video\Ştirile PRO TV - 4 Noiembrie 2026 _ Cutremur politic după alegerile din Statele Unite.mp4";
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let p = ui.painter();
        let width = |s: &str| text_width_font(p, font.clone(), s);
        // Fits: as it is.
        assert_eq!(ellipsize_middle(p, &font, "C:\\a.mp4", 400.0), "C:\\a.mp4");
        for max in [120.0, 200.0, 300.0] {
            let s = ellipsize_middle(p, &font, path, max);
            assert!(width(&s) <= max, "{s} is wider than {max}");
            assert!(s.starts_with("C:\\") && s.ends_with(".mp4"), "{s}");
            assert!(s.contains("..."), "{s}");
        }
        // Nothing fits but the dots: no panic, the dots alone.
        assert_eq!(ellipsize_middle(p, &font, path, 1.0), "...");
    });
    out.textures_delta.clear();
}

/// A glide covers most of its way at first and slows as it nears, never going past its
/// target and ending on it exactly; it goes either way.
#[test]
fn a_glide_eases_to_its_target() {
    let mut pos = 0.0;
    let mut steps = Vec::new();
    for _ in 0..60 {
        pos = glide_toward(pos, 51.0, 1.0 / 60.0);
        steps.push(pos);
    }
    assert!(steps[0] > 10.0 && steps[0] < 51.0, "{steps:?}");
    assert!(steps.windows(2).all(|w| w[1] >= w[0] && w[1] <= 51.0));
    let gains: Vec<f32> = steps.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gains[0] > gains[3], "slows down: {gains:?}");
    let there = steps.iter().position(|&p| p == 51.0).expect("arrives");
    assert!(
        there < 20,
        "in about a sixth of a second at 60 Hz: {there} frames"
    );
    assert_eq!(
        glide_toward(100.0, 49.0, 1.0),
        49.0,
        "a long frame: there at once"
    );
    assert!(glide_toward(100.0, 49.0, 1.0 / 60.0) < 100.0);
    assert_eq!(glide_toward(5.0, 5.0, 0.0), 5.0);
}

/// The wheel glides a row list to where its notch takes it, part of the way at the first
/// frame and the rest over the next ones, ending on a whole row; the keys move it at once,
/// by whole rows, ending the glide.
#[test]
fn the_wheel_glides_a_row_list_to_whole_rows() {
    let ctx = egui::Context::default();
    let mut defs = egui::FontDefinitions::default();
    install_fonts(&mut defs, None);
    ctx.set_fonts(defs);
    let mut st = RowList::default();
    let mut t = 0.0;
    let mut frame = |st: &mut RowList, events: Vec<egui::Event>| {
        t += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
            time: Some(t),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        let mut ev = RowEvents::default();
        let mut out = ctx.run_ui(input, |ui| {
            let rc = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(200.0, 102.0));
            ev = row_list(ui, "glide-rows", rc, 100, 20.0, st, |_, _, _, _, _| {});
        });
        out.textures_delta.clear();
        ev
    };
    frame(&mut st, vec![egui::Event::PointerMoved(pos2(50.0, 50.0))]);
    let notch = || egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    };
    #[cfg(windows)]
    let lines = crate::win32::wheel_lines();
    #[cfg(unix)]
    let lines = crate::unix::wheel_lines();
    // Five rows show in the box: a page if the system scrolls pages.
    let rows = lines.unwrap_or(5.0).round() as usize;
    frame(&mut st, vec![notch()]);
    let first = st.first as f32 * 20.0 + st.px;
    assert!(
        first > 0.0 && first < rows as f32 * 20.0,
        "part of the way: {first}"
    );
    for _ in 0..40 {
        frame(&mut st, vec![]);
    }
    assert_eq!((st.first, st.px), (rows, 0.0), "on a whole row");
    // Another notch, then a key halfway: at once, on whole rows.
    frame(&mut st, vec![notch()]);
    assert!(st.px > 0.0 || st.first > rows);
    st.selected = Some(rows + 40);
    st.pending = true;
    frame(&mut st, vec![]);
    assert_eq!(st.px, 0.0);
    let at = st.first;
    for _ in 0..10 {
        frame(&mut st, vec![]);
    }
    assert_eq!(st.first, at, "the glide ended there");
}

/// A row list drawn headless with AccessKit on: its rows' nodes (role, name, place in the
/// list) and the list's state after the frame.
struct Drawn {
    rows: Vec<(egui::accesskit::NodeId, String, usize, usize)>,
    list_children: usize,
    st: RowList,
    events: RowEvents,
    took: std::time::Duration,
}

fn draw_list(ctx: &egui::Context, n: usize, st: RowList, events: Vec<egui::Event>) -> Drawn {
    let mut st = st;
    let mut ev = RowEvents::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
        events,
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let mut out = ctx.run_ui(input, |ui| {
        let rc = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(200.0, 102.0));
        ev = row_list(ui, "test-rows", rc, n, 20.0, &mut st, |_, _, i, _, look| {
            look.name = format!("Row {i}");
        });
    });
    let took = started.elapsed();
    out.textures_delta.clear();
    let update = out
        .platform_output
        .accesskit_update
        .expect("AccessKit is on");
    let mut rows = Vec::new();
    let by_id: std::collections::HashMap<_, _> =
        update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    // The list's rows as a screen reader sees them: its children, through the containers
    // that AccessKit leaves out.
    fn options(
        by_id: &std::collections::HashMap<egui::accesskit::NodeId, &egui::accesskit::Node>,
        node: &egui::accesskit::Node,
    ) -> usize {
        node.children()
            .iter()
            .filter_map(|c| by_id.get(c))
            .map(|c| match c.role() {
                egui::accesskit::Role::ListBoxOption => 1,
                egui::accesskit::Role::GenericContainer => options(by_id, c),
                _ => 0,
            })
            .sum()
    }
    let mut list_children = 0;
    for (id, node) in &update.nodes {
        match node.role() {
            egui::accesskit::Role::ListBoxOption => rows.push((
                *id,
                node.label().unwrap_or_default().to_owned(),
                node.position_in_set().unwrap_or(0),
                node.size_of_set().unwrap_or(0),
            )),
            egui::accesskit::Role::ListBox => list_children = options(&by_id, node),
            _ => {}
        }
    }
    rows.sort_by_key(|r| r.2);
    Drawn {
        rows,
        list_children,
        st,
        events: ev,
        took,
    }
}

fn ask(target: egui::accesskit::NodeId, action: egui::accesskit::Action) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action,
        target_tree: egui::accesskit::TreeId::ROOT,
        target_node: target,
        data: None,
    })
}

/// #111: every row of a short list is in the accessibility tree, in view or not, inside the
/// list, named, with its place; a screen reader can scroll one into view or select it.
#[test]
fn every_row_of_a_short_list_is_reachable() {
    use egui::accesskit::Action;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let st = RowList::default();
    let d = draw_list(&ctx, 40, st, vec![]);
    let d = draw_list(&ctx, 40, d.st, vec![]);
    assert_eq!(d.rows.len(), 40, "every row has a node");
    assert_eq!(d.list_children, 40, "inside the list");
    for (i, (_, name, pos, size)) in d.rows.iter().enumerate() {
        assert_eq!(
            (name.as_str(), *pos, *size),
            (format!("Row {i}").as_str(), i, 40)
        );
    }
    assert_eq!(d.st.first, 0);
    // Scroll into view: row 35 shows (the view is 5 rows).
    let row35 = d.rows[35].0;
    let d = draw_list(&ctx, 40, d.st, vec![ask(row35, Action::ScrollIntoView)]);
    assert!((d.st.first..d.st.first + 5).contains(&35), "{:?}", d.st);
    // Select (AccessKit's click) a row out of view: selected and brought into view.
    let row2 = d.rows[2].0;
    let d = draw_list(&ctx, 40, d.st, vec![ask(row2, Action::Click)]);
    assert_eq!((d.st.selected, d.events.picked), (Some(2), Some(2)));
    let d = draw_list(&ctx, 40, d.st, vec![]);
    assert!((d.st.first..d.st.first + 5).contains(&2), "{:?}", d.st);
}

/// #111: a list of thousands (a playlist, #69) exposes a bounded window of rows around the
/// view, each with its place among all of them, so a frame costs what a short list's does.
#[test]
fn a_long_list_exposes_a_window_of_rows_around_the_view() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let st = RowList {
        first: 5_000,
        ..RowList::default()
    };
    let d = draw_list(&ctx, 10_000, st, vec![]);
    let d = draw_list(&ctx, 10_000, d.st, vec![]);
    assert_eq!(d.rows.len(), A11Y_ROWS);
    let (lo, hi) = (d.rows[0].2, d.rows[A11Y_ROWS - 1].2);
    assert!(lo < 5_000 && hi >= 5_004, "{lo}..={hi}");
    assert!(d.rows.iter().all(|r| r.3 == 10_000));
    // Timing, for the record (cargo test --release -- --nocapture): AccessKit on vs off.
    let mut on = std::time::Duration::ZERO;
    let mut st = d.st;
    for _ in 0..20 {
        let d = draw_list(&ctx, 10_000, st, vec![]);
        on += d.took;
        st = d.st;
    }
    let off_ctx = egui::Context::default();
    let mut off = std::time::Duration::ZERO;
    for _ in 0..20 {
        let started = std::time::Instant::now();
        let mut out = off_ctx.run_ui(egui::RawInput::default(), |ui| {
            let rc = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(200.0, 102.0));
            row_list(
                ui,
                "test-rows",
                rc,
                10_000,
                20.0,
                &mut st,
                |_, _, i, _, look| {
                    look.name = format!("Row {i}");
                },
            );
        });
        off += started.elapsed();
        out.textures_delta.clear();
    }
    println!(
        "10,000 rows, a frame: {:?} with a screen reader on, {:?} off",
        on / 20,
        off / 20
    );
}

#[test]
fn a_control_greyed_with_the_keys_hands_them_on() {
    let ctx = egui::Context::default();
    let a = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(75.0, 23.0));
    let b = a.translate(egui::vec2(0.0, 30.0));
    let mut first_id = None;
    // Labels from variables: the catalog's extractor takes literal ones as texts to translate.
    let (one, two) = ("OK", "Cancel");
    let frame = |enabled: bool, first_id: &mut Option<egui::Id>| {
        let mut second = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let kind = if enabled {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            };
            let r = button(ui, a, one, kind, false, None);
            if first_id.is_none() {
                r.request_focus();
            }
            *first_id = Some(r.id);
            second = Some(button(ui, b, two, ButtonKind::Normal, false, None).id);
        });
        out.textures_delta.clear();
        second.unwrap()
    };
    frame(true, &mut first_id);
    frame(true, &mut first_id);
    assert!(ctx.memory(|m| m.has_focus(first_id.unwrap())));
    // Greyed: the next button has the keys (egui had kept them on the greyed one).
    let second = frame(false, &mut first_id);
    frame(false, &mut first_id);
    assert!(
        ctx.memory(|m| m.has_focus(second)),
        "{:?}",
        ctx.memory(|m| m.focused())
    );
}

#[test]
/// egui's context locks itself for `data`, `memory`, `input` and their `_mut`s: asking it
/// again inside one (`ctx.viewport_id()` to build a key) deadlocks the app at once. The
/// key is worked out first; this keeps it so in every file of the app.
fn no_context_call_inside_a_context_lock() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let opens = [
        ".data(|",
        ".data_mut(|",
        ".memory(|",
        ".memory_mut(|",
        ".input(|",
        ".input_mut(|",
        ".options(|",
        ".options_mut(|",
        ".fonts(|",
        ".fonts_mut(|",
    ];
    // What asks the context again: its own methods, and a Response's that read its memory
    // (`d.insert_temp(key, resp.has_focus())` froze every editable drop-down as it took the
    // focus: 0.2.2's Add URL).
    let inner = [
        "ctx()",
        ".viewport_id()",
        ".cumulative_pass_nr()",
        "_key(ctx",
        "_key(ui",
        "_key(&",
        ".has_focus()",
        ".gained_focus()",
        ".lost_focus()",
        ".request_focus()",
        ".surrender_focus()",
        "ui.data(",
        "ui.data_mut(",
        "ui.memory(",
        "ui.memory_mut(",
        "ui.input(",
        "ui.input_mut(",
        "keyboard_in_use(",
        "widget_id(",
    ];
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let mut src = std::fs::read_to_string(&path).unwrap();
        // Not this test's own patterns.
        if let Some(me) = src.find("fn no_context_call_inside_a_context_lock") {
            src.truncate(me);
        }
        for open in opens {
            let mut from = 0;
            while let Some(at) = src[from..].find(open) {
                let start = from + at + open.len() - 2; // the "(" of the call
                let mut depth = 0;
                let mut end = start;
                for (k, c) in src[start..].char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    if depth == 0 {
                        end = start + k;
                        break;
                    }
                }
                let body = &src[start..end];
                if inner.iter().any(|i| body.contains(i)) {
                    let line = src[..start].lines().count();
                    found.push(format!("{}:{line}", path.display()));
                }
                from = end.max(start + 1);
            }
        }
    }
    assert!(
        found.is_empty(),
        "context asked inside its own lock: {found:?}"
    );
}

#[test]
fn no_let_in_an_if_binds_a_condition() {
    // `if a && let x = b && c` binds `b` alone and makes `c` a condition of the `if`: 0.2.3's
    // toolbar bound `on_arrow = dropdown && <pointer past the button>`, and only its arrows
    // ever clicked. Such a value is computed before the `if`.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for (n, line) in src.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.contains("fn no_let_in_an_if") {
                continue;
            }
            // `let <name> = <expr> &&…` / `||…` after an `if` or `&&` on the same line.
            for lead in ["if let ", "&& let "] {
                if let Some(at) = code.find(lead) {
                    let rest = &code[at + lead.len()..];
                    if let Some(eq) = rest.find(" = ")
                        && rest[..eq].chars().all(|c| c.is_alphanumeric() || c == '_')
                        && (rest[eq..].contains(" && ") || rest[eq..].contains(" || "))
                    {
                        found.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a let in an if binds only up to its &&: {found:?}"
    );
}

#[test]
fn button_labels_stay_centred_in_taller_buttons() {
    // The measured heights keep their measured baselines.
    assert_eq!(button_baseline(21.0), 13.0);
    assert_eq!(button_baseline(19.0), 13.0);
    assert_eq!(button_baseline(23.0), 14.0);
    // Taller: the label moves down by half the extra height.
    assert_eq!(button_baseline(25.0), 15.0);
    assert_eq!(button_baseline(27.0), 16.0);
    // Text boxes and read-only fields centre the same way.
    assert_eq!(baseline_in(100.0, 21.0), 113.0);
    assert_eq!(baseline_in(100.0, 23.0), 114.0);
    assert_eq!(baseline_in(100.0, 26.0), 116.0);
}

#[test]
fn lines_land_on_whole_pixels_at_any_scaling() {
    let r = Rect::from_min_max(pos2(10.0, 3.0), pos2(11.0, 4.0));
    assert_eq!(snap(1.0, r), r);
    // 125 %: a 1 pt line at x 10 → pixels 12.5..13.75: one whole pixel, on the grid.
    let s = snap(1.25, r);
    assert_eq!(
        ((s.min.x * 1.25).fract(), (s.max.x * 1.25).fract()),
        (0.0, 0.0)
    );
    assert!(s.width() * 1.25 >= 1.0);
}

#[test]
fn a_paste_into_one_line_is_one_clean_line() {
    assert_eq!(single_line("https://x/a.zip\r\n"), "https://x/a.zip");
    assert_eq!(single_line("a\r\nb\tc\u{7}"), "a b c");
}

#[test]
fn a_pressed_button_looks_pressed_in_both_themes() {
    // Issue #7: pressed was #f5f5f5 against hot #f6f6f6, the same to the eye.
    for theme in crate::palette::builtins() {
        for c in [&theme.light, &theme.dark] {
            button_states_differ(c, theme.controls.button);
        }
    }
}

fn button_states_differ(c: &crate::palette::Palette, look: crate::palette::ButtonLook) {
    let button_colors = |kind, state, c| button_colors_in(kind, state, c, look);
    {
        for kind in [ButtonKind::Normal, ButtonKind::Default] {
            let rest = button_colors(kind, State::Normal, c);
            let hot = button_colors(kind, State::Hot, c);
            let pressed = button_colors(kind, State::Pressed, c);
            // KDE: the edge takes the accent under the pointer, the face pressed; the label
            // stays.
            if look == crate::palette::ButtonLook::Breeze {
                assert_ne!(hot.border, rest.border, "hot takes the accent's edge");
                assert_ne!(pressed.fill, rest.fill, "pressed tints the face");
                assert_eq!(pressed.bottom, pressed.border, "one edge all round");
                continue;
            }
            // The era themes: pressed deepens the face; under the pointer the face lightens
            // (Windows 7, Mac OS X) or Windows XP's glow shows inside the edge.
            use crate::palette::ButtonLook as B;
            if matches!(look, B::Luna | B::Aero | B::Aqua) {
                assert_ne!(pressed.fill, rest.fill, "pressed deepens the face");
                if look == B::Luna {
                    assert!(c.button_glow.a() > 0, "XP glows under the pointer");
                } else {
                    assert_ne!(hot.fill, rest.fill, "hot differs from rest");
                }
                continue;
            }
            assert_ne!(pressed.text, rest.text, "pressed dims the label");
            assert_eq!(
                pressed.bottom, pressed.border,
                "pressed flattens the bottom edge"
            );
            // A flat style (Windows 8) has no darker bottom edge; pressed is then told by
            // its face.
            if c.button_bottom != c.button_border {
                assert_ne!(
                    rest.bottom, rest.border,
                    "at rest the bottom edge is darker"
                );
            } else {
                assert_ne!(
                    pressed.fill, rest.fill,
                    "a flat button's pressed face differs"
                );
            }
            assert_ne!(hot.fill, rest.fill, "hot differs from rest");
        }
        let disabled = button_colors(ButtonKind::Disabled, State::Disabled, c);
        let enabled = button_colors(ButtonKind::Normal, State::Normal, c);
        let gap = |a: Color32, b: Color32| (a.r() as i32 - b.r() as i32).abs();
        assert!(
            gap(disabled.text, enabled.text) >= 0x50,
            "disabled text is clearly dimmer"
        );
    }
}

#[test]
fn control_states_open_and_tint() {
    assert_eq!(State::Hot.open_if(true), State::Open);
    assert_eq!(State::Normal.open_if(true), State::Open);
    assert_eq!(State::Pressed.open_if(true), State::Pressed);
    assert_eq!(State::Disabled.open_if(true), State::Disabled);
    assert_eq!(State::Hot.open_if(false), State::Hot);
    let (light, dark) = (
        &crate::palette::CLASSIC.light,
        &crate::palette::CLASSIC.dark,
    );
    for c in [light, dark] {
        assert_eq!(subtle_fill(State::Normal, c), None);
        assert_eq!(subtle_fill(State::Disabled, c), None);
        let grey = |s| subtle_fill(s, c).unwrap().r();
        // Pressed (and open) is deeper than hot, on either surface.
        assert!(grey(State::Pressed) < grey(State::Hot));
        assert_eq!(subtle_fill(State::Open, c), subtle_fill(State::Pressed, c));
    }
    assert!(
        highlight_fill(State::Pressed, light).unwrap().r()
            < highlight_fill(State::Hot, light).unwrap().r()
    );
    // Windows 11 boxes: deeper under the pointer, deeper still pressed.
    // Light: darker under the pointer, darker still pressed. Dark: lighter under the
    // pointer, lighter still pressed (Windows 11). The three differ; disabled is empty.
    let fill = |s, c| fluent_box_in(s, c).0.unwrap().r();
    assert!(
        fill(State::Pressed, light) < fill(State::Hot, light)
            && fill(State::Hot, light) < fill(State::Normal, light)
    );
    assert!(
        fill(State::Normal, dark) < fill(State::Hot, dark)
            && fill(State::Hot, dark) < fill(State::Pressed, dark)
    );
    for c in [light, dark] {
        assert_eq!(fluent_box_in(State::Disabled, c).0, None);
    }
}

/// 6 px per character: a stand-in for the font.
fn w(s: &str) -> f32 {
    6.0 * s.chars().count() as f32
}

fn limits_lines() -> [SentenceLine; 2] {
    // Options → Connection's Download limits, as IDM places them.
    let line = |lead_baseline: f32, y: f32, gap: f32, baseline: f32| SentenceLine {
        lead: Lead::Right(170.0),
        lead_baseline,
        boxes: pos2(178.0, y),
        gap,
        baseline,
    };
    [
        line(345.0, 330.0, 8.0, 346.0),
        line(366.0, 351.0, 11.0, 364.0),
    ]
}

const BOXES: [(&str, egui::Vec2); 2] = [
    ("mb", egui::Vec2::new(36.0, 20.0)),
    ("hours", egui::Vec2::new(36.0, 20.0)),
];

#[test]
fn pieces_split_at_the_boxes_only() {
    let rows = sentence_pieces("Get {n} of {mb} MB\n{hours} h", &["mb", "hours"]);
    assert_eq!(
        rows,
        vec![
            vec![
                Piece::Text("Get {n} of"),
                Piece::Slot("mb"),
                Piece::Text("MB")
            ],
            vec![Piece::Slot("hours"), Piece::Text("h")],
        ]
    );
}

#[test]
fn a_long_right_aligned_lead_stays_in_the_window() {
    let w = |s: &str| 6.0 * s.chars().count() as f32;
    // 240 px ending at 170 would start at -70: it starts at the window's edge instead and the
    // box moves right to make room.
    let template = format!("{} {{mb}} MB\nevery {{hours}} hours", "x".repeat(40));
    let (texts, rects) = sentence_layout(&template, &limits_lines(), &BOXES, w, 6.0);
    assert_eq!(texts[0].0, 6.0);
    assert_eq!(rects[0].min.x, 6.0 + 240.0 + 3.0);
    // The second line, short, keeps its English place.
    assert_eq!(rects[1].min.x, 178.0);
}

#[test]
fn english_lands_where_the_pieces_were() {
    let (texts, rects) = sentence_layout(
        "Download no more than {mb} MBytes\nevery {hours} hours",
        &limits_lines(),
        &BOXES,
        w,
        f32::MIN,
    );
    let lead = w("Download no more than");
    assert_eq!(
        texts,
        vec![
            (170.0 - lead, 345.0, "Download no more than".to_owned()),
            (222.0, 346.0, "MBytes".to_owned()),
            (170.0 - w("every"), 366.0, "every".to_owned()),
            (225.0, 364.0, "hours".to_owned()),
        ]
    );
    assert_eq!(
        rects[0],
        Rect::from_min_size(pos2(178.0, 330.0), vec2(36.0, 20.0))
    );
    assert_eq!(
        rects[1],
        Rect::from_min_size(pos2(178.0, 351.0), vec2(36.0, 20.0))
    );
}

#[test]
fn a_translation_moves_the_boxes() {
    // Boxes first on each line, the hours on the first line.
    let (texts, rects) =
        sentence_layout("{hours} h: {mb} MB", &limits_lines(), &BOXES, w, f32::MIN);
    assert_eq!(rects[1].min, pos2(178.0, 330.0));
    assert_eq!(texts[0], (222.0, 346.0, "h:".to_owned()));
    assert_eq!(rects[0].min, pos2(222.0 + w("h:") + 8.0, 330.0));
    assert_eq!(texts[1].0, rects[0].right() + 8.0);
}

#[test]
fn a_long_lead_pushes_a_left_aligned_box() {
    let line = [SentenceLine {
        lead: Lead::Left(242.0),
        lead_baseline: 66.0,
        boxes: pos2(297.0, 53.0),
        gap: 5.0,
        baseline: 66.0,
    }];
    let files = [("files", vec2(39.0, 23.0))];
    let (_, rects) = sentence_layout("Download {files} files", &line, &files, w, f32::MIN);
    assert_eq!(rects[0].min.x, 297.0);
    let (_, rects) = sentence_layout("Herunterladen {files} Dateien", &line, &files, w, f32::MIN);
    assert_eq!(rects[0].min.x, 242.0 + w("Herunterladen") + 3.0);
}

/// A 400 px client area at x 100..500.
fn mirrored() -> Surface {
    Surface::enter_if(true, 100.0, 500.0)
}

#[test]
fn a_left_to_right_surface_changes_nothing() {
    let _s = Surface::enter_if(false, 100.0, 500.0);
    assert!(!rtl());
    let o = pos2(100.0, 50.0);
    assert_eq!(
        r(o, 10.0, 5.0, 30.0, 8.0),
        Rect::from_min_size(pos2(110.0, 55.0), vec2(30.0, 8.0))
    );
    assert_eq!((mx(110.0), mcol(110.0), fwd()), (110.0, 110.0, 1.0));
    assert_eq!(at(pos2(110.0, 7.0), 13.0), pos2(110.0, 7.0));
}

#[test]
fn rects_are_made_at_their_mirrored_place() {
    let _s = mirrored();
    assert!(rtl());
    let o = pos2(100.0, 50.0);
    // x → L + R − x − w: 10 px from the left edge, 30 wide → ends 10 px from the right.
    let rc = r(o, 10.0, 5.0, 30.0, 8.0);
    assert_eq!(rc, Rect::from_min_size(pos2(460.0, 55.0), vec2(30.0, 8.0)));
    // Made twice is where it was: `rect` turns a screen rect back into its English one.
    assert_eq!(
        rect(rc),
        Rect::from_min_size(pos2(110.0, 55.0), vec2(30.0, 8.0))
    );
    // An edge, a pixel column, a point that starts something 13 wide.
    assert_eq!(mx(110.0), 490.0);
    assert_eq!(mcol(110.0), 489.0);
    assert_eq!(at(pos2(110.0, 7.0), 13.0), pos2(477.0, 7.0));
    // Leading edge and reading direction.
    assert_eq!(lead(rc), rc.right());
    assert_eq!(fwd(), -1.0);
    assert_eq!(start_align(), egui::Align::RIGHT);
    assert_eq!(end_align(), egui::Align::LEFT);
}

#[test]
fn surfaces_nest_and_end() {
    {
        let _outer = mirrored();
        {
            // A popup's own surface, then a time picker's left-to-right inside.
            let _popup = Surface::enter_if(true, 0.0, 200.0);
            assert_eq!(mx(20.0), 180.0);
            let _ltr = Surface::enter_if(false, 0.0, 0.0);
            assert!(!rtl());
            assert_eq!(mx(20.0), 20.0);
        }
        assert_eq!(mx(110.0), 490.0);
    }
    assert!(!rtl());
    assert_eq!(mx(110.0), 110.0);
}

#[test]
fn a_sentence_runs_from_the_right() {
    // Download limits as English places them, then mirrored: the lead text starts at the
    // right (its pen is mirrored by `text`), the boxes follow to its left.
    let (texts, rects) = sentence_layout(
        "Download no more than {mb} MBytes\nevery {hours} hours",
        &limits_lines(),
        &BOXES,
        w,
        f32::MIN,
    );
    let _s = Surface::enter_if(true, 0.0, 400.0);
    let shown: Vec<Rect> = rects.into_iter().map(rect).collect();
    assert_eq!(
        shown[0],
        Rect::from_min_size(pos2(400.0 - 178.0 - 36.0, 330.0), vec2(36.0, 20.0))
    );
    // The lead's English box ends at 170: on screen it starts there from the right, right of
    // the box.
    let lead = &texts[0];
    let lead_box = rect(Rect::from_min_size(
        pos2(lead.0, 0.0),
        vec2(w(&lead.2), 1.0),
    ));
    assert_eq!(lead_box.left(), 400.0 - 170.0);
    assert!(lead_box.left() > shown[0].right());
}

#[test]
fn extra_lines_join_the_last_and_a_missing_box_still_shows() {
    let (texts, rects) = sentence_layout("a {mb}\nb\nc", &limits_lines(), &BOXES, w, f32::MIN);
    assert_eq!(texts.last().unwrap().2, "c");
    assert_eq!(texts.last().unwrap().1, 364.0);
    assert!(rects[1].min.x > 170.0);
}
