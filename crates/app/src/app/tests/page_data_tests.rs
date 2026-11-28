use super::page_data_for;

const PAGE: Option<&str> = Some("https://forum.example/thread/1");
const DATA: Option<&str> = Some("a=1");

#[test]
fn links_take_what_belongs_to_them() {
    let got = |link| page_data_for(link, PAGE, DATA);
    assert_eq!(got("https://forum.example/files/a.zip").as_deref(), DATA);
    assert_eq!(got("https://FORUM.example/b.zip").as_deref(), DATA);
    assert_eq!(got("https://other.test/a.zip"), None);
    assert_eq!(got("https://forum.example.other.test/a.zip"), None);
    assert_eq!(got("https://cdn.forum.example/a.zip"), None);
    assert_eq!(got("http://forum.example/a.zip"), None);
    assert_eq!(got("not an address"), None);
}

#[test]
fn no_page_or_nothing_to_take() {
    assert_eq!(page_data_for("https://a.test/x", None, DATA), None);
    assert_eq!(
        page_data_for("https://a.test/x", Some("https://a.test/"), None),
        None
    );
    assert_eq!(page_data_for("https://a.test/x", Some("nope"), DATA), None);
}
