use super::*;

/// View → Arrange files → By Save Path, By Referer, By Parent Web Page: the folder then the
/// name; the Referer; a video's own page, else its Referer. Saved with the layout, and an
/// unknown slot is dropped when read.
#[test]
fn the_arrange_orders_without_a_column_sort_by_their_text() {
    let mut a = Item::new(
        1,
        "https://b.example/x.zip".parse().unwrap(),
        PathBuf::from("/dl/b/x.zip"),
        Category::Compressed,
    );
    a.context.referer = Some("https://Z.example/page".into());
    let mut v = Item::new(
        2,
        "https://videos.example/watch?v=1".parse().unwrap(),
        PathBuf::from("/dl/a/clip.mp4"),
        Category::Video,
    );
    v.video = Some(model::VideoSpec {
        selector: "best".into(),
        subs: Default::default(),
    });
    let key = |i: &Item, col| sort_key(i, col, false).1;
    assert!(key(&v, list::BY_SAVE_PATH) < key(&a, list::BY_SAVE_PATH));
    assert_eq!(key(&a, list::BY_REFERER), "https://z.example/page");
    assert_eq!(key(&v, list::BY_REFERER), "");
    assert_eq!(key(&v, list::BY_PAGE), "https://videos.example/watch?v=1");
    assert_eq!(key(&a, list::BY_PAGE), "https://z.example/page");
    for col in [list::BY_SAVE_PATH, list::BY_REFERER, list::BY_PAGE] {
        assert!(list::sort_slot(col));
    }
    assert!(!list::sort_slot(list::COLS) && !list::sort_slot(99));
}
