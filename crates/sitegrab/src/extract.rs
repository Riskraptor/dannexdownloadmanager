//! The links of a web page and of a style sheet, read as browsers find them: not a full HTML
//! parser, a tokenizer that knows what browsers know about where text isn't markup (comments,
//! scripts, styles, `<textarea>` and `<title>`), quoted and unquoted attributes, character
//! references, `<base href>`, `srcset` and CSS `url()`.
//!
//! The tokenizer reads bytes and notes where each attribute's value is in the page, so an offline
//! copy (`crate::offline`) changes exactly those values and leaves every other byte as it was (a
//! page in an older encoding than UTF-8 stays in it).

use std::ops::Range;

use url::Url;

/// What a link leads to, as the tag that holds it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Somewhere to go (`<a href>`, `<area>`, `<frame>`, `<iframe>`, a refresh, `rel=next`): a
    /// page to explore, or a file.
    Page,
    /// Something the page shows or uses (images, video, audio, scripts, icons, CSS `url()`).
    Resource,
    /// A style sheet (`<link rel=stylesheet>`, `@import`): a resource whose own `url()`s count.
    Style,
}

/// One link as written on the page (character references decoded, not yet resolved).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub href: String,
    pub kind: LinkKind,
}

/// A page's links, and the `<base href>` its relative links are resolved against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageLinks {
    pub base: Option<String>,
    pub links: Vec<Link>,
}

impl PageLinks {
    fn push(&mut self, href: &str, kind: LinkKind) {
        let href = decode(href.trim());
        if !href.is_empty() {
            self.links.push(Link { href, kind });
        }
    }
}

/// Elements whose content is text, not markup: read to their end tag.
const RAW_TEXT: [&str; 5] = ["script", "style", "textarea", "title", "xmp"];

/// One attribute of a start tag as the page writes it.
#[derive(Clone, Debug)]
pub(crate) struct Attr {
    /// Lowercase.
    pub name: String,
    /// As written: character references not decoded yet.
    pub value: String,
    /// Where the value is, its quotes too (None: an attribute without a value).
    pub raw: Option<Range<usize>>,
    /// The whole attribute, from its name to the end of its value.
    pub whole: Range<usize>,
}

/// A start tag: its name (lowercase) and attributes, in the order written.
#[derive(Clone, Debug)]
pub(crate) struct Tag {
    pub name: String,
    pub attrs: Vec<Attr>,
    pub self_closing: bool,
    /// Just past its name: where an attribute can be added.
    pub name_end: usize,
}

impl Tag {
    /// The first attribute named `name` (a repeated one counts once, as in browsers).
    pub fn index(&self, name: &str) -> Option<usize> {
        self.attrs.iter().position(|a| a.name == name)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.index(name).map(|i| self.attrs[i].value.as_str())
    }
}

/// What the tokenizer meets that holds links.
pub(crate) enum Piece {
    Tag(Tag),
    /// The text of a `<style>` element (CSS).
    Style(Range<usize>),
}

/// Every start tag of `page`, and the text of its `<style>` elements, in order.
pub(crate) fn scan(page: &[u8], mut each: impl FnMut(Piece)) {
    let mut at = 0;
    while let Some(i) = find_byte(&page[at..], b'<') {
        let start = at + i;
        let rest = &page[start..];
        if rest.starts_with(b"<!--") {
            at = find(&page[start + 4..], b"-->").map_or(page.len(), |e| start + 4 + e + 3);
            continue;
        }
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") || rest.starts_with(b"</") {
            // A doctype, CDATA, a processing instruction or an end tag: nothing to read.
            at = find_byte(&page[start + 1..], b'>').map_or(page.len(), |e| start + 1 + e + 1);
            continue;
        }
        // A start tag is `<` then a letter; anything else is text.
        if !page.get(start + 1).is_some_and(|b| b.is_ascii_alphabetic()) {
            at = start + 1;
            continue;
        }
        let (tag, end) = read_tag(page, start + 1);
        at = end;
        let raw_text = RAW_TEXT.contains(&tag.name.as_str()) && !tag.self_closing;
        let name = tag.name.clone();
        each(Piece::Tag(tag));
        if raw_text {
            let close = format!("</{name}");
            let body_end = find_ci(&page[at..], close.as_bytes()).map_or(page.len(), |e| at + e);
            if name == "style" {
                each(Piece::Style(at..body_end));
            }
            at = find_byte(&page[body_end..], b'>').map_or(page.len(), |e| body_end + e + 1);
        }
    }
}

