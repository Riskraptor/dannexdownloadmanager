use super::{
    FOCUS_GRACE, FocusVerdict, MenuBarEvent, MenuEntry, MenuFocus, MenuKind, heading_into, item_at,
    menu_viewport, menubar_next,
};
use egui::{Rect, pos2};

/// #277: on X11 the menu bar's menus share one window per level, so going from title to
/// title moves it (a compositor has no new window to fade in); elsewhere, and for every
/// other menu, a window of their own as before.
#[test]
fn the_bar_menus_share_one_window_on_x11() {
    let bar =
        |i: usize, depth, shared| menu_viewport(MenuKind::Bar, &format!("menu-{i}"), depth, shared);
    // Tasks, File, Downloads, View, Help: one window (and one for their submenus).
    for i in 1..5 {
        assert_eq!(bar(0, 0, true), bar(i, 0, true));
        assert_eq!(bar(0, 1, true), bar(i, 1, true));
    }
    assert_ne!(bar(0, 0, true), bar(0, 1, true));
    // Without sharing (Windows, macOS, native Wayland): one per menu, as before.
    assert_ne!(bar(0, 0, false), bar(1, 0, false));
    assert_eq!(
        bar(2, 0, false),
        egui::ViewportId::from_hash_of(("menu", "menu-2", 0usize))
    );
    // A context menu and a drop-down list keep their own window, shared or not.
    for shared in [true, false] {
        let ctx = menu_viewport(MenuKind::Context, "popup-1", 0, shared);
        let list = menu_viewport(MenuKind::List, "drop-x", 0, shared);
        assert_ne!(ctx, bar(0, 0, shared));
        assert_ne!(list, bar(0, 0, shared));
        assert_ne!(ctx, list);
        assert_eq!(
            ctx,
            egui::ViewportId::from_hash_of(("menu", "popup-1", 0usize))
        );
    }
}

/// Runs `events` from no menu open; the open menu after each.
fn run(events: &[MenuBarEvent]) -> Vec<Option<usize>> {
    let mut open = None;
    events
        .iter()
        .map(|e| {
            open = menubar_next(open, *e);
            open
        })
        .collect()
}

#[test]
fn a_title_clicked_after_escape_opens_its_menu_again() {
    // Click Tasks, Escape (the pointer still on Tasks: no press), click Tasks.
    let steps = run(&[
        MenuBarEvent::Title {
            i: 0,
            released: true,
        },
        MenuBarEvent::Dismissed {
            press_on_title: false,
        },
        MenuBarEvent::Title {
            i: 0,
            released: true,
        },
    ]);
    assert_eq!(steps, [Some(0), None, Some(0)]);
}

#[test]
fn clicking_the_open_menus_title_shuts_it() {
    // The press shuts the menu's window; its release, on the same title, shuts the menu.
    let steps = run(&[
        MenuBarEvent::Title {
            i: 1,
            released: true,
        },
        MenuBarEvent::Dismissed {
            press_on_title: true,
        },
        MenuBarEvent::Title {
            i: 1,
            released: true,
        },
        MenuBarEvent::Title {
            i: 1,
            released: true,
        },
    ]);
    assert_eq!(steps, [Some(1), Some(1), None, Some(1)]);
}

#[test]
fn another_title_opens_its_menu_from_an_open_one() {
    let steps = run(&[
        MenuBarEvent::Title {
            i: 0,
            released: true,
        },
        // Hovered while Tasks is open: File opens.
        MenuBarEvent::Hover(1),
        // Pressed and released on Downloads.
        MenuBarEvent::Dismissed {
            press_on_title: true,
        },
        MenuBarEvent::Title {
            i: 2,
            released: true,
        },
    ]);
    assert_eq!(steps, [Some(0), Some(1), Some(1), Some(2)]);
}

/// The focus over a menu's passes (`(focused, time)` each), as `MenuFocus` judges it.
fn focus_run(passes: &[(bool, f64)]) -> Vec<FocusVerdict> {
    let mut f = MenuFocus::default();
    passes.iter().map(|&(on, t)| f.pass(on, t)).collect()
}

/// #250: Tasks open, Import's submenu open with the focus (KWin, sway), the pointer back
/// onto another item: the submenu's window closes and hands the focus on, and for a few
/// frames no window of ours has it. The menu stays.
#[test]
fn a_submenu_closing_with_the_focus_leaves_its_menu_open() {
    let v = focus_run(&[
        (true, 0.0),   // the main window, then the menu's window
        (true, 0.5),   // the submenu's window took it
        (false, 1.0),  // the submenu closed: nobody has it
        (false, 1.02), // the next frame
        (true, 1.05),  // the main window (or the menu's) got it back
        (true, 1.6),
    ]);
    assert!(v.iter().all(|x| *x != FocusVerdict::Left), "{v:?}");
    assert_eq!(v[2], FocusVerdict::Away { wait: FOCUS_GRACE });
    // Gone again later: counted afresh.
    let v = focus_run(&[
        (true, 0.0),
        (false, 1.0),
        (true, 1.3),
        (false, 5.0),
        (false, 5.3),
    ]);
    assert!(v.iter().all(|x| *x != FocusVerdict::Left), "{v:?}");
}

