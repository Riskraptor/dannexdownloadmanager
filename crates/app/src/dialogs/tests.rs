use super::*;

#[test]
fn add_url_recognizes_metalink_descriptors() {
    let descriptor =
        |address| crate::metalinkdlg::is_metalink_url(&Url::parse(address).unwrap(), None, None);
    assert!(descriptor("https://releases.example/app.meta4"));
    assert!(descriptor(
        "https://releases.example/app.metalink?channel=stable"
    ));
    assert!(!descriptor("https://releases.example/app.zip"));
}

/// #440: Download File Info behind the Create folder question ended without a closing pass,
/// and the next one opened as a new, hidden window that was never told to show: later
/// downloads waited behind it for good.
#[test]
fn a_dialog_gone_without_a_closing_pass_shows_when_it_opens_again() {
    egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
        let input = egui::RawInput {
            viewport_id: viewport.ids.this,
            viewports: std::iter::once((viewport.ids.this, egui::ViewportInfo::default()))
                .collect(),
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0))),
            ..Default::default()
        };
        let mut ui_cb = viewport.viewport_ui_cb;
        let mut out = ctx.run_ui(input, |ui| ui_cb(ui));
        out.textures_delta.clear();
    });
    // A test-only title, kept out of the catalog (extract.py reads literal titles).
    let title = String::from("test window 440");
    let title = title.as_str();
    let id = "test-440";
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    // A pass of the main window with the dialog (or not); whether it was told to show.
    let pass = |open: bool| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            if open {
                let _ = window(
                    ui.ctx(),
                    id,
                    title,
                    [300.0, 200.0],
                    false,
                    egui::Color32::LIGHT_GRAY,
                    |_, _| Outcome::<()>::Open,
                );
            }
        });
        out.textures_delta.clear();
        out.viewport_output
            .get(&ViewportId::from_hash_of(id))
            .is_some_and(|o| {
                o.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Visible(true)))
            })
    };
    assert!(
        !pass(true),
        "the first frame only gives the window its theme"
    );
    assert!(pass(true), "the second frame shows it");
    assert!(!pass(true));
    // The dialog ended while its window was open (no pass with the Outcome that closes it),
    // and a pass without it: egui destroys that window.
    assert!(!pass(false));
    // Opened again: a new window.
    assert!(!pass(true), "again, first its theme");
    assert!(pass(true), "and then it is shown");
    assert!(!pass(true));
}

/// #394: the question's answers: Enter ("Yes", the default) cancels the waiting ones too,
/// Escape ("No") skips this one alone, and with no answer nothing is decided.
#[test]
fn cancel_waiting_answers_enter_escape_and_nothing() {
    // The key a pass presses, delivered to the window that renders the question (a
    // thread-local: the renderer is global, and another test's window must see no key).
    thread_local! {
        static KEY: std::cell::Cell<Option<egui::Key>> = const { std::cell::Cell::new(None) };
    }
    egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
        let events = KEY.with(|k| {
            k.take().map(|key| {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                }]
            })
        });
        let input = egui::RawInput {
            viewport_id: viewport.ids.this,
            events: events.unwrap_or_default(),
            viewports: std::iter::once((viewport.ids.this, egui::ViewportInfo::default()))
                .collect(),
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 120.0))),
            ..Default::default()
        };
        let mut ui_cb = viewport.viewport_ui_cb;
        let mut out = ctx.run_ui(input, |ui| ui_cb(ui));
        out.textures_delta.clear();
    });
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    // A pass of the question, with a key pressed in it (or not); the answer it gave.
    let pass = |key: Option<egui::Key>, answer: &std::cell::Cell<Option<Outcome<bool>>>| {
        KEY.with(|k| k.set(key));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            answer.set(Some(cancel_waiting(ui.ctx(), 2)));
        });
        out.textures_delta.clear();
        answer.take()
    };
    let answer = std::cell::Cell::new(None);
    assert!(matches!(pass(None, &answer), Some(Outcome::Open)));
    assert!(matches!(
        pass(Some(egui::Key::Enter), &answer),
        Some(Outcome::Done(true))
    ));
    // Opened again: a new question.
    assert!(matches!(pass(None, &answer), Some(Outcome::Open)));
    assert!(matches!(
        pass(Some(egui::Key::Escape), &answer),
        Some(Outcome::Done(false))
    ));
}

/// Add URL opens and its address box takes the focus, frame after frame, without the app
/// hanging (0.2.2 froze here: an editable drop-down asked egui's context inside its lock).
#[test]
fn add_url_opens_and_takes_the_focus() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (address, history) in [
            ("", vec![]),
            ("https://example.com/file.zip", vec![]),
            (
                "https://example.com/file.zip",
                vec!["https://a.example/x.zip".to_owned()],
            ),
        ] {
            let ctx = egui::Context::default();
            let mut dlg = AddUrl {
                address: address.to_owned(),
                history,
                ..AddUrl::default()
            };
            for _ in 0..6 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let _ = dlg.show(ui.ctx(), |_| None);
                });
                out.textures_delta.clear();
            }
        }
        tx.send(()).unwrap();
    });
    rx.recv_timeout(std::time::Duration::from_secs(20))
        .expect("Add URL hung");
}