/// Every link of the HTML `page`, in the order written.
pub fn html_links(page: &str) -> PageLinks {
    let mut out = PageLinks::default();
    let bytes = page.as_bytes();
    scan(bytes, |piece| match piece {
        Piece::Tag(tag) => on_tag(&tag, &mut out),
        Piece::Style(r) => {
            for (href, kind) in css_links(&String::from_utf8_lossy(&bytes[r])) {
                out.push(&href, kind);
            }
        }
    });
    out
}

/// The tag whose name starts at byte `from` (just past `<`), and where it ends (past `>`).
fn read_tag(page: &[u8], from: usize) -> (Tag, usize) {
    let b = page;
    let text = |r: Range<usize>| String::from_utf8_lossy(&page[r]).into_owned();
    let mut i = from;
    while i < b.len() && !(b[i].is_ascii_whitespace() || b[i] == b'>' || b[i] == b'/') {
        i += 1;
    }
    let name = text(from..i).to_ascii_lowercase();
    let name_end = i;
    let mut attrs: Vec<Attr> = Vec::new();
    let mut self_closing = false;
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            self_closing = b[i] == b'/';
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
        self_closing = false;
        // A name runs to whitespace, `=`, `>` or `/` (a first `=` is part of it).
        let k0 = i;
        i += 1;
        while i < b.len() && !(b[i].is_ascii_whitespace() || matches!(b[i], b'=' | b'>' | b'/')) {
            i += 1;
        }
        let key = text(k0..i).to_ascii_lowercase();
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = String::new();
        let mut raw = None;
        if j < b.len() && b[j] == b'=' {
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            match b.get(j) {
                Some(&q @ (b'"' | b'\'')) => {
                    let v0 = j + 1;
                    let close = find_byte(&page[v0..], q).map_or(page.len(), |e| v0 + e);
                    value = text(v0..close);
                    let end = (close + 1).min(page.len());
                    raw = Some(j..end);
                    j = end;
                }
                Some(_) => {
                    let v0 = j;
                    while j < b.len() && !(b[j].is_ascii_whitespace() || b[j] == b'>') {
                        j += 1;
                    }
                    value = text(v0..j);
                    raw = Some(v0..j);
                }
                None => {}
            }
            i = j;
        }
        attrs.push(Attr {
            name: key,
            value,
            raw,
            whole: k0..i,
        });
    }
    (
        Tag {
            name,
            attrs,
            self_closing,
            name_end,
        },
        i,
    )
}

/// What an attribute's value holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holds {
    /// One address: a link of this kind.
    Url(LinkKind),
    /// One address the page uses but that is not followed (a form's action, a canonical link):
    /// an offline copy only makes it absolute, so it still works.
    Address,
    /// `srcset`'s addresses (pictures).
    Srcset,
    /// CSS (a `style` attribute).
    Css,
    /// `<meta http-equiv=refresh>`'s content (a wait, then an address).
    Refresh,
    /// A picture the page's metadata names (`og:image`): collected, left as it is in a copy.
    Meta,
}

