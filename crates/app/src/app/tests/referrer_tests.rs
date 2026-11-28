use super::sent_referrer;

#[test]
fn referrers_as_browsers_send_them() {
    let url = |u: &str| url::Url::parse(u).unwrap();
    let page = "https://shop.test/orders/123?tab=2#top";
    assert_eq!(
        sent_referrer(&url("https://shop.test/invoice.pdf"), page).as_deref(),
        Some("https://shop.test/orders/123?tab=2"),
        "same site: the whole address, no fragment"
    );
    assert_eq!(
        sent_referrer(&url("https://cdn.test/a.zip"), page).as_deref(),
        Some("https://shop.test/"),
        "another site: the origin only"
    );
    assert_eq!(
        sent_referrer(&url("http://plain.test/a.zip"), page),
        None,
        "https to http: none"
    );
    assert_eq!(
        sent_referrer(&url("https://x.test/a"), "javascript:void(0)"),
        None
    );
}

#[test]
fn a_drive_players_button_downloads_the_file() {
    use super::{drive_file_behind_player, ipc};
    let page = "https://drive.google.com/file/d/1s52ek_4YTDRt_EOkx1FS53u-vJa0c4nu/view";
    let cmd = |video: &str, referer: Option<&str>| ipc::Command {
        video_page: Some(video.to_owned()),
        referer: referer.map(str::to_owned),
        ..Default::default()
    };
    assert_eq!(
        drive_file_behind_player(&cmd(page, None)).as_deref(),
        Some(page)
    );
    let stream = "https://abc.c.drive.google.com/videoplayback?id=1";
    assert_eq!(
        drive_file_behind_player(&cmd(stream, Some(page))).as_deref(),
        Some(page)
    );
    // Another site's video, even when reached from a Drive page, stays a video.
    assert_eq!(
        drive_file_behind_player(&cmd("https://www.youtube.com/watch?v=x", Some(page))),
        None
    );
    let folder = "https://drive.google.com/drive/folders/1s52ek_4YTDRt_EOkx1FS53u-vJa0c4nu";
    assert_eq!(drive_file_behind_player(&cmd(folder, None)), None);
}

/// The quality menu leaves a Drive file's player to the window's click (the file itself),
/// and everything to it in a run without a window.
#[test]
fn the_quality_menu_leaves_drive_and_headless_to_the_window() {
    use super::{ipc, menu_needs_window};
    let page = "https://drive.google.com/file/d/1s52ek_4YTDRt_EOkx1FS53u-vJa0c4nu/view";
    let look = |address: &str, referer: Option<&str>| ipc::Command {
        video_lookup: Some(ipc::VideoLookup {
            address: address.to_owned(),
            ..Default::default()
        }),
        referer: referer.map(str::to_owned),
        startup: true,
        ..Default::default()
    };
    assert!(menu_needs_window(&look(page, None), false));
    let stream = "https://abc.c.drive.google.com/videoplayback?id=1";
    assert!(menu_needs_window(&look(stream, Some(page)), false));
    let video = look("https://www.youtube.com/watch?v=x", Some(page));
    assert!(!menu_needs_window(&video, false));
    assert!(menu_needs_window(&video, true), "no window to show it");
    assert!(menu_needs_window(&look("file:///C:/v.mp4", None), false));
    assert!(menu_needs_window(&look("", None), false));
    let poll = ipc::Command {
        video_poll: Some("abc".into()),
        ..Default::default()
    };
    assert!(!menu_needs_window(&poll, false));
    assert!(menu_needs_window(&poll, true));
}