/// Another app takes the focus (a click on its window, Alt+Tab): the menu closes once the
/// focus has stayed away, and asks to be looked at again until then.
#[test]
fn the_focus_gone_to_another_app_closes_the_menu() {
    let v = focus_run(&[(true, 0.0), (false, 2.0), (false, 2.1), (false, 2.5)]);
    assert_eq!(v[0], FocusVerdict::Here);
    assert!(matches!(v[1], FocusVerdict::Away { wait } if (wait - FOCUS_GRACE).abs() < 1e-9));
    assert!(
        matches!(v[2], FocusVerdict::Away { wait } if (wait - (FOCUS_GRACE - 0.1)).abs() < 1e-9)
    );
    assert_eq!(v[3], FocusVerdict::Left);
    // A menu opened while another app was in front (a tray menu): no focus to lose.
    let v = focus_run(&[(false, 0.0), (false, 1.0), (false, 9.0)]);
    assert!(v.iter().all(|x| *x == FocusVerdict::Here), "{v:?}");
}

/// #277: Help → About clicked, About closed, then the pointer over the titles: nothing opens
/// until a click. The bar is as inactive after a command as before the first click.
#[test]
fn after_a_command_the_pointer_alone_opens_nothing() {
    use MenuBarEvent::{Chosen, Dismissed, Hover, Title};
    let steps = run(&[
        // Before any click: the pointer over the titles opens nothing.
        Hover(0),
        Hover(4),
        // Help clicked; the pointer on its own title keeps it.
        Title {
            i: 4,
            released: true,
        },
        Hover(4),
        // About chosen: the menu closes.
        Chosen,
        // About opens and takes the focus, is closed, the main window has it again: no
        // event of the bar's (its menu is closed). A stray dismissal changes nothing.
        Dismissed {
            press_on_title: false,
        },
        // The pointer over every title: still nothing.
        Hover(0),
        Hover(1),
        Hover(2),
        Hover(3),
        Hover(4),
        // A click opens a menu again.
        Title {
            i: 1,
            released: true,
        },
    ]);
    assert_eq!(
        steps,
        [
            None,
            None,
            Some(4),
            Some(4),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1)
        ]
    );
    // The same after a command chosen from the keyboard (Alt+H, A), and after Escape or a
    // click elsewhere.
    for end in [
        Chosen,
        Dismissed {
            press_on_title: false,
        },
    ] {
        assert_eq!(
            run(&[MenuBarEvent::Open(4), end, Hover(0), Hover(3)]),
            [Some(4), None, None, None]
        );
    }
}

/// #579: Tasks with Export's submenu open to the right (menu 200 wide, Export's slot at
/// y 190..212). The pointer going on down or up the menu is not heading into the submenu, so
/// the next item closes it at once; crossing the item below diagonally towards the submenu
/// keeps it, as does going there on the left side of a right-to-left menu.
#[test]
fn a_submenu_stays_open_only_while_the_pointer_heads_into_it() {
    let sub = Rect::from_min_max(pos2(194.0, 187.0), pos2(330.0, 236.0));
    let left_export = pos2(120.0, 205.0);
    // Down to Import, up to Show drop target: straight, not towards the submenu.
    assert!(!heading_into(left_export, pos2(118.0, 216.0), sub));
    assert!(!heading_into(left_export, pos2(121.0, 170.0), sub));
    assert!(!heading_into(left_export, pos2(60.0, 225.0), sub));
    // Diagonally across Import, towards the submenu.
    assert!(heading_into(left_export, pos2(150.0, 214.0), sub));
    assert!(heading_into(left_export, pos2(185.0, 230.0), sub));
    // Right to left: the submenu on the left.
    let rtl = Rect::from_min_max(pos2(-130.0, 187.0), pos2(6.0, 236.0));
    assert!(heading_into(pos2(80.0, 205.0), pos2(50.0, 214.0), rtl));
    assert!(!heading_into(pos2(80.0, 205.0), pos2(82.0, 216.0), rtl));
}