/// The attributes of `t` that hold addresses (their indexes), and what they hold; in the order
/// links are listed: the tag's own first, then `srcset`, `style` and `background`.
pub(crate) fn values(t: &Tag) -> Vec<(usize, Holds)> {
    use Holds::*;
    use LinkKind::*;
    let mut out = Vec::new();
    let mut put = |attr: &str, holds: Holds| {
        if let Some(i) = t.index(attr) {
            out.push((i, holds));
        }
    };
    match t.name.as_str() {
        "a" | "area" => put("href", Url(Page)),
        "frame" | "iframe" => put("src", Url(Page)),
        "img" => {
            put("src", Url(Resource));
            // Pictures loaded late by a script keep their address here.
            for lazy in LAZY {
                put(lazy, Url(Resource));
            }
        }
        "video" => {
            put("src", Url(Resource));
            put("poster", Url(Resource));
        }
        "audio" | "track" | "embed" | "source" | "script" => put("src", Url(Resource)),
        "object" => put("data", Url(Resource)),
        // An SVG's pictures.
        "image" => {
            put("href", Url(Resource));
            put("xlink:href", Url(Resource));
        }
        "input"
            if t.attr("type")
                .is_some_and(|v| v.eq_ignore_ascii_case("image")) =>
        {
            put("src", Url(Resource));
            put("formaction", Address);
        }
        "input" | "button" => put("formaction", Address),
        "form" => put("action", Address),
        "link" => {
            let rel = t.attr("rel").unwrap_or_default().to_ascii_lowercase();
            let rels: Vec<&str> = rel.split_ascii_whitespace().collect();
            let holds = if rels.contains(&"stylesheet") {
                Url(Style)
            } else if rels.iter().any(|r| {
                matches!(
                    *r,
                    "icon"
                        | "apple-touch-icon"
                        | "preload"
                        | "prefetch"
                        | "image_src"
                        | "enclosure"
                )
            }) {
                Url(Resource)
            } else if rels
                .iter()
                .any(|r| matches!(*r, "next" | "prev" | "alternate"))
            {
                Url(Page)
            } else {
                Address
            };
            put("href", holds);
        }
        "meta" => {
            let equiv = t.attr("http-equiv").unwrap_or_default();
            let property = t
                .attr("property")
                .or_else(|| t.attr("name"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if equiv.eq_ignore_ascii_case("refresh") {
                put("content", Refresh);
            } else if matches!(
                property.as_str(),
                "og:image" | "og:video" | "og:audio" | "twitter:image"
            ) {
                put("content", Meta);
            }
        }
        _ => {}
    }
    // Any element: its pictures in `srcset`, `style` and the old `background`.
    put("srcset", Srcset);
    put("data-srcset", Srcset);
    put("style", Css);
    put("background", Url(Resource));
    out
}

/// The attributes of `<img>` where scripts that load pictures late keep their address.
pub(crate) const LAZY: [&str; 3] = ["data-src", "data-original", "data-lazy-src"];

/// The links tag `t` carries.
fn on_tag(t: &Tag, out: &mut PageLinks) {
    if t.name == "base" {
        if out.base.is_none()
            && let Some(v) = t.attr("href")
        {
            out.base = Some(decode(v.trim()));
        }
        return;
    }
    for (i, holds) in values(t) {
        let v = t.attrs[i].value.as_str();
        match holds {
            Holds::Url(kind) => out.push(v, kind),
            Holds::Meta => out.push(v, LinkKind::Resource),
            Holds::Address => {}
            Holds::Srcset => {
                for href in srcset(&decode(v)) {
                    out.push(&href, LinkKind::Resource);
                }
            }
            Holds::Css => {
                for (href, kind) in css_links(&decode(v)) {
                    out.push(&href, kind);
                }
            }
            Holds::Refresh => {
                if let Some((_, to)) = refresh_target(v) {
                    out.push(to, LinkKind::Page);
                }
            }
        }
    }
}

/// Where `<meta http-equiv=refresh content="5; url=…">` sends the browser: the address, and
/// where it starts in `content`.
pub(crate) fn refresh_target(content: &str) -> Option<(usize, &str)> {
    let (_, rest) = content.split_once([';', ','])?;
    let rest = rest.trim_start();
    let lower = rest.to_ascii_lowercase();
    let rest = if lower.starts_with("url") {
        rest[3..].trim_start().strip_prefix('=')?.trim_start()
    } else {
        rest
    };
    let rest = rest.trim_matches(|c| c == '"' || c == '\'');
    if rest.is_empty() {
        return None;
    }
    let at = rest.as_ptr() as usize - content.as_ptr() as usize;
    Some((at, rest))
}

/// The candidates of a `srcset` (`a.jpg 1x, b.jpg 2x`, `c.jpg 640w`), as the HTML standard
/// splits it: each address and its descriptors (`2x`, empty when none). An address may hold
/// commas, a descriptor list ends at a comma.
pub(crate) fn srcset_candidates(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = value;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.is_empty() {
            return out;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let mut url = &rest[..end];
        rest = &rest[end..];
        let mut descriptors = "";
        if url.ends_with(',') {
            // No descriptors: the address ends at its commas.
            url = url.trim_end_matches(',');
        } else {
            // Descriptors up to the next comma outside parentheses.
            let mut depth = 0;
            let mut stop = rest.len();
            for (i, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ',' if depth <= 0 => {
                        stop = i;
                        break;
                    }
                    _ => {}
                }
            }
            descriptors = rest[..stop].trim();
            rest = &rest[stop..];
        }
        if !url.is_empty() {
            out.push((url.to_owned(), descriptors.to_owned()));
        }
    }
}

/// The addresses of a `srcset` (`a.jpg 1x, b.jpg 2x`, `c.jpg 640w`), as the HTML standard
/// splits it: an address may hold commas, a descriptor list ends at a comma.
pub fn srcset(value: &str) -> Vec<String> {
    srcset_candidates(value)
        .into_iter()
        .map(|(url, _)| url)
        .collect()
}

/// One address in CSS text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CssUrl {
    /// Its escapes undone.
    pub href: String,
    pub kind: LinkKind,
    /// The whole of it as written: `url(…)`, or an `@import`'s quoted string.
    pub token: Range<usize>,
    /// Written as `url(…)` (else an `@import`'s string).
    pub is_url: bool,
}

