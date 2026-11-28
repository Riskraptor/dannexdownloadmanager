use super::*;

/// Safari's copy of the helper, inside the app's extension, finds the app it opens.
#[test]
fn safari_helper_finds_its_app() {
    let exe = std::path::Path::new(
        "/Applications/Dannex Download Manager.app/Contents/PlugIns/Dannex Download Manager.appex/Contents/MacOS/dannexdownloadmanager-nmhost",
    );
    assert_eq!(
        bundle_of(exe),
        Some(PathBuf::from("/Applications/Dannex Download Manager.app"))
    );
    assert_eq!(bundle_of(std::path::Path::new("/usr/bin/nmhost")), None);
}

/// The Chrome Web Store's copy and the app folder's (unpacked) both connect, and both are
/// served: the manifest's allowed_origins and the host's own caller check agree.
#[test]
fn store_and_unpacked_extensions_both_allowed() {
    let manifest = chromium_manifest(&this_program().unwrap());
    for id in chromium_ids() {
        let o = format!("chrome-extension://{id}/");
        assert!(
            manifest["allowed_origins"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(o)),
            "{manifest}"
        );
        assert!(our_caller(&[o]));
    }
    assert!(!our_caller(&[
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".into()
    ]));
}

/// #3: what the browser hands over is timed in DDM's log, never a question, a setting or a
/// notice; the line names no address or file.
#[test]
fn only_hand_overs_are_timed() {
    let msg = |raw: &str| serde_json::from_str::<Message>(raw).unwrap();
    assert!(hands_over(&msg(r#"{"url":"https://x.test/a.zip"}"#)));
    assert!(hands_over(&msg(r#"{"links":["https://x.test/a.zip"]}"#)));
    assert!(hands_over(&msg(
        r#"{"url":"https://x.test/watch","video":true}"#
    )));
    assert!(!hands_over(&msg(r#"{"rules":true}"#)));
    assert!(!hands_over(&msg(r#"{"hello":true}"#)));
    assert!(!hands_over(&msg(
        r#"{"url":"https://x.test/watch","panel":true}"#
    )));
    assert!(!hands_over(&msg(r#"{"videoPoll":{"token":"t"}}"#)));
    assert_eq!(
        handed_line(3, 1, 9),
        "start time: browser integration: handed to DDM in 9 ms (1 ms reading Options; the message came 3 ms after the host started)"
    );
}

/// #3: without a browser of the user's own (Add browser...), the program that started the
/// host isn't looked for: a snapshot of every process, for every download.
#[test]
fn no_added_browser_no_caller_lookup() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    assert!(rules.custom_browsers.is_empty());
    assert_eq!(custom_browser(&rules), None);
    // The caller is looked for (once) only with one added, and named when it is that one.
    let added = mdm_ipc::capture::CaptureRules {
        custom_browsers: vec![("Nobody".into(), r"C:\nowhere\none.exe".into())],
        ..Default::default()
    };
    assert_eq!(
        custom_browser(&added),
        None,
        "not the program that started it"
    );
}

#[test]
fn web_and_ftp_links_are_downloadable() {
    assert!(downloadable("https://x.org/a.zip") && downloadable("FTP://x.org/a.iso"));
    assert!(downloadable("ftps://x/a") && downloadable("ftpes://x/a"));
    assert!(!downloadable("file:///C:/a") && !downloadable("javascript:alert(1)"));
}

/// Download with DDM on a magnet link: one line, a magnet's own form; never by itself.
#[test]
fn magnet_links_go_when_asked_for() {
    assert!(is_magnet(
        "magnet:?xt=urn:btih:08ada5a7a6183aae1e09d831df6748d566095a10"
    ));
    assert!(is_magnet("MAGNET:?xt=urn:btmh:1220abc&dn=x"));
    assert!(!is_magnet("magnet:"), "no query: nothing to look up");
    assert!(!is_magnet("magnet:?xt=urn:btih:abc\nx"), "one line");
    assert!(!is_magnet(&format!("magnet:?xt={}", "a".repeat(20_000))));
    assert!(
        !downloadable("magnet:?xt=urn:btih:abc"),
        "not a download by itself"
    );
    // Taken by itself (not asked for), a magnet is refused as before, not delivered.
    let raw = r#"{"url":"magnet:?xt=urn:btih:abc","userAgent":"UA","brands":[],"v":"0.6.6"}"#;
    let m: Message = serde_json::from_str(raw).unwrap();
    assert!(!m.explicit);
    let r = answer(m, &mdm_ipc::capture::CaptureRules::default());
    assert!(!r.ok, "refused: {:?}", r.error);
}

#[test]
fn only_our_extension_in_either_browser() {
    let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    for id in chromium_ids() {
        assert!(our_caller(&s(&[&format!("chrome-extension://{id}/")])));
    }
    assert!(our_caller(&s(&[r"C:\x\host.firefox.json", FIREFOX_ID])));
    assert!(!our_caller(&s(&["chrome-extension://someoneelse/"])));
    assert!(!our_caller(&s(&[r"C:\x\host.json", "evil@example.com"])));
    assert!(!our_caller(&[]));
    // Each caller is told apart: Test browser integration names its copy.
    let unpacked = s(&["chrome-extension://nphkobgebehllokfnmfdchmcbolllkif/"]);
    assert_eq!(caller_copy(&unpacked), "unpacked");
    let store = s(&[&format!("chrome-extension://{STORE_ID}/")]);
    let listed = mdm_ipc::extension::store_id().is_some();
    assert_eq!(caller_copy(&store), if listed { "store" } else { "" });
    let firefox = s(&["host.firefox.json", FIREFOX_ID]);
    assert_eq!(caller_copy(&firefox), "firefox");
    assert_eq!(caller_copy(&s(&["chrome-extension://someoneelse/"])), "");
}

#[test]
fn options_decide_what_is_taken() {
    let mut rules = mdm_ipc::capture::CaptureRules::default();
    let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36";
    let msg = |url: &str, name: &str| Message {
        url: url.into(),
        filename: Some(name.into()),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    // Not a listed type: left to the browser, nothing delivered.
    let r = answer(msg("https://example.com/page.html", "page.html"), &rules);
    assert!(!r.ok && r.error.unwrap().contains("FileType"));
    // An excluded site.
    let r = answer(
        msg("https://dl.update.microsoft.com/kb.exe", "kb.exe"),
        &rules,
    );
    assert!(!r.ok && r.error.unwrap().contains("Site"));
    // The browser off in Options.
    rules.browsers = vec![("Google Chrome".into(), false)];
    let r = answer(
        msg(
            "https://example.com/a.zip",
            "C:\\Users\\u\\Downloads\\a.zip",
        ),
        &rules,
    );
    assert!(!r.ok && r.error.unwrap().contains("Browser"));
    // The rules themselves.
    let r = answer(
        Message {
            rules: true,
            ..Default::default()
        },
        &rules,
    );
    assert!(r.ok && r.types.unwrap().contains("ZIP"));
    assert_eq!(file_name_of(&msg("https://x.org/f/b.rar?x=1", "")), "b.rar");
    // Right-click → Download with Dannex Download Manager: any link, whatever Options say.
    let mut page = msg("https://example.com/page.html", "");
    assert!(verdict(&page, &rules).is_err());
    page.explicit = true;
    assert!(verdict(&page, &rules).is_ok());
    rules.advanced_integration = false;
    assert!(verdict(&page, &rules).is_ok());
}

#[test]
fn the_video_button_asks_the_rules() {
    let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36";
    let mut rules = mdm_ipc::capture::CaptureRules {
        video_panel_sites: "*.quiet.test".into(),
        ..Default::default()
    };
    let ask = |page: &str| Message {
        panel: true,
        url: page.into(),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    let r = answer(ask("https://videos.test/watch"), &rules);
    assert!(r.ok);
    // With what Download panels says beyond on or off.
    assert_eq!(
        r.panel,
        Some(PanelLook {
            video: true,
            audio: true,
            min_seconds: 0,
            corner: "top-right"
        })
    );
    rules.video_panel_audio = false;
    rules.video_panel_min_s = 90;
    rules.video_panel_corner = "bottom-left".into();
    let json = serde_json::to_string(&answer(ask("https://videos.test/watch"), &rules)).unwrap();
    assert!(
        json.contains(
            r#""panel":{"video":true,"audio":false,"minSeconds":90,"corner":"bottom-left"}"#
        ),
        "{json}"
    );
    rules.video_panel_video = false;
    assert!(
        !answer(ask("https://videos.test/watch"), &rules).ok,
        "over nothing: off"
    );
    rules.video_panel_video = true;
    assert!(!answer(ask("https://www.quiet.test/watch"), &rules).ok);
    // #532: a player embedded from another site, in a tab on a listed site.
    let embedded = Message {
        referrer: Some("https://www.quiet.test/post".into()),
        ..ask("https://videos.test/embed/1")
    };
    assert!(!answer(embedded, &rules).ok);
    rules.video_panel = false;
    assert!(!answer(ask("https://videos.test/watch"), &rules).ok);
    // Clicked where it's off (the page was open before the change): refused, nothing sent.
    let click = Message {
        video: true,
        url: "https://videos.test/watch".into(),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    let r = answer(click, &rules);
    assert!(!r.ok && r.error.unwrap().contains("off"));
    // The toolbar button's media list asks for a stream explicitly (#38): taken wherever the
    // button is off, the manifest fetched as from its page with the cookies its server gets.
    let listed = Message {
        video: true,
        explicit: true,
        url: "https://www.quiet.test/watch".into(),
        src: Some("https://cdn.test/v/master.m3u8?t=1".into()),
        title: Some("A talk about download managers".into()),
        cookies: Some("cdn=1".into()),
        cookie_jar: vec![mdm_ipc::BrowserCookie {
            name: "cdn".into(),
            value: "1".into(),
            domain: ".cdn.test".into(),
            path: "/".into(),
            ..Default::default()
        }],
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    let cmd = video_command(&listed, &rules, browser(&listed)).unwrap();
    assert_eq!(
        cmd.video_page.as_deref(),
        Some("https://cdn.test/v/master.m3u8?t=1")
    );
    assert_eq!(cmd.referer.as_deref(), Some("https://www.quiet.test/watch"));
    assert_eq!(cmd.cookies.as_deref(), Some("cdn=1"));
    assert_eq!(
        cmd.cookie_jar, listed.cookie_jar,
        "each cookie with its domain"
    );
    assert_eq!(cmd.user_agent.as_deref(), Some(ua));
    // The page's title goes with it (#345): the app names a video yt-dlp knows only by its
    // address (here `master`) after it.
    assert_eq!(
        cmd.video_title.as_deref(),
        Some("A talk about download managers")
    );
    // With a page too: the app keeps yt-dlp's title unless it is only the address's.
    let page_only = Message {
        video: true,
        explicit: true,
        url: "https://www.quiet.test/watch".into(),
        title: Some("A talk about download managers".into()),
        ..Default::default()
    };
    let cmd = video_command(&page_only, &rules, browser(&page_only)).unwrap();
    assert_eq!(
        cmd.video_page.as_deref(),
        Some("https://www.quiet.test/watch")
    );
    assert_eq!(
        cmd.video_title.as_deref(),
        Some("A talk about download managers")
    );
    // A blank title is none.
    let blank = Message {
        video: true,
        explicit: true,
        url: "https://www.quiet.test/watch".into(),
        src: Some("https://cdn.test/v/master.m3u8".into()),
        title: Some("   ".into()),
        ..Default::default()
    };
    let cmd = video_command(&blank, &rules, browser(&blank)).unwrap();
    assert_eq!(cmd.video_title, None);
    // From the wire: the extension's `title`.
    let m: Message = serde_json::from_str(
        r#"{"video":true,"explicit":true,"url":"https://a.test/w","src":"https://cdn.test/hls/index.m3u8","title":"Clip"}"#,
    )
    .unwrap();
    assert_eq!(
        video_command(&m, &rules, browser(&m))
            .unwrap()
            .video_title
            .as_deref(),
        Some("Clip")
    );
    // Never a page that isn't on the web.
    let local = Message {
        url: "file:///C:/x.html".into(),
        ..listed
    };
    assert!(video_command(&local, &rules, browser(&local)).is_err());
    // From the wire: the extension's camelCase names.
    let m: Message =
        serde_json::from_str(r#"{"hideSite":"tube.test","openOptions":false}"#).unwrap();
    assert_eq!(m.hide_site.as_deref(), Some("tube.test"));
}

/// `hello` and `rules` say the host and its app have the quality menu; nothing else does.
#[test]
fn the_quality_menu_is_announced() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    let ua = Some("Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36".into());
    for m in [
        Message {
            hello: true,
            user_agent: ua.clone(),
            ..Default::default()
        },
        Message {
            rules: true,
            user_agent: ua.clone(),
            ..Default::default()
        },
    ] {
        let json = serde_json::to_string(&answer(m, &rules)).unwrap();
        assert!(json.contains(r#""videoLookup":true"#), "{json}");
    }
    let asked = answer(
        Message {
            panel: true,
            url: "https://videos.test/watch".into(),
            user_agent: ua,
            ..Default::default()
        },
        &rules,
    );
    let json = serde_json::to_string(&asked).unwrap();
    assert!(
        !json.contains("videoLookup") && !json.contains("lookup"),
        "{json}"
    );
}

/// The quality menu's three messages, as the extension sends them (camelCase, no `url` of
/// their own), become the app's questions, each with `startup` (an older app opens nothing).
#[test]
fn the_quality_menus_messages_become_the_apps_questions() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    let m: Message = serde_json::from_str(
        r#"{"videoLookup":{"page":"https://videos.test/watch?v=1","src":"https://cdn.test/v/master.m3u8?sig=1",
                "title":"  A talk about download managers  ","referrer":"https://search.test/","storeId":"firefox-container-2"},
              "cookies":"a=1","cookieJar":[{"name":"a","value":"1","domain":".cdn.test","path":"/","hostOnly":false}],
              "private":true,"userAgent":"Mozilla/5.0 Firefox/145.0","brands":[],"v":"0.4.0"}"#,
    )
    .unwrap();
    assert_eq!(m.url, "", "no address of its own: an older host refuses it");
    assert_eq!(menu_ask(&m), Some(MenuAsk::Lookup));
    let cmd = menu_command(MenuAsk::Lookup, &m, &rules, browser(&m)).unwrap();
    assert!(cmd.startup && cmd.wants_reply());
    assert_eq!(
        cmd.video_lookup,
        Some(mdm_ipc::VideoLookup {
            address: "https://cdn.test/v/master.m3u8?sig=1".into(),
            title: Some("A talk about download managers".into()),
            store_id: Some("firefox-container-2".into()),
            prefetch: false,
        }),
        "the video's own stream, with its page's title"
    );
    assert_eq!(
        cmd.referer.as_deref(),
        Some("https://videos.test/watch?v=1"),
        "fetched as from its page"
    );
    assert_eq!(cmd.cookies.as_deref(), Some("a=1"));
    assert_eq!(cmd.cookie_jar.len(), 1);
    assert_eq!(cmd.cookie_jar[0].domain, ".cdn.test");
    assert_eq!(cmd.user_agent.as_deref(), Some("Mozilla/5.0 Firefox/145.0"));
    assert!(cmd.private);
    assert!(
        cmd.video_page.is_none() && cmd.url.is_none() && cmd.video_title.is_none(),
        "never the window's click nor a download"
    );
    // A page alone: yt-dlp's to look up, from the page's own referrer; its title goes too
    // (the app keeps yt-dlp's unless it is only the address's).
    let page: Message = serde_json::from_str(
        r#"{"videoLookup":{"page":"https://videos.test/watch?v=1","src":"blob:https://videos.test/x",
                "title":"A talk","referrer":"https://search.test/"}}"#,
    )
    .unwrap();
    let cmd = menu_command(MenuAsk::Lookup, &page, &rules, browser(&page)).unwrap();
    let ask = cmd.video_lookup.unwrap();
    assert_eq!(ask.address, "https://videos.test/watch?v=1");
    assert_eq!(ask.title.as_deref(), Some("A talk"));
    assert_eq!(ask.store_id, None);
    assert_eq!(cmd.referer.as_deref(), Some("https://search.test/"));
    // A long title is cut.
    let long = Message {
        video_lookup: Some(LookupAsk {
            page: "https://videos.test/w".into(),
            src: Some("https://cdn.test/v.mp4".into()),
            title: Some("x".repeat(1000)),
            ..Default::default()
        }),
        ..Default::default()
    };
    let cmd = menu_command(MenuAsk::Lookup, &long, &rules, "").unwrap();
    assert_eq!(cmd.video_lookup.unwrap().title.unwrap().len(), TITLE_MAX);
    // Asked ahead of a click: said so to the app.
    let ahead: Message = serde_json::from_str(
        r#"{"videoLookup":{"page":"https://videos.test/watch?v=1","prefetch":true}}"#,
    )
    .unwrap();
    let cmd = menu_command(MenuAsk::Lookup, &ahead, &rules, "").unwrap();
    assert!(cmd.video_lookup.unwrap().prefetch);
    // No longer wanted: the token alone.
    let cancel: Message = serde_json::from_str(r#"{"videoCancel":{"token":"abc"}}"#).unwrap();
    assert_eq!(menu_ask(&cancel), Some(MenuAsk::Cancel));
    assert_eq!(
        cancel.url, "",
        "no address of its own: an older host refuses it"
    );
    let cmd = menu_command(MenuAsk::Cancel, &cancel, &rules, "").unwrap();
    assert_eq!(cmd.video_cancel.as_deref(), Some("abc"));
    assert!(cmd.startup && cmd.wants_reply() && cmd.video_lookup.is_none());
    let blank: Message = serde_json::from_str(r#"{"videoCancel":{"token":""}}"#).unwrap();
    assert_eq!(
        menu_command(MenuAsk::Cancel, &blank, &rules, "").err(),
        Some(mdm_ipc::api::lookup::GONE)
    );
    // An app that doesn't answer it (an older one, none running): nothing to do.
    assert_eq!(
        menu_answer(MenuAsk::Cancel, Ok("ok".into()), "").state,
        mdm_ipc::api::lookup::GONE
    );
    // How it goes.
    let poll: Message = serde_json::from_str(r#"{"videoPoll":{"token":"abc"}}"#).unwrap();
    assert_eq!(menu_ask(&poll), Some(MenuAsk::Poll));
    let cmd = menu_command(MenuAsk::Poll, &poll, &rules, "").unwrap();
    assert_eq!(cmd.video_poll.as_deref(), Some("abc"));
    assert!(cmd.startup && cmd.wants_reply());
    // The choice, with the tab's private window and cookie store.
    let pick: Message = serde_json::from_str(
        r#"{"videoPick":{"token":"abc","choice":2,"storeId":"firefox-private"},"private":true}"#,
    )
    .unwrap();
    assert_eq!(menu_ask(&pick), Some(MenuAsk::Pick));
    let cmd = menu_command(MenuAsk::Pick, &pick, &rules, "").unwrap();
    assert_eq!(
        cmd.video_pick,
        Some(mdm_ipc::VideoPick {
            token: "abc".into(),
            choice: 2,
            store_id: Some("firefox-private".into()),
        })
    );
    assert!(cmd.private && cmd.startup && cmd.wants_reply());
    // No token: answered without asking the app.
    let blank: Message = serde_json::from_str(r#"{"videoPoll":{"token":" "}}"#).unwrap();
    assert_eq!(
        menu_command(MenuAsk::Poll, &blank, &rules, "").err(),
        Some(mdm_ipc::api::lookup::GONE)
    );
    let blank: Message = serde_json::from_str(r#"{"videoPick":{"choice":1}}"#).unwrap();
    assert_eq!(
        menu_command(MenuAsk::Pick, &blank, &rules, "").err(),
        Some(mdm_ipc::api::lookup::USE_WINDOW)
    );
    assert_eq!(menu_ask(&Message::default()), None);
}

/// Where the button is off, or for a page not on the web, the menu is answered at once
/// (`off`, `use-window`), in `lookup` and never as a `why`.
#[test]
fn the_quality_menu_follows_options() {
    let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36";
    let mut rules = mdm_ipc::capture::CaptureRules {
        video_panel_sites: "*.quiet.test".into(),
        ..Default::default()
    };
    let look = |page: &str| Message {
        video_lookup: Some(LookupAsk {
            page: page.into(),
            ..Default::default()
        }),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    let state = |r: Reply| {
        assert!(r.ok && r.why.is_none() && r.error.is_none());
        r.lookup.unwrap().state
    };
    assert_eq!(
        state(answer(look("https://www.quiet.test/w"), &rules)),
        "off"
    );
    assert_eq!(
        state(answer(look("file:///C:/x.html"), &rules)),
        "use-window"
    );
    assert_eq!(state(answer(look(""), &rules)), "use-window");
    rules.video_panel = false;
    assert_eq!(state(answer(look("https://videos.test/w"), &rules)), "off");
    let json = serde_json::to_string(&answer(look("https://videos.test/w"), &rules)).unwrap();
    assert_eq!(json, r#"{"ok":true,"lookup":{"state":"off"}}"#);
}

/// The app's answer passes through; its absence, or a line that isn't one, says what the
/// contract says: the window for a look-up or a choice, `pending` or `gone` for a question.
#[test]
fn answers_that_are_not_the_apps() {
    use mdm_ipc::SendError;
    use mdm_ipc::api::lookup;
    let line = |s: &str| Ok::<String, SendError>(s.to_owned());
    let a = menu_answer(
        MenuAsk::Lookup,
        line(r#"{"state":"pending","token":"t1"}"#),
        "",
    );
    assert_eq!(
        (a.state.as_str(), a.token.as_deref()),
        ("pending", Some("t1"))
    );
    let a = menu_answer(
        MenuAsk::Poll,
        line(
            r#"{"state":"ready","token":"t1","title":"A talk","choices":[{"label":"720p mp4","ext":"mp4","size":null}]}"#,
        ),
        "t1",
    );
    assert_eq!(a.state, lookup::READY);
    assert_eq!(a.choices[0].ext, "mp4");
    assert_eq!(
        menu_answer(MenuAsk::Pick, line(r#"{"state":"queued"}"#), "").state,
        lookup::QUEUED
    );
    let not_answers = [
        "ok",
        "denied",
        "gone",
        "too-large",
        "",
        r#"{"error":"the automation API is off","code":"api-off"}"#,
    ];
    for got in not_answers {
        for ask in [MenuAsk::Lookup, MenuAsk::Pick] {
            assert_eq!(
                menu_answer(ask, line(got), "").state,
                lookup::USE_WINDOW,
                "{ask:?} {got}"
            );
        }
    }
    // `timeout`: the app has the command and will act on it. The choice downloads soon (the
    // Download video window would download it again), the look-up runs (another window
    // would run a second yt-dlp).
    assert_eq!(
        menu_answer(MenuAsk::Pick, line("timeout"), "").state,
        lookup::SENT
    );
    assert_eq!(
        menu_answer(MenuAsk::Pick, line("timeout\n"), "").state,
        lookup::SENT
    );
    assert_eq!(
        menu_answer(MenuAsk::Lookup, line("timeout"), "").state,
        lookup::BUSY
    );
    for err in [
        SendError::NotRunning("no data dir".into()),
        SendError::Refused("connection reset".into()),
    ] {
        assert_eq!(
            menu_answer(MenuAsk::Lookup, Err(err), "").state,
            lookup::USE_WINDOW
        );
    }
    assert_eq!(
        menu_answer(MenuAsk::Pick, Err(SendError::NotRunning("x".into())), "").state,
        lookup::USE_WINDOW
    );
    // A question: the app only too busy is asked again; anything else, the look-up is gone.
    let busy = menu_answer(MenuAsk::Poll, line("timeout"), "t1");
    assert_eq!(
        (busy.state.as_str(), busy.token.as_deref()),
        ("pending", Some("t1"))
    );
    for got in ["ok", "denied", "gone", "too-large", ""] {
        assert_eq!(
            menu_answer(MenuAsk::Poll, line(got), "t1").state,
            lookup::GONE
        );
    }
    assert_eq!(
        menu_answer(MenuAsk::Poll, Err(SendError::NotRunning("x".into())), "t1").state,
        lookup::GONE
    );
    // As the extension reads it.
    let json = serde_json::to_string(&Reply::lookup(menu_answer(MenuAsk::Lookup, line("ok"), "")))
        .unwrap();
    assert_eq!(json, r#"{"ok":true,"lookup":{"state":"use-window"}}"#);
}

/// A question waits while the look-up runs, asking the app every 100 ms, and answers as soon
/// as it is ready, or `pending` after 1.5 s; whether a choice is sound alone reaches the
/// browser as the app says it.
#[test]
fn a_question_waits_for_the_look_up() {
    use mdm_ipc::SendError;
    use mdm_ipc::api::lookup;
    let ready = r#"{"state":"ready","token":"t1","title":"A talk","choices":[{"label":"720p MP4","ext":"mp4","size":null,"audio":false},{"label":"Audio only MP3 320 kbps","ext":"mp3","size":null,"audio":true}]}"#;
    // A clock of the test's own: asking every 20 ms, the answer is ready at the third
    // question, 40 ms in.
    let now = std::cell::Cell::new(Duration::ZERO);
    let elapsed = || now.get();
    let sleep = |d: Duration| now.set(now.get() + d);
    let mut asked = 0;
    let answer = until_settled_on(
        || {
            asked += 1;
            let got = if asked < 3 {
                r#"{"state":"pending","token":"t1"}"#
            } else {
                ready
            };
            menu_answer(MenuAsk::Poll, Ok::<String, SendError>(got.into()), "t1")
        },
        POLL_WAIT,
        Duration::from_millis(20),
        elapsed,
        sleep,
    );
    assert_eq!((answer.state.as_str(), asked), (lookup::READY, 3));
    assert_eq!(now.get(), Duration::from_millis(40));
    let json = serde_json::to_string(&Reply::lookup(answer)).unwrap();
    assert!(
        json.contains(r#""ext":"mp4","size":null,"audio":false"#)
            && json.contains(r#""ext":"mp3","size":null,"audio":true"#),
        "{json}"
    );
    // Still running after the wait: `pending`, having asked every tenth of a second (at 0,
    // 100, 200 and 300 ms) and never waited past the 300 ms.
    now.set(Duration::ZERO);
    let mut asked = 0;
    let answer = until_settled_on(
        || {
            asked += 1;
            mdm_ipc::api::VideoLookupAnswer::pending("t1")
        },
        Duration::from_millis(300),
        Duration::from_millis(100),
        elapsed,
        sleep,
    );
    assert_eq!(answer.state, lookup::PENDING);
    assert_eq!(asked, 4);
    assert_eq!(now.get(), Duration::from_millis(300));
    // Anything else at once, without waiting.
    now.set(Duration::ZERO);
    let mut asked = 0;
    let gone = until_settled_on(
        || {
            asked += 1;
            mdm_ipc::api::VideoLookupAnswer::of(lookup::GONE)
        },
        POLL_WAIT,
        POLL_EVERY,
        elapsed,
        sleep,
    );
    assert_eq!(now.get(), Duration::ZERO);
    assert_eq!((gone.state.as_str(), asked), (lookup::GONE, 1));
}

#[test]
fn look_up_log_lines_keep_private_windows_out() {
    let open = lookup_line(
        "Mozilla Firefox",
        "https://cdn.test/v/master.m3u8?sig=secret",
        false,
        false,
        "pending",
    );
    assert_eq!(
        open,
        "browser integration: Mozilla Firefox looked up the qualities of a video on cdn.test: pending"
    );
    let private = lookup_line("", "https://secret.test/v.mp4", true, false, "use-window");
    assert!(!private.contains("secret") && private.contains("private window"));
    assert!(private.starts_with("browser integration: a browser "));
    assert!(!lookup_line("", "https://x.test/\n", false, false, "a\nb").contains('\n'));
    assert_eq!(
        lookup_line(
            "Chrome",
            "https://www.youtube.com/watch?v=1",
            false,
            true,
            "pending"
        ),
        "browser integration: Chrome looked up the qualities of a video on www.youtube.com ahead of a click: pending"
    );
}

#[test]
fn the_browsers_whole_context_reaches_the_app() {
    let m: Message = serde_json::from_str(
        r#"{"url":"https://x.test/get?id=1","finalUrl":"https://cdn.test/f/abc","filename":"C:\\dl\\report.pdf",
                "size":12345,"mime":"application/pdf","post":"token=abc","postType":"application/x-www-form-urlencoded",
                "private":true,"userAgent":"UA","brands":["Brave"],"v":"0.2.5"}"#,
    )
    .unwrap();
    assert_eq!(browser(&m), "Brave Browser");
    let c = to_command(m);
    assert_eq!(
        c.url.as_deref(),
        Some("https://x.test/get?id=1"),
        "the address asked for"
    );
    assert_eq!(c.final_url.as_deref(), Some("https://cdn.test/f/abc"));
    assert_eq!(
        c.name.as_deref(),
        Some("report.pdf"),
        "without the browser's folder"
    );
    assert_eq!(
        (c.size, c.mime.as_deref()),
        (Some(12345), Some("application/pdf"))
    );
    assert_eq!(c.post.as_deref(), Some("token=abc"));
    assert_eq!(
        c.post_type.as_deref(),
        Some("application/x-www-form-urlencoded")
    );
    assert!(c.private);
    // No form: no Content-Type for one.
    let m: Message =
        serde_json::from_str(r#"{"url":"https://x.test/a.zip","postType":"text/plain","size":0}"#)
            .unwrap();
    let c = to_command(m);
    assert_eq!((c.post_type, c.size, c.final_url), (None, None, None));
}

#[test]
fn redirects_are_checked_at_both_ends() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36";
    let mut m = Message {
        url: "https://example.com/get/a.zip".into(),
        final_url: Some("https://download.windowsupdate.com/a.zip".into()),
        filename: Some("a.zip".into()),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    assert_eq!(verdict(&m, &rules), Err(mdm_ipc::capture::Declined::Site));
    m.final_url = Some("https://cdn.example.com/a.zip".into());
    assert_eq!(verdict(&m, &rules), Ok(()));
}

#[test]
fn rules_say_everything_the_extension_needs() {
    let mut rules = mdm_ipc::capture::CaptureRules::default();
    let hello = || Message {
        hello: true,
        user_agent: Some("Mozilla/5.0 Firefox/145.0".into()),
        ..Default::default()
    };
    let r = answer(hello(), &rules);
    assert!(r.ok && r.capture == Some(true) && r.opened == Some(true));
    assert_eq!(r.version, Some(env!("CARGO_PKG_VERSION")));
    // The store's page once its listing is public, and only then: the copy loaded from
    // DDM's folder offers it (an answer without it shows nothing).
    assert_eq!(r.store, mdm_ipc::extension::store_page());
    assert_eq!(
        r.store.is_some(),
        mdm_ipc::extension::listed_store_id().is_some()
    );
    if let Some(page) = &r.store {
        assert!(page.starts_with("https://chromewebstore.google.com/detail/"));
    }
    assert!(r.excluded_sites.unwrap().contains("windowsupdate"));
    rules.browsers = vec![("Mozilla Firefox".into(), false)];
    assert_eq!(
        answer(hello(), &rules).capture,
        Some(false),
        "this browser off"
    );
    // The wire names.
    let json = serde_json::to_string(&answer(hello(), &rules)).unwrap();
    assert!(json.contains("\"excludedSites\"") && json.contains("\"capture\":false"));
    // Options → General → Keys... and the right-click items: as saved, the defaults first.
    assert!(
        json.contains(r#""keys":{"prevent":"alt","force":"ctrl+alt"}"#),
        "{json}"
    );
    assert!(
        json.contains(r#""menu":["link","selected","all","images","media"]"#),
        "{json}"
    );
    rules.key_prevent = "Shift".into();
    rules.key_force = String::new();
    rules.browser_menu_hidden = vec!["images".into(), "media".into()];
    let r = answer(hello(), &rules);
    assert_eq!(
        r.keys,
        Some(ClickKeys {
            prevent: "shift",
            force: ""
        })
    );
    assert_eq!(r.menu, Some(vec!["link", "selected", "all"]));
    // The same keys for both: the browser's (as Alt always was).
    rules.key_force = "shift".into();
    assert_eq!(answer(hello(), &rules).keys.unwrap().force, "");
    // Links that aren't downloads are refused without anything sent.
    let r = answer(
        Message {
            links: vec!["javascript:void(0)".into(), "mailto:a@b".into()],
            ..Default::default()
        },
        &rules,
    );
    assert!(!r.ok && r.why == Some("refused"));
}

/// A page's links keep their texts in step when some are left out, each text tidied; an
/// older extension's links (no texts) and links without any text send none.
#[test]
fn page_links_keep_their_texts() {
    let m: Message = serde_json::from_str(
        r#"{"links":["mailto:a@b","https://a.org/x.zip","https://a.org/y.zip"],
                "texts":["Mail us","  Get\n\tthe   zip ",""]}"#,
    )
    .unwrap();
    let texts = m
        .texts
        .iter()
        .map(String::as_str)
        .chain(std::iter::repeat(""));
    let kept: Vec<(String, String)> = m
        .links
        .iter()
        .zip(texts)
        .filter(|(l, _)| downloadable(l))
        .map(|(l, t)| (l.clone(), link_text(t)))
        .collect();
    assert_eq!(
        kept,
        [
            ("https://a.org/x.zip".to_owned(), "Get the zip".to_owned()),
            ("https://a.org/y.zip".to_owned(), String::new()),
        ]
    );
    let long = link_text(&"word ".repeat(100));
    assert_eq!(long.chars().count(), LINK_TEXT_MAX + 1);
    assert!(long.ends_with('…'));
    let old: Message = serde_json::from_str(r#"{"links":["https://a.org/x.zip"]}"#).unwrap();
    assert!(old.texts.is_empty());
}

#[test]
fn a_refusal_says_which_rule() {
    let rules = mdm_ipc::capture::CaptureRules {
        browsers: vec![("Microsoft Edge".into(), false)],
        ..Default::default()
    };
    let ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36";
    let msg = |url: &str, name: &str, ua: &str| Message {
        url: url.into(),
        filename: Some(name.into()),
        user_agent: Some(ua.into()),
        ..Default::default()
    };
    let r = answer(msg("https://x.test/p.png", "p.png", ua), &rules);
    assert_eq!(
        (r.ok, r.why, r.declined),
        (false, Some("options"), Some("file-type"))
    );
    // The words older extensions read stay as they were.
    assert_eq!(
        r.error.as_deref(),
        Some("left to the browser by Options (FileType)")
    );
    let edge = "Mozilla/5.0 (Windows NT 10.0) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
    let r = answer(msg("https://x.test/a.zip", "a.zip", edge), &rules);
    assert_eq!(r.declined, Some("browser"));
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains(r#""declined":"browser""#), "{json}");
    // The rules say why nothing is taken from this browser.
    let hello = Message {
        hello: true,
        user_agent: Some(edge.into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&answer(hello, &rules)).unwrap();
    assert!(json.contains(r#""capture":false"#) && json.contains(r#""captureOff":"browser""#));
}

#[test]
fn the_extensions_own_refusals_are_only_noted() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    let raw = r#"{"declined":{"why":"file-type","site":"x.test","name":"p.png"},"userAgent":"UA","brands":[],"v":"0.3.0"}"#;
    let m: Message = serde_json::from_str(raw).unwrap();
    assert_eq!(
        m.declined,
        Some(Notice {
            why: "file-type".into(),
            site: "x.test".into(),
            name: "p.png".into()
        })
    );
    // No address of its own: an older host, which knows no `declined`, refuses it as an
    // address it can't download, and hands nothing over.
    assert!(!downloadable(&m.url));
    let r = answer(m, &rules);
    assert!(r.ok && r.error.is_none(), "noted, nothing delivered");
    assert_eq!(
        notice_reason("site"),
        mdm_ipc::capture::Declined::Site.reason()
    );
    assert!(notice_reason("site-access").contains("site access"));
    assert_eq!(notice_reason("x\ny"), r#""x y""#);
}

#[test]
fn log_lines_name_what_was_left_and_why() {
    let reason = mdm_ipc::capture::Declined::FileType.reason();
    assert_eq!(
        left_line("Waterfox", "x.test", "p.png", reason, false),
        "browser integration: Waterfox keeps p.png from x.test: its type isn't in Options > File types"
    );
    // A private window's download is never named; nor can a name break the line.
    let private = left_line("Google Chrome", "secret.test", "s.zip", reason, true);
    assert!(!private.contains("secret") && private.contains("private window"));
    assert!(!left_line("", "x.test", "a\nb.zip", reason, false).contains('\n'));
    assert!(
        left_line("", "", "", reason, false)
            .starts_with("browser integration: a browser keeps a download:")
    );
}

#[test]
fn older_extensions_messages_still_work() {
    // As the Chrome Web Store's 0.2.4 extension sends a download (no browser name from
    // getBrowserInfo, no notices), and as 0.2.4's own extension did (no brands, no version).
    let rules = mdm_ipc::capture::CaptureRules::default();
    for raw in [
        r#"{"url":"https://x.test/a.zip","finalUrl":"","referrer":"https://x.test/","cookies":"","filename":"a.zip","size":1048576,"mime":"application/zip","userAgent":"Mozilla/5.0 Chrome/141.0.0.0 Safari/537.36","brands":["Not:A-Brand","Chromium","Google Chrome"],"v":"0.2.4"}"#,
        r#"{"url":"https://x.test/a.zip","referrer":"https://x.test/","cookies":"","userAgent":"Mozilla/5.0 Chrome/141.0.0.0 Safari/537.36","filename":"a.zip"}"#,
    ] {
        let m: Message = serde_json::from_str(raw).unwrap();
        assert_eq!(verdict(&m, &rules), Ok(()), "{raw}");
        assert_eq!(to_command(m).name.as_deref(), Some("a.zip"));
    }
}

#[test]
fn framing_round_trip() {
    let mut buf = Vec::new();
    write_message(&mut buf, br#"{"url":"https://x/a.zip"}"#).unwrap();
    assert_eq!(&buf[..4], &25u32.to_le_bytes());
    let mut r = &buf[..];
    assert_eq!(
        read_message(&mut r).unwrap().unwrap().unwrap(),
        br#"{"url":"https://x/a.zip"}"#
    );
    assert!(
        read_message(&mut r).unwrap().is_none(),
        "clean end of input"
    );
}

#[test]
fn huge_messages_are_read_past_and_said() {
    let big = 2usize << 20;
    let mut input = (big as u32).to_le_bytes().to_vec();
    input.extend(vec![b'x'; big]);
    write_message(&mut input, br#"{"rules":true}"#).unwrap();
    let mut r = &input[..];
    assert_eq!(read_message(&mut r).unwrap(), Some(Err(big)));
    assert!(
        matches!(read_message(&mut r).unwrap(), Some(Ok(_))),
        "the next one reads"
    );
}

/// The extension's cookies with their domains, as `cookies.getAll` names them (its other
/// fields ignored), reach the app; an older extension's message has none.
#[test]
fn cookies_keep_their_domains() {
    let m: Message = serde_json::from_str(
        r#"{"url":"https://music.youtube.com/watch?v=x","cookies":"SID=1; pref=2","userAgent":"UA",
              "cookieJar":[{"name":"SID","value":"1","domain":".youtube.com","hostOnly":false,"path":"/","secure":true,"httpOnly":true,"sameSite":"lax","session":false,"expirationDate":1893456000.5,"storeId":"0"},
                           {"name":"pref","value":"2","domain":"music.youtube.com","hostOnly":true,"path":"/","secure":false,"session":true}]}"#,
    )
    .unwrap();
    let c = to_command(m);
    assert_eq!(c.cookies.as_deref(), Some("SID=1; pref=2"));
    assert_eq!(
        c.cookie_jar,
        [
            mdm_ipc::BrowserCookie {
                name: "SID".into(),
                value: "1".into(),
                domain: ".youtube.com".into(),
                host_only: false,
                path: "/".into(),
                secure: true,
                expires: Some(1893456000.5),
            },
            mdm_ipc::BrowserCookie {
                name: "pref".into(),
                value: "2".into(),
                domain: "music.youtube.com".into(),
                host_only: true,
                path: "/".into(),
                secure: false,
                expires: None,
            },
        ]
    );
    // To the app and back (its command line): the same.
    let line = serde_json::to_string(&c).unwrap();
    let back: mdm_ipc::Command = serde_json::from_str(&line).unwrap();
    assert_eq!(back.cookie_jar, c.cookie_jar);
    let old: Message =
        serde_json::from_str(r#"{"url":"https://x/a.zip","cookies":"a=1"}"#).unwrap();
    let c = to_command(old);
    assert!(c.cookie_jar.is_empty());
    assert!(!serde_json::to_string(&c).unwrap().contains("cookie_jar"));
}

#[test]
fn message_to_command() {
    let m: Message = serde_json::from_str(
        r#"{"url":"https://x/a.zip","referrer":"https://x/","cookies":"a=1","userAgent":"UA","filename":""}"#,
    )
    .unwrap();
    let c = to_command(m);
    assert_eq!(c.url.as_deref(), Some("https://x/a.zip"));
    assert_eq!(c.referer.as_deref(), Some("https://x/"));
    assert_eq!(c.cookies.as_deref(), Some("a=1"));
    assert_eq!(c.user_agent.as_deref(), Some("UA"));
    assert_eq!(
        c.name, None,
        "empty filename means: let Dannex Download Manager decide"
    );
    assert!(!c.later && !c.silent);
    assert_eq!((c.proxy, c.proxy_pac, c.proxy_login), (None, None, false));
}

/// #376: the app hears which browser a download comes from, and whether it is the browser's
/// own download waiting for the answer (it is unless the user asked from the right-click
/// menu, where no download of the browser's waits).
#[test]
fn the_browser_and_its_waiting_download_go_with_it() {
    let firefox = r#"{"url":"https://x/a.zip","userAgent":"Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0","brands":["Zen"]}"#;
    let c = to_command(serde_json::from_str(firefox).unwrap());
    assert_eq!(c.from_browser.as_deref(), Some("Zen Browser"));
    assert!(c.browser_waits);
    let asked = r#"{"url":"https://x/a.zip","explicit":true,"userAgent":"Mozilla/5.0 Chrome/141.0.0.0 Safari/537.36","brands":["Brave"]}"#;
    let c = to_command(serde_json::from_str(asked).unwrap());
    assert_eq!(c.from_browser.as_deref(), Some("Brave Browser"));
    assert!(
        !c.browser_waits,
        "the right-click menu's: no download waits"
    );
    // A browser the host can't name sends none.
    let c = to_command(serde_json::from_str(r#"{"url":"https://x/a.zip"}"#).unwrap());
    assert_eq!(c.from_browser, None);
}

/// #376: the app's `handed-back` becomes a refusal the extension leaves to the browser
/// quietly (`declined`, as a rule of Options'), never a connection problem; its other
/// answers stay as they were.
#[test]
fn a_download_handed_back_is_declined() {
    let r = reply_to(Err(Undelivered::Refused(mdm_ipc::HANDED_BACK.into())));
    assert_eq!(
        (r.ok, r.why, r.declined),
        (false, Some("options"), Some("handed-back"))
    );
    let json = serde_json::to_string(&r).unwrap();
    assert!(
        json.contains(r#""declined":"handed-back""#) && json.contains(r#""why":"options""#),
        "{json}"
    );
    let r = reply_to(Err(Undelivered::Refused("too-large".into())));
    assert_eq!((r.why, r.declined), (Some("too-large"), None));
    let r = reply_to(Err(Undelivered::Refused("gone".into())));
    assert_eq!((r.why, r.declined), (Some("refused"), None));
    let r = reply_to(Err(Undelivered::NotRunning("no file".into())));
    assert_eq!((r.why, r.declined), (Some("not-running"), None));
    assert!(reply_to(Ok(())).ok);
    // The log says so, in the words of the other downloads left to the browser.
    assert_eq!(
        left_line(
            "Mozilla Firefox",
            "x.test",
            "a.zip",
            HANDED_BACK_REASON,
            false
        ),
        "browser integration: Mozilla Firefox keeps a.zip from x.test: handed back by DDM \
             after the site's bot check (Download with the browser)"
    );
}

#[test]
fn the_browsers_proxy_goes_with_the_download() {
    let m: Message = serde_json::from_str(
        r#"{"url":"https://x/a.zip","proxy":"socks5h://vpn.example:1080","proxyLogin":true}"#,
    )
    .unwrap();
    let c = to_command(m);
    assert_eq!(c.proxy.as_deref(), Some("socks5h://vpn.example:1080"));
    assert!(c.proxy_login);
    let pac = r#"{"url":"https://x/a.zip","proxyPac":"function FindProxyForURL(u, h) { return \"PROXY p:8080\"; }"}"#;
    assert!(
        to_command(serde_json::from_str(pac).unwrap())
            .proxy_pac
            .is_some()
    );
    // Only proxies the app can use, written as expected.
    for good in [
        "http://10.0.0.1:3128",
        "https://p.example:443",
        "socks4://[::1]:1080",
    ] {
        assert_eq!(browser_proxy(good).as_deref(), Some(good));
    }
    for bad in [
        "ftp://p:21",
        "http://p",
        "http://p:0",
        "http://user:pw@p:8080",
        "http://p:8080/path",
        "socks5://bad host:1080",
    ] {
        assert_eq!(browser_proxy(bad), None, "{bad}");
    }
    let not_pac = r#"{"url":"https://x/a.zip","proxyPac":"alert(1)"}"#;
    assert!(
        to_command(serde_json::from_str(not_pac).unwrap())
            .proxy_pac
            .is_none()
    );
}

/// An extension up to 0.6.5 asking for the app's downloads (its Downloads and links window,
/// gone): refused as a message the host doesn't know, with nothing asked of the app or
/// handed to it, and no `why` that would say the browser can't reach DDM.
#[test]
fn the_old_windows_questions_are_refused() {
    let rules = mdm_ipc::capture::CaptureRules::default();
    for raw in [
        r#"{"downloads":true,"v":"0.6.5"}"#,
        r#"{"downloadsAct":{"action":"pause","all":true},"v":"0.6.5"}"#,
        r#"{"downloadsAct":{"action":"folder","ids":[7]},"v":"0.6.5"}"#,
    ] {
        let m: Message = serde_json::from_str(raw).unwrap();
        assert_eq!(
            m,
            Message {
                v: Some("0.6.5".into()),
                ..Default::default()
            },
            "{raw}"
        );
        let r = answer(m, &rules);
        assert!(!r.ok && r.why == Some("refused"), "{raw}: {:?}", r.why);
        assert!(r.lookup.is_none());
    }
}