/// #661: Add URL asks for the clipboard as it opens and draws its frames without waiting for
/// the answer, even from a clipboard owner that never answers; the link fills the box on the
/// frame it comes, the caret after it as when it opens with it, and never a box typed in.
#[test]
fn add_url_never_waits_for_the_clipboard() {
    use std::time::{Duration, Instant};
    let (release, stuck) = std::sync::mpsc::channel::<()>();
    let stuck = std::sync::Mutex::new(stuck);
    let reader: &'static crate::clipboard::Reader =
        Box::leak(Box::new(crate::clipboard::Reader::new(
            move || {
                let _ = stuck.lock().unwrap().recv();
                Some(
                    " https://late.example/a.zip
"
                    .to_owned(),
                )
            },
            || {},
        )));
    let ctx = egui::Context::default();
    let frame = |dlg: &mut AddUrl, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let _ = dlg.show(ui.ctx(), |_| None);
        });
        out.textures_delta.clear();
    };
    let mut dlg = AddUrl {
        clip: crate::clipboard::Prefill::ask_of(reader),
        ..AddUrl::default()
    };
    let mut typed = AddUrl {
        clip: crate::clipboard::Prefill::ask_of(reader),
        ..AddUrl::default()
    };
    let started = Instant::now();
    for _ in 0..10 {
        frame(&mut dlg, vec![]);
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "10 frames took {:?}",
        started.elapsed()
    );
    assert!(dlg.address.is_empty(), "nothing yet");
    typed.address = "https://typed.example/b.zip".into();
    drop(release);
    let until = Instant::now() + Duration::from_secs(5);
    while dlg.address.is_empty() && Instant::now() < until {
        frame(&mut dlg, vec![]);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(dlg.address, "https://late.example/a.zip");
    // The caret is at its end, as in a box that opened with it: typing goes after it.
    frame(&mut dlg, vec![]);
    frame(&mut dlg, vec![egui::Event::Text("x".into())]);
    assert_eq!(dlg.address, "https://late.example/a.zipx");
    while typed.clip.pending() && Instant::now() < until {
        frame(&mut typed, vec![]);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!typed.clip.pending(), "answered");
    assert_eq!(typed.address, "https://typed.example/b.zip", "typed: kept");
}

#[test]
fn add_url_takes_only_an_address_it_downloads_from_the_clipboard() {
    let fill = |clip: &str| {
        let mut d = AddUrl::default();
        d.fill_from_clipboard(clip);
        d.address
    };
    assert_eq!(
        fill(
            " https://a.example/f.zip 
"
        ),
        "https://a.example/f.zip"
    );
    assert_eq!(fill("magnet:?xt=urn:btih:ab"), "magnet:?xt=urn:btih:ab");
    assert_eq!(fill("some words"), "");
    assert_eq!(fill("mailto:a@b.example"), "");
}

/// #511: an address with an Options → Sites Logins entry shows that login in "Use
/// authorization" (the match the download uses), follows the address while left as filled,
/// and keeps a login typed in the box. OK hands on only a login of the user's own.
#[test]
fn add_url_shows_the_sites_logins_entry_for_its_address() {
    // A made-up login; asserts don't print it.
    const PASS: &str = "dummy-pass";
    let settings = crate::settings::Settings {
        site_logins: vec![(
            "files.example.com/private".into(),
            "demo".into(),
            PASS.into(),
        )],
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut dlg = AddUrl {
        address: "https://files.example.com/private/x.zip".into(),
        ..AddUrl::default()
    };
    let pass = |dlg: &mut AddUrl| {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = dlg.show(ui.ctx(), |u| settings.site_login(u));
        });
        out.textures_delta.clear();
    };
    pass(&mut dlg);
    assert!(dlg.auth, "Use authorization is ticked");
    assert_eq!(dlg.login, "demo");
    assert!(dlg.password == PASS, "the password is the saved one");
    assert!(dlg.typed_login().is_none(), "the download follows Options");

    // Another address: the box filled from Options empties again.
    dlg.address = "https://files.example.com/public/x.zip".into();
    pass(&mut dlg);
    assert!(!dlg.auth);
    assert!(dlg.login.is_empty() && dlg.password.is_empty());

    // Back to the address, then a login typed over it: kept, and handed on.
    dlg.address = "https://files.example.com/private/y.zip".into();
    pass(&mut dlg);
    assert!(dlg.auth && dlg.login == "demo");
    dlg.login = "someone".into();
    dlg.address = "https://files.example.com/public/y.zip".into();
    pass(&mut dlg);
    assert!(dlg.auth && dlg.login == "someone", "a typed login stays");
    assert_eq!(dlg.typed_login().map(|l| l.0).as_deref(), Some("someone"));
}