/// The addresses in CSS text, with where each is: `url(…)` (quoted or not) and `@import "…"`;
/// comments skipped, `data:` addresses left out (they are no file to fetch).
pub(crate) fn css_urls(css: &str) -> Vec<CssUrl> {
    // Comments blanked, the same length: the places found are the text's own.
    let blank = blank_css_comments(css);
    let lower = blank.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("url(") {
        let token_start = at + i;
        let open = token_start + 4;
        let inner = blank[open..].trim_start();
        let skipped = blank[open..].len() - inner.len();
        let (url, used, close_paren) = match inner.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let close = inner[1..].find(q).map_or(inner.len(), |e| e + 1);
                let after = (close + 1).min(inner.len());
                // The `)` after the closing quote.
                let paren = inner[after..].find(')').map(|p| after + p);
                (&inner[1..close], after, paren)
            }
            _ => {
                let close = inner.find(')');
                let end = close.unwrap_or(inner.len());
                (inner[..end].trim_end(), end, close)
            }
        };
        // An `@import url(…)` is a style sheet too.
        let before = lower[..token_start].trim_end();
        let kind = if before.ends_with("@import") {
            LinkKind::Style
        } else {
            LinkKind::Resource
        };
        let token_end = match close_paren {
            Some(p) => open + skipped + p + 1,
            None => css.len(),
        };
        let href = unescape_css(url.trim());
        if !href.is_empty() && !href.to_ascii_lowercase().starts_with("data:") {
            out.push(CssUrl {
                href,
                kind,
                token: token_start..token_end,
                is_url: true,
            });
        }
        at = (open + skipped + used).min(css.len());
    }
    // `@import "x.css";` without url().
    let mut at = 0;
    while let Some(i) = lower[at..].find("@import") {
        let after = at + i + 7;
        let rest = blank[after..].trim_start();
        let rest_at = blank.len() - rest.len();
        if let Some(q @ ('"' | '\'')) = rest.chars().next()
            && let Some(close) = rest[1..].find(q)
        {
            let href = unescape_css(&rest[1..1 + close]);
            if !href.is_empty() {
                out.push(CssUrl {
                    href,
                    kind: LinkKind::Style,
                    token: rest_at..rest_at + close + 2,
                    is_url: false,
                });
            }
        }
        at = after;
    }
    out
}