/// #579, the menu itself (egui without a window system: each level drawn in the main
/// window): Tasks-like, Import's submenu open, the pointer going on up the menu closes it at
/// once and lights only the item it is on, where before the submenu stayed open (and its
/// entry lit) until the pointer had rested 0.35 s on one item, so never while it moved on.
/// Crossing an item on the way into the submenu keeps it.
#[test]
fn the_pointer_on_another_item_closes_the_submenu_at_once() {
    // Labels through names, so tools/i18n/extract.py doesn't take them for texts to translate.
    let [rss, show_drop, import, file, exit] = [
        "RSS feeds...",
        "Show drop target",
        "Import",
        "From a file",
        "Exit",
    ];
    let entries: Vec<MenuEntry<u8>> = vec![
        MenuEntry::Item(rss, 0, true, false),
        MenuEntry::Item(show_drop, 1, true, false),
        MenuEntry::Sep,
        MenuEntry::Sub(import, true, vec![MenuEntry::Item(file, 2, true, false)]),
        MenuEntry::Item(exit, 3, true, false),
    ];
    let ctx = egui::Context::default();
    let sub_key = egui::Id::unique(("popup-sub", "t579"));
    let mut t = 0.0;
    let mut frame = |pointer: Option<egui::Pos2>| {
        t += 0.016;
        let mut raw = egui::RawInput {
            time: Some(t),
            screen_rect: Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        if let Some(p) = pointer {
            raw.events.push(egui::Event::PointerMoved(p));
        }
        let mut out = ctx.run_ui(raw, |ui| {
            let mut result = None;
            super::popup_level(
                ui.ctx(),
                MenuKind::Bar,
                "t579",
                pos2(100.0, 100.0),
                None,
                &entries,
                sub_key,
                &mut result,
                0,
                &|_| None,
            );
        });
        out.textures_delta.clear();
    };
    frame(None);
    let o = ctx
        .data(|d| d.get_temp::<egui::Pos2>(sub_key.with(("origin", 0usize))))
        .expect("drawn");
    // Slots: 3..25, 25..47, separator, Import 56..78, Exit 78..100.
    let at = |x: f32, y: f32| Some(o + egui::vec2(x, y));
    let open = |ctx: &egui::Context| ctx.data(|d| d.get_temp::<usize>(sub_key));
    for _ in 0..3 {
        frame(at(60.0, 67.0));
    }
    assert_eq!(open(&ctx), Some(3), "Import's submenu opens");
    // Straight up to Show drop target: closed in that pass.
    frame(at(60.0, 40.0));
    frame(at(60.0, 38.0));
    assert_eq!(open(&ctx), None, "closed at once");

    // Again, then diagonally across Exit towards the submenu: kept.
    for _ in 0..3 {
        frame(at(60.0, 67.0));
    }
    assert_eq!(open(&ctx), Some(3));
    let right = ctx
        .data(|d| d.get_temp::<egui::Pos2>(sub_key.with(("origin", 1usize))))
        .expect("submenu drawn")
        .x;
    // Most of the way to the submenu, a little lower: over Exit.
    let x = o.x + (right - o.x) * 0.9 - o.x;
    frame(at(x, 80.0));
    assert_eq!(open(&ctx), Some(3), "kept on the way into it");
}

/// #579: the entry under the pointer, separators and padding none.
#[test]
fn the_item_under_the_pointer() {
    let [a, b, c] = ["a", "b", "c"]; // not texts to translate (tools/i18n/extract.py)
    let entries: Vec<MenuEntry<u8>> = vec![
        MenuEntry::Item(a, 0, true, false),
        MenuEntry::Sep,
        MenuEntry::Sub(b, true, vec![]),
        MenuEntry::Item(c, 1, true, false),
    ];
    let o = pos2(0.0, 0.0);
    assert_eq!(item_at(&entries, o, 200.0, pos2(50.0, 10.0)), Some(0));
    // The separator: 25..34.
    assert_eq!(item_at(&entries, o, 200.0, pos2(50.0, 29.0)), None);
    assert_eq!(item_at(&entries, o, 200.0, pos2(50.0, 40.0)), Some(2));
    assert_eq!(item_at(&entries, o, 200.0, pos2(50.0, 60.0)), Some(3));
    // The padding at the sides.
    assert_eq!(item_at(&entries, o, 200.0, pos2(3.0, 40.0)), None);
}

/// With a menu open, the pointer moves it from title to title, as in Windows' menu bars
/// (and IDM's); on its own title it stays.
#[test]
fn with_a_menu_open_the_pointer_switches_menus() {
    use MenuBarEvent::{Hover, Title};
    let steps = run(&[
        Title {
            i: 0,
            released: true,
        },
        Hover(1),
        Hover(1),
        Hover(3),
        Hover(0),
    ]);
    assert_eq!(steps, [Some(0), Some(1), Some(1), Some(3), Some(0)]);
}

#[test]
fn a_press_elsewhere_a_choice_or_the_focus_lost_closes_the_menu() {
    for end in [
        MenuBarEvent::Dismissed {
            press_on_title: false,
        },
        MenuBarEvent::Chosen,
    ] {
        assert_eq!(run(&[MenuBarEvent::Open(3), end]), [Some(3), None]);
    }
    // Left / Right from the keyboard move to the neighbour.
    assert_eq!(
        run(&[MenuBarEvent::Open(0), MenuBarEvent::Open(4)]),
        [Some(0), Some(4)]
    );
}