/// #207: a dialog closed by the window manager (its title bar's ×) is painted in the pass
/// that closes it. The dialog's window is rendered as eframe renders it: its own pass, after
/// the window was cleared to transparent black. A pass that painted nothing left the window
/// black on screen until it was destroyed (seconds, on Linux).
#[test]
fn a_dialog_closed_by_the_window_manager_is_painted_in_its_last_pass() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let close = Rc::new(Cell::new(false));
    // How many shapes each pass of the dialog's window painted.
    let painted = Rc::new(RefCell::new(Vec::new()));
    {
        let (close, painted) = (close.clone(), painted.clone());
        egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
            let mut info = egui::ViewportInfo::default();
            if close.get() {
                info.events.push(egui::ViewportEvent::Close);
            }
            let input = egui::RawInput {
                viewport_id: viewport.ids.this,
                viewports: std::iter::once((viewport.ids.this, info)).collect(),
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0))),
                ..Default::default()
            };
            let mut ui_cb = viewport.viewport_ui_cb;
            let mut out = ctx.run_ui(input, |ui| ui_cb(ui));
            out.textures_delta.clear();
            painted.borrow_mut().push(out.shapes.len());
        });
    }
    // A test-only title, kept out of the catalog (extract.py reads literal titles).
    let title = String::from("test window");
    let title = title.as_str();
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(false);
    let pass = |closing: bool| {
        close.set(closing);
        let mut outcome = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            outcome = Some(window(
                ui.ctx(),
                "test-207",
                title,
                [300.0, 200.0],
                false,
                egui::Color32::LIGHT_GRAY,
                |ui, origin| {
                    ui.painter().text(
                        origin + vec2(10.0, 10.0),
                        egui::Align2::LEFT_TOP,
                        "Dannex Download Manager",
                        egui::FontId::proportional(12.0),
                        egui::Color32::BLACK,
                    );
                    Outcome::<()>::Open
                },
            ));
        });
        out.textures_delta.clear();
        outcome.unwrap()
    };
    assert!(matches!(pass(false), Outcome::Open));
    assert!(matches!(pass(true), Outcome::Closed));
    let painted = painted.borrow();
    assert_eq!(
        painted.len(),
        2,
        "the dialog's window rendered once per pass"
    );
    assert!(painted[0] > 0, "the open dialog painted nothing");
    assert_eq!(
        painted[1], painted[0],
        "the pass that closes the dialog paints it as the one before"
    );
}

/// A look at the disk that waits until the test lets it go (a sleeping drive, a network
/// folder that answers late); then answers as the engine would on `taken` names.
fn slow_look(taken: &'static [&'static str]) -> (Look, std::sync::mpsc::Sender<()>) {
    let (release, stuck) = std::sync::mpsc::channel::<()>();
    let stuck = Mutex::new(stuck);
    let look: Look = Arc::new(move |ask: &PlaceAsk| {
        // Released once: every look after it answers at once.
        let _ = stuck.lock().unwrap().recv();
        let mut name = ask.name.clone();
        if ask.number {
            let (stem, ext) = ask.name.rsplit_once('.').unwrap_or((ask.name.as_str(), ""));
            let ext = if ext.is_empty() {
                String::new()
            } else {
                format!(".{ext}")
            };
            let mut n = 0;
            while taken.contains(&name.as_str()) {
                n += 1;
                name = format!("{stem} ({n}){ext}");
            }
        }
        Found {
            dest: ask.dir.join(name),
            folder: true,
        }
    });
    (look, release)
}

/// Wait (at most 5 s) until `place` answers for `save_as`.
fn answered(place: &mut SaveAsPlace, save_as: &mut String) -> Option<bool> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < until {
        if let Some(there) = place.follow(save_as) {
            return Some(there);
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    None
}

/// #743: a disk that doesn't answer holds up no frame of Download File Info. Save As shows
/// the name as given (no " (1)": not known yet), and takes the free numbered one when the
/// answer comes at last.
#[test]
fn a_disk_that_never_answers_holds_up_no_frame() {
    let dir = std::env::temp_dir().join("mdm-743-slow");
    let (look, release) = slow_look(&["a.zip"]);
    let mut place = SaveAsPlace::with(look, None);
    let started = std::time::Instant::now();
    let mut save_as = place.name_in(&dir, "a.zip");
    assert_eq!(save_as, display_path(&dir.join("a.zip")), "the guess");
    for _ in 0..1000 {
        assert_eq!(place.follow(&mut save_as), None);
        assert_eq!(place.folder(&mut save_as), None, "not known yet");
    }
    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "{:?} for 1000 frames",
        started.elapsed()
    );
    assert_eq!(save_as, display_path(&dir.join("a.zip")), "still the guess");
    // The disk answers: the name is taken there.
    drop(release);
    assert_eq!(answered(&mut place, &mut save_as), Some(true));
    assert_eq!(save_as, display_path(&dir.join("a (1).zip")));
    // Asked once: the next frames keep it.
    assert_eq!(place.folder(&mut save_as), Some(true));
    assert_eq!(save_as, display_path(&dir.join("a (1).zip")));
}