/// The addresses in CSS text: `url(…)` (quoted or not) and `@import "…"`; comments skipped,
/// `data:` addresses left out (they are no file to fetch).
pub fn css_links(css: &str) -> Vec<(String, LinkKind)> {
    css_urls(css)
        .into_iter()
        .map(|u| (u.href, u.kind))
        .collect()
}

/// `css` with each comment's characters made spaces (the same length in bytes).
fn blank_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        let comment_len = rest[i + 2..].find("*/").map_or(rest.len() - i, |e| e + 4);
        out.extend(std::iter::repeat_n(' ', comment_len));
        rest = &rest[i + comment_len..];
    }
    out.push_str(rest);
    out
}

/// CSS's backslash escapes in an address (`\(` and the like); hex escapes are rare in addresses
/// and left as they are.
fn unescape_css(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n) if !n.is_ascii_hexdigit() => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn find_byte(hay: &[u8], b: u8) -> Option<usize> {
    hay.iter().position(|&x| x == b)
}

/// Byte offset of `needle` in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Byte offset of `needle` (ASCII) in `hay`, in any case.
fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let n = needle.len();
    if n == 0 || hay.len() < n {
        return None;
    }
    (0..=hay.len() - n).find(|&i| hay[i..i + n].eq_ignore_ascii_case(needle))
}