/// An answer comes for the text it was asked for only: Save As typed over meanwhile keeps
/// what was typed, and a new name asked for (another folder) drops the old question's.
#[test]
fn a_late_answer_never_replaces_what_was_typed_or_picked_since() {
    let dir = std::env::temp_dir().join("mdm-743-typed");
    let (look, release) = slow_look(&["a.zip", "b.zip"]);
    let mut place = SaveAsPlace::with(look, None);
    let _guess = place.name_in(&dir, "a.zip");
    // Typed over before the disk answered.
    let mut save_as = display_path(&dir.join("mine.zip"));
    drop(release);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(place.follow(&mut save_as), None);
    assert_eq!(save_as, display_path(&dir.join("mine.zip")), "typed: kept");
    // Typed text has its folder looked at, as typed (never numbered here).
    let mut there = None;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while there.is_none() && std::time::Instant::now() < until {
        there = place.folder(&mut save_as);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(there, Some(true));
    assert_eq!(save_as, display_path(&dir.join("mine.zip")));
    // Another name: its own answer, not the last one's.
    let other = dir.join("elsewhere");
    save_as = place.name_in(&other, "b.zip");
    assert_eq!(answered(&mut place, &mut save_as), Some(true));
    assert_eq!(save_as, display_path(&other.join("b (1).zip")));
}

/// The real look follows the engine's rules: a name taken on the disk (a file, another
/// download's part file) is numbered on, the folder said there or not.
#[test]
fn the_look_numbers_as_the_engine_does() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.zip"), b"x").unwrap();
    std::fs::write(
        mdm_engine::naming::part_path(&d.path().join("a (1).zip")),
        b"x",
    )
    .unwrap();
    let mut place = SaveAsPlace::new(None);
    let mut save_as = place.name_in(d.path(), "a.zip");
    assert_eq!(answered(&mut place, &mut save_as), Some(true));
    assert_eq!(save_as, display_path(&d.path().join("a (2).zip")));
    assert_eq!(
        resolve_path(&save_as),
        mdm_engine::naming::destination(d.path(), "a.zip")
    );
    let gone = d.path().join("not-there");
    let mut save_as = place.name_in(&gone, "a.zip");
    assert_eq!(answered(&mut place, &mut save_as), Some(false));
    assert_eq!(save_as, display_path(&gone.join("a.zip")));
}

/// #743: Start in a dialog whose folder is on a disk slow to answer holds up no frame: the
/// dialog stays as it is until the answer comes, then goes on with what Start said.
#[test]
fn start_waits_for_the_folder_without_holding_up_a_frame() {
    let ctx = egui::Context::default();
    let (release, stuck) = std::sync::mpsc::channel::<()>();
    let mut gate = FolderGate::<u32>::default();
    let folder = std::env::temp_dir().join("mdm-743-gate");
    let started = std::time::Instant::now();
    let is_dir = move |_: &Path| {
        let _ = stuck.recv();
        true
    };
    let first = ensure_folder_with(&ctx, &mut gate, Outcome::Done(7), &folder, is_dir);
    assert!(matches!(first, Outcome::Open));
    for _ in 0..1000 {
        // A second Start meanwhile changes nothing; nor does a frame without one.
        let again = ensure_folder_with(&ctx, &mut gate, Outcome::Done(8), &folder, |_| {
            unreachable!("looked at once")
        });
        assert!(matches!(again, Outcome::Open));
    }
    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    drop(release);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let done = loop {
        match ensure_folder_with(&ctx, &mut gate, Outcome::Open, &folder, |_| {
            unreachable!("looked at once")
        }) {
            Outcome::Done(v) => break Some(v),
            _ if std::time::Instant::now() > until => break None,
            _ => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    };
    assert_eq!(done, Some(7), "the first Start's");
    // Let go: the next Start is looked at afresh.
    let next = ensure_folder_with(&ctx, &mut gate, Outcome::Done(9), &folder, |_| true);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut next = next;
    while !matches!(next, Outcome::Done(_)) && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
        next = ensure_folder_with(&ctx, &mut gate, Outcome::Open, &folder, |_| true);
    }
    assert!(matches!(next, Outcome::Done(9)));
}