/// `s` with its character references decoded: the named ones addresses use (`&amp; &lt; &gt;
/// &quot; &apos; &nbsp;`), `&#NN;` and `&#xNN;`. Anything else stays as written.
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let reference = rest[1..].find(';').filter(|&n| n <= 10).and_then(|n| {
            let name = &rest[1..1 + n];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => match name.strip_prefix('#') {
                    Some(hex) if hex.starts_with(['x', 'X']) => u32::from_str_radix(&hex[1..], 16)
                        .ok()
                        .and_then(char::from_u32),
                    Some(dec) => dec.parse().ok().and_then(char::from_u32),
                    None => None,
                },
            };
            c.map(|c| (c, n + 2))
        });
        match reference {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `href` resolved against `base` as a browser would, without its fragment: only `http` and
/// `https` addresses (no `mailto:`, `javascript:` or `data:`).
pub fn resolve(base: &Url, href: &str) -> Option<Url> {
    let mut url = base.join(href.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hrefs(page: &str) -> Vec<(String, LinkKind)> {
        html_links(page)
            .links
            .into_iter()
            .map(|l| (l.href, l.kind))
            .collect()
    }

    #[test]
    fn tags_and_attributes_are_read_as_browsers_read_them() {
        use LinkKind::*;
        let page = r#"<!doctype html><HTML><head>
            <base href="https://cdn.example/root/">
            <link rel="stylesheet" href="/s.css"><link rel="shortcut icon" href=fav.ico>
            <link rel=canonical href="/ignored">
            <meta http-equiv="Refresh" content="5; URL='/next.html'">
            <meta property="og:image" content="/og.jpg">
            </head><body background=bg.gif>
            <A HREF='a.html?x=1&amp;y=2#top' title="1 > 0">A</A>
            <a href=b.zip>B</a>
            <img data-src="lazy.png" src = "pic.jpg" srcset="s1.jpg 1x, s2.jpg 2x">
            <video poster="p.jpg"><source src="v.mp4" type="video/mp4"></video>
            <audio src="s.mp3"></audio>
            <div style="background: url(&quot;d.png&quot;)"></div>
            <iframe src="frame.html"></iframe>
            <input type=image src="go.gif"><input type=text src="no.gif">
            <form action="/search"><button formaction="/other">Go</button></form>
            <svg><image href="vector.png"/></svg>
            </body></html>"#;
        let out = html_links(page);
        assert_eq!(out.base.as_deref(), Some("https://cdn.example/root/"));
        let got: Vec<(String, LinkKind)> =
            out.links.into_iter().map(|l| (l.href, l.kind)).collect();
        let want = [
            ("/s.css", Style),
            ("fav.ico", Resource),
            ("/next.html", Page),
            ("/og.jpg", Resource),
            ("bg.gif", Resource),
            ("a.html?x=1&y=2#top", Page),
            ("b.zip", Page),
            ("pic.jpg", Resource),
            ("lazy.png", Resource),
            ("s1.jpg", Resource),
            ("s2.jpg", Resource),
            ("p.jpg", Resource),
            ("v.mp4", Resource),
            ("s.mp3", Resource),
            ("d.png", Resource),
            ("frame.html", Page),
            ("go.gif", Resource),
            ("vector.png", Resource),
        ];
        let want: Vec<(String, LinkKind)> = want.iter().map(|(h, k)| (h.to_string(), *k)).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn comments_scripts_and_text_areas_hold_no_links() {
        let page = r#"<p>1 < 2</p><!-- <a href="commented.html"> -->
            <script>document.write('<a href="scripted.html">')</script>
            <script src="real.js"></script>
            <textarea><img src="typed.png"></textarea>
            <title><a href="t.html"></title>
            <style>body { background: url('styled.png') } /* url(gone.png) */</style>
            <a href="kept.html">"#;
        let got = hrefs(page);
        let names: Vec<&str> = got.iter().map(|(h, _)| h.as_str()).collect();
        assert_eq!(names, ["real.js", "styled.png", "kept.html"]);
    }

    #[test]
    fn a_broken_page_ends_without_a_panic() {
        for page in [
            "<a href=",
            "<a href=\"x",
            "<",
            "<!--",
            "<script>",
            "<img src='é.png'",
            "<a\u{a0}href=x>",
            "<<a href=ok.html>",
            "<style>url(",
            "<style>url('x",
            "<style>@import '",
            "<style>/* unclosed",
        ] {
            let _ = html_links(page);
        }
        assert_eq!(hrefs("<<a href=ok.html>")[0].0, "ok.html");
        // Bytes that aren't UTF-8 (an older encoding) don't stop it either.
        let mut latin1 = b"<a href=\"caf".to_vec();
        latin1.extend([0xe9, b'.', b'h', b't', b'm', b'l', b'"', b'>']);
        let mut n = 0;
        scan(&latin1, |_| n += 1);
        assert_eq!(n, 1);
    }

    #[test]
    fn attribute_places_are_the_pages_own() {
        let page = br#"<img  SRC = 'a.png' alt=x data-src=b.png checked>"#;
        let mut tags = Vec::new();
        scan(page, |p| {
            if let Piece::Tag(t) = p {
                tags.push(t);
            }
        });
        let t = &tags[0];
        let raw = |name: &str| {
            let a = &t.attrs[t.index(name).unwrap()];
            std::str::from_utf8(&page[a.raw.clone().unwrap()]).unwrap()
        };
        assert_eq!(raw("src"), "'a.png'");
        assert_eq!(raw("data-src"), "b.png");
        let checked = &t.attrs[t.index("checked").unwrap()];
        assert!(checked.raw.is_none());
        assert_eq!(&page[checked.whole.clone()], b"checked");
        assert_eq!(&page[t.name_end..t.name_end + 2], b"  ");
    }

    #[test]
    fn srcset_candidates_are_split_as_the_standard_says() {
        assert_eq!(srcset("a.jpg 1x, b.jpg 2x"), ["a.jpg", "b.jpg"]);
        // A URL runs to the next whitespace, commas and all; only trailing commas end it.
        assert_eq!(srcset(" a.jpg,b.jpg 640w ,c.jpg"), ["a.jpg,b.jpg", "c.jpg"]);
        assert_eq!(srcset("a.jpg, b.jpg,, c.jpg"), ["a.jpg", "b.jpg", "c.jpg"]);
        assert_eq!(
            srcset("img,w=300.jpg 300w, img,w=600.jpg 600w"),
            ["img,w=300.jpg", "img,w=600.jpg"]
        );
        assert!(srcset("  ,  ").is_empty());
        assert_eq!(
            srcset_candidates("a.jpg 1x, b.jpg 2x , c.jpg"),
            [
                ("a.jpg".to_owned(), "1x".to_owned()),
                ("b.jpg".to_owned(), "2x".to_owned()),
                ("c.jpg".to_owned(), String::new())
            ]
        );
    }

    #[test]
    fn css_urls_and_imports() {
        let css = r#"@import "base.css"; @import url(print.css) print;
            .a { background: url( "x y.png" ) } .b{background:URL(z.gif)}
            .c { background: url(data:image/png;base64,AAAA) }
            .d { background: url('esc\(1\).png') }"#;
        let got = css_links(css);
        let want = [
            ("print.css", LinkKind::Style),
            ("x y.png", LinkKind::Resource),
            ("z.gif", LinkKind::Resource),
            ("esc(1).png", LinkKind::Resource),
            ("base.css", LinkKind::Style),
        ];
        let want: Vec<(String, LinkKind)> = want.iter().map(|(h, k)| (h.to_string(), *k)).collect();
        assert_eq!(got, want);
        // Each one's place: the whole `url(…)`, or the import's quoted string.
        let places: Vec<&str> = css_urls(css)
            .iter()
            .map(|u| &css[u.token.clone()])
            .collect();
        assert_eq!(
            places,
            [
                "url(print.css)",
                r#"url( "x y.png" )"#,
                "URL(z.gif)",
                r"url('esc\(1\).png')",
                r#""base.css""#
            ]
        );
        // A comment keeps its length: what follows is found where it is.
        let commented = "/* url(gone.png) */ .x{background:url(kept.png)}";
        let found = css_urls(commented);
        assert_eq!(found.len(), 1);
        assert_eq!(&commented[found[0].token.clone()], "url(kept.png)");
    }

    #[test]
    fn character_references_are_decoded() {
        assert_eq!(decode("a&amp;b&#47;c&#x2F;d&bogus;e&"), "a&b/c/d&bogus;e&");
    }

    #[test]
    fn refresh_targets_and_where_they_start() {
        let c = "5; URL='/next.html'";
        let (at, to) = refresh_target(c).unwrap();
        assert_eq!(to, "/next.html");
        assert_eq!(&c[at..at + to.len()], "/next.html");
        assert_eq!(refresh_target("0;x.html").map(|t| t.1), Some("x.html"));
        assert!(refresh_target("30").is_none());
    }

    #[test]
    fn addresses_resolve_against_the_page_and_lose_their_fragment() {
        let base = Url::parse("https://site.example/dir/page.html?q=1").unwrap();
        let r = |h: &str| resolve(&base, h).map(|u| u.to_string());
        assert_eq!(
            r("img/a.png").as_deref(),
            Some("https://site.example/dir/img/a.png")
        );
        assert_eq!(
            r("../up.zip").as_deref(),
            Some("https://site.example/up.zip")
        );
        assert_eq!(
            r("/root.pdf#p2").as_deref(),
            Some("https://site.example/root.pdf")
        );
        assert_eq!(
            r("//cdn.example/x.jpg").as_deref(),
            Some("https://cdn.example/x.jpg")
        );
        assert_eq!(
            r("?page=2").as_deref(),
            Some("https://site.example/dir/page.html?page=2")
        );
        assert_eq!(
            r(" \n spaced.png\t").as_deref(),
            Some("https://site.example/dir/spaced.png")
        );
        // Letters beyond ASCII are percent-encoded, as a browser sends them.
        assert_eq!(
            r("café/menü.html").as_deref(),
            Some("https://site.example/dir/caf%C3%A9/men%C3%BC.html")
        );
        for skipped in [
            "mailto:a@b.c",
            "javascript:void(0)",
            "data:image/png;base64,AA",
            "ftp://x/y",
            "#top",
        ] {
            let got = r(skipped);
            assert!(got.is_none() || skipped == "#top", "{skipped} gave {got:?}");
        }
        // A fragment alone is the page itself.
        assert_eq!(
            r("#top").as_deref(),
            Some("https://site.example/dir/page.html?q=1")
        );
    }
}
