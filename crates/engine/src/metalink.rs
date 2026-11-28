//! Metalink 3 and 4 manifests (metalinker.org's 3.0, RFC 5854): the files they describe, each
//! with its sources (priority, location), size, strongest whole-file hash and piece hashes.
//!
//! A manifest is data, not a web page. This reader only parses its XML: it never follows a
//! link, expands an entity, or honours a local path from the document. Callers decide which
//! listed sources to use, verify the result with [`File::checksum`] and [`File::pieces`], and
//! place the file under their own folder with [`File::relative_path`].

use std::collections::HashSet;
use std::path::PathBuf;

use roxmltree::{Document, Node};
use url::Url;

use crate::verify::{Algo, Hash, Pieces};

/// The most a manifest may be: piece hashes of a few large files fit many times over; anything
/// bigger isn't a manifest, and would be read whole into memory.
pub const MAX_MANIFEST: u64 = 16 << 20;

/// The most files one manifest may describe, and sources one file may list: beyond them it is
/// no publisher's list.
const MAX_FILES: usize = 10_000;
const MAX_SOURCES: usize = 256;

/// The priority of a source that gives none: after every source that does (RFC 5854 §4.2.16:
/// 1 is the most preferred, 999999 the least).
const NO_PRIORITY: u32 = 999_999;

/// Why a document is no usable manifest. In English for the log and the command line; the app
/// says each in the interface language.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MetalinkError {
    #[error("the Metalink is larger than {} MiB", MAX_MANIFEST >> 20)]
    TooLarge,
    #[error("the Metalink is not UTF-8 text")]
    NotText,
    #[error("invalid Metalink XML: {0}")]
    InvalidXml(String),
    #[error("the Metalink has no files")]
    NoFiles,
    #[error("the Metalink describes more than {MAX_FILES} files")]
    TooManyFiles,
    #[error("the Metalink's {file} has no HTTP, HTTPS or FTP address")]
    NoSource { file: String },
    /// A name that climbs out of the folder it is saved in, or starts at a drive's root
    /// (RFC 5854 §4.1.2.1: such a document is refused, not repaired).
    #[error("the Metalink's file name {name} would be saved outside its folder")]
    UnsafeName { name: String },
}

/// A Metalink document. One document may describe several files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub files: Vec<File>,
}

/// One place a file can be downloaded from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub url: Url,
    /// Lower is preferred (Metalink 4's `priority`; Metalink 3's `preference`, where higher is
    /// preferred, turned around); [`NO_PRIORITY`] when the document gives none.
    pub priority: u32,
    /// The server's country (ISO 3166-1 alpha-2, upper case), when given.
    pub location: Option<String>,
}

/// One file described by a Metalink document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The publisher's file name, when the document supplied one: it may name folders
    /// (`docs/manual.pdf`); [`File::relative_path`] is where it goes.
    pub name: Option<String>,
    /// The expected byte size, when supplied.
    pub size: Option<u64>,
    /// Download locations, in the publisher's preference order (priority, then the document's
    /// order). Duplicate or unsupported locations are removed before the list reaches the
    /// downloader.
    pub urls: Vec<Url>,
    /// The same locations with their priority and country ([`File::ordered_urls`]).
    pub sources: Vec<Source>,
    /// The strongest whole-file hash that DDM understands.
    pub checksum: Option<Hash>,
    /// The strongest piece hashes that DDM understands, when they cover `size` exactly.
    pub pieces: Option<Pieces>,
}

impl File {
    /// The locations in the order to use them: the publisher's priority first; among sources
    /// of one priority, those in `country` (the user's, ISO 3166-1 alpha-2) before the others;
    /// then the document's order.
    pub fn ordered_urls(&self, country: Option<&str>) -> Vec<Url> {
        let mut ranked: Vec<(usize, &Source)> = self.sources.iter().enumerate().collect();
        ranked.sort_by_key(|(i, s)| {
            let here = match (country, s.location.as_deref()) {
                (Some(c), Some(l)) => !c.eq_ignore_ascii_case(l),
                _ => true,
            };
            (s.priority, here, *i)
        });
        ranked.into_iter().map(|(_, s)| s.url.clone()).collect()
    }

    /// Where the file goes under the folder it is saved in: the name's folders and file name,
    /// each made safe for this system (`naming::sanitize`). None without a name.
    pub fn relative_path(&self) -> Option<PathBuf> {
        let name = self.name.as_deref()?;
        let parts: Vec<String> = components(name)
            .map(crate::naming::sanitize)
            .filter(|p| !p.is_empty())
            .collect();
        (!parts.is_empty()).then(|| parts.iter().collect())
    }

    /// The name shown for the file: its own file name (without its folders), else the last part
    /// of its first address.
    pub fn display_name(&self) -> String {
        self.relative_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .or_else(|| {
                self.urls
                    .first()
                    .map(|u| crate::naming::sanitize(&crate::naming::file_name(None, u)))
            })
            .unwrap_or_else(|| "download".to_owned())
    }
}

/// The parts of a manifest's file name, split at either slash, `.` parts left out.
fn components(name: &str) -> impl Iterator<Item = &str> {
    name.split(['/', '\\'])
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != ".")
}

/// Whether a manifest's file name stays inside the folder it is saved in: not absolute (a
/// leading slash, a drive or a share), no `..` part.
fn safe_name(name: &str) -> bool {
    let name = name.trim();
    let bytes = name.as_bytes();
    let drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    !(name.starts_with(['/', '\\']) || drive || components(name).any(|p| p == ".."))
}

/// Reads a manifest's bytes (a UTF-8 byte order mark allowed): [`parse`] after the size and
/// encoding checks.
pub fn parse_bytes(bytes: &[u8], source: &Url) -> Result<Manifest, MetalinkError> {
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(MetalinkError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let xml = std::str::from_utf8(bytes).map_err(|_| MetalinkError::NotText)?;
    parse(xml, source)
}

/// Reads a Metalink v3 or v4 XML document.
///
/// Relative `url` values resolve relative to the manifest itself (`source`). A valid manifest
/// needs at least one downloadable URL for every advertised file; this prevents accidentally
/// adding an empty or HTML error document as a download manifest.
pub fn parse(xml: &str, source: &Url) -> Result<Manifest, MetalinkError> {
    let doc = Document::parse(xml).map_err(|e| MetalinkError::InvalidXml(e.to_string()))?;
    let nodes: Vec<_> = doc
        .descendants()
        .filter(|n| element_named(*n, "file"))
        .collect();
    if nodes.len() > MAX_FILES {
        return Err(MetalinkError::TooManyFiles);
    }
    let files: Vec<_> = nodes
        .into_iter()
        .map(|n| parse_file(n, source))
        .collect::<Result<_, _>>()?;
    if files.is_empty() {
        return Err(MetalinkError::NoFiles);
    }
    Ok(Manifest { files })
}

fn parse_file(file: Node<'_, '_>, source: &Url) -> Result<File, MetalinkError> {
    let name = file
        .attribute("name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    if let Some(name) = &name
        && !safe_name(name)
    {
        return Err(MetalinkError::UnsafeName { name: name.clone() });
    }
    let size = file
        .descendants()
        .find(|n| element_named(*n, "size"))
        .and_then(text)
        .and_then(|s| s.parse().ok());

    let mut ordered = Vec::new();
    for (index, node) in file
        .descendants()
        .filter(|n| element_named(*n, "url"))
        .enumerate()
    {
        // Metalink 3's `type="bittorrent"` points at a .torrent, not at the file.
        if let Some(kind) = node.attribute("type")
            && !matches!(
                kind.trim().to_ascii_lowercase().as_str(),
                "http" | "https" | "ftp" | "ftps"
            )
        {
            continue;
        }
        let Some(raw) = text(node) else { continue };
        let Ok(url) = source.join(raw) else { continue };
        if !crate::supported(&url) {
            continue;
        }
        // Metalink 4: the lower `priority` wins. Metalink 3: the higher `preference` (0-100).
        let priority = node
            .attribute("priority")
            .and_then(|n| n.trim().parse::<u32>().ok())
            .filter(|p| *p > 0)
            .or_else(|| {
                node.attribute("preference")
                    .and_then(|n| n.trim().parse::<u32>().ok())
                    .map(|p| 101 - p.min(100))
            })
            .unwrap_or(NO_PRIORITY);
        let location = node
            .attribute("location")
            .map(str::trim)
            .filter(|l| l.len() == 2 && l.chars().all(|c| c.is_ascii_alphabetic()))
            .map(str::to_ascii_uppercase);
        ordered.push((
            index,
            Source {
                url,
                priority,
                location,
            },
        ));
    }
    ordered.sort_by_key(|(index, s)| (s.priority, *index));
    let mut seen = HashSet::new();
    let sources: Vec<Source> = ordered
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| seen.insert(s.url.to_string()))
        .take(MAX_SOURCES)
        .collect();
    if sources.is_empty() {
        let shown = name.as_deref().unwrap_or("file").to_owned();
        return Err(MetalinkError::NoSource { file: shown });
    }

    // Whole-file hashes: those not inside `<pieces>`.
    let checksum = file
        .descendants()
        .filter(|n| element_named(*n, "hash"))
        .filter(|n| !n.parent().is_some_and(|p| element_named(p, "pieces")))
        .filter_map(parse_hash)
        .max_by_key(|h| strength(h.algo));
    let pieces = file
        .descendants()
        .filter(|n| element_named(*n, "pieces"))
        .filter_map(parse_pieces)
        .filter(|p| size.is_some_and(|s| p.fits(s)))
        .max_by_key(|p| strength(p.algo));
    Ok(File {
        name,
        size,
        urls: sources.iter().map(|s| s.url.clone()).collect(),
        sources,
        checksum,
        pieces,
    })
}

fn element_named(node: Node<'_, '_>, wanted: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(wanted)
}

fn text<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    let text = node.text()?.trim();
    (!text.is_empty()).then_some(text)
}

fn algo_named(kind: &str) -> Option<Algo> {
    Some(match kind.trim().to_ascii_lowercase().as_str() {
        "crc32" => Algo::Crc32,
        "md5" => Algo::Md5,
        "sha1" | "sha-1" => Algo::Sha1,
        "sha256" | "sha-256" => Algo::Sha256,
        "sha512" | "sha-512" => Algo::Sha512,
        _ => return None,
    })
}

fn hex_of(node: Node<'_, '_>) -> Option<Vec<u8>> {
    let hex: String = text(node)?.chars().filter(|c| !c.is_whitespace()).collect();
    hex::decode(hex).ok()
}

fn parse_hash(node: Node<'_, '_>) -> Option<Hash> {
    let algo = algo_named(node.attribute("type")?)?;
    let hex = hex::encode(hex_of(node)?);
    let name = algo.name().to_ascii_lowercase().replace('-', "");
    Hash::parse(&format!("{name}:{hex}"))
}

/// A `<pieces length="…" type="…">` element's hashes, in piece order (Metalink 3 numbers them,
/// `piece="0"`; Metalink 4 lists them in order). None when any is missing or malformed.
fn parse_pieces(node: Node<'_, '_>) -> Option<Pieces> {
    let algo = algo_named(node.attribute("type")?)?;
    let length = node.attribute("length")?.trim().parse::<u64>().ok()?;
    let mut hashes: Vec<(usize, Vec<u8>)> = Vec::new();
    for (i, h) in node
        .children()
        .filter(|n| element_named(*n, "hash"))
        .enumerate()
    {
        let at = match h.attribute("piece") {
            Some(p) => p.trim().parse().ok()?,
            None => i,
        };
        hashes.push((at, hex_of(h)?));
    }
    hashes.sort_by_key(|(at, _)| *at);
    if hashes.iter().enumerate().any(|(i, (at, _))| i != *at) {
        return None;
    }
    let digests: Vec<Vec<u8>> = hashes.into_iter().map(|(_, d)| d).collect();
    Pieces::new(length, algo, &digests)
}

fn strength(algo: Algo) -> u8 {
    match algo {
        Algo::Crc32 => 0,
        Algo::Md5 => 1,
        Algo::Sha1 => 2,
        Algo::Sha256 => 3,
        Algo::Sha512 => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::Algo;

    fn source() -> Url {
        Url::parse("https://publisher.example/releases/app.meta4").unwrap()
    }

    #[test]
    fn v4_sorts_mirrors_deduplicates_and_keeps_the_hash() {
        let xml = r#"
            <metalink xmlns="urn:ietf:params:xml:ns:metalink">
              <file name="app.zip">
                <size>42</size>
                <hash type="sha-256">d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592</hash>
                <url priority="2">https://slow.example/app.zip</url>
                <url priority="1">files/app.zip</url>
                <url priority="3">https://slow.example/app.zip</url>
                <url>magnet:?xt=urn:btih:not-a-file-source</url>
                <metaurl mediatype="torrent">https://publisher.example/app.zip.torrent</metaurl>
              </file>
            </metalink>
        "#;
        let manifest = parse(xml, &source()).unwrap();
        let file = &manifest.files[0];
        assert_eq!(file.name.as_deref(), Some("app.zip"));
        assert_eq!(file.size, Some(42));
        assert_eq!(
            file.urls[0].as_str(),
            "https://publisher.example/releases/files/app.zip"
        );
        assert_eq!(file.urls[1].host_str(), Some("slow.example"));
        assert_eq!(file.urls.len(), 2, "the torrent's metaurl is no source");
        assert_eq!(file.checksum.as_ref().unwrap().algo, Algo::Sha256);
        assert_eq!(file.pieces, None);
    }

    #[test]
    fn legacy_documents_choose_the_strongest_known_hash() {
        let xml = r#"
            <metalink version="3.0">
              <files><file name="app.tar.xz">
                <verification>
                  <hash type="md5">9e107d9d372bb6826bd81d3542a419d6</hash>
                  <hash type="SHA-512">07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb64
                    2e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6</hash>
                </verification>
                <resources><url type="https">https://cdn.example/app.tar.xz</url></resources>
              </file></files>
            </metalink>
        "#;
        let manifest = parse(xml, &source()).unwrap();
        assert_eq!(
            manifest.files[0].checksum.as_ref().unwrap().algo,
            Algo::Sha512
        );
    }

    #[test]
    fn rejects_a_manifest_that_cannot_download_its_file() {
        let xml =
            r#"<metalink><file name="empty"><url>magnet:?xt=urn:btih:x</url></file></metalink>"#;
        assert_eq!(
            parse(xml, &source()).unwrap_err(),
            MetalinkError::NoSource {
                file: "empty".into()
            }
        );
        assert_eq!(
            parse("<metalink/>", &source()).unwrap_err(),
            MetalinkError::NoFiles
        );
        assert!(matches!(
            parse("<html><body>Not found", &source()).unwrap_err(),
            MetalinkError::InvalidXml(_)
        ));
    }

    /// Metalink 3 (metalinker.org): `preference` (higher first), `location`, `type`; its
    /// .torrent resource is no source of the file itself.
    #[test]
    fn v3_preference_location_and_type() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <metalink version="3.0" xmlns="http://www.metalinker.org/">
              <files>
                <file name="disc.iso">
                  <size>1000</size>
                  <resources>
                    <url type="http" location="us" preference="50">http://us.example/disc.iso</url>
                    <url type="bittorrent" preference="100">http://p.example/disc.iso.torrent</url>
                    <url type="ftp" location="de" preference="90">ftp://de.example/disc.iso</url>
                    <url type="http" location="fr" preference="50">http://fr.example/disc.iso</url>
                  </resources>
                </file>
              </files>
            </metalink>"#;
        let file = parse(xml, &source()).unwrap().files.remove(0);
        let hosts = |urls: Vec<Url>| -> Vec<String> {
            urls.iter()
                .map(|u| u.host_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(
            hosts(file.urls.clone()),
            ["de.example", "us.example", "fr.example"]
        );
        assert_eq!(file.sources[0].location.as_deref(), Some("DE"));
        // A French user: the French server before the American one of the same preference;
        // the preferred German one stays first.
        assert_eq!(
            hosts(file.ordered_urls(Some("fr"))),
            ["de.example", "fr.example", "us.example"]
        );
        assert_eq!(hosts(file.ordered_urls(None)), hosts(file.urls.clone()));
    }

    /// Metalink 4's `location` only reorders sources of the same priority.
    #[test]
    fn v4_location_breaks_ties_within_a_priority() {
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
              <file name="a.bin">
                <url priority="1" location="us">https://us.example/a.bin</url>
                <url priority="1" location="ke">https://ke.example/a.bin</url>
                <url priority="2" location="ke">https://ke2.example/a.bin</url>
                <url location="ke">https://ke3.example/a.bin</url>
              </file></metalink>"#;
        let file = parse(xml, &source()).unwrap().files.remove(0);
        let order: Vec<_> = file
            .ordered_urls(Some("KE"))
            .iter()
            .map(|u| u.host_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            order,
            ["ke.example", "us.example", "ke2.example", "ke3.example"]
        );
    }

    #[test]
    fn several_files_with_folders_and_pieces() {
        // SHA-1 of 4 bytes "abcd" and of 2 bytes "ef".
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
              <file name="set/one.txt">
                <size>6</size>
                <hash type="sha-1">1f8ac10f23c5b5bc1167bda84b833e5c057a77d2</hash>
                <pieces length="4" type="sha-1">
                  <hash>81fe8bfe87576c3ecb22426f8e57847382917acf</hash>
                  <hash>c2c2c8d3cc4ce7bc6d7a1c0d4c1a3a6d1ffbb3a3</hash>
                </pieces>
                <url>https://a.example/one.txt</url>
              </file>
              <file name="set/docs/two.txt">
                <size>9</size>
                <pieces length="4" type="sha-1"><hash>81fe8bfe87576c3ecb22426f8e57847382917acf</hash></pieces>
                <url>https://a.example/two.txt</url>
              </file>
            </metalink>"#;
        let m = parse(xml, &source()).unwrap();
        assert_eq!(m.files.len(), 2);
        let one = &m.files[0];
        assert_eq!(
            one.relative_path().unwrap(),
            PathBuf::from("set").join("one.txt")
        );
        assert_eq!(one.display_name(), "one.txt");
        let pieces = one.pieces.as_ref().unwrap();
        assert_eq!((pieces.length, pieces.count()), (4, 2));
        assert_eq!(one.checksum.as_ref().unwrap().algo, Algo::Sha1);
        // Its one piece hash can't cover 9 bytes: not taken (the whole file still verifies by
        // nothing here, which is what the document gives).
        assert_eq!(m.files[1].pieces, None);
        assert_eq!(
            m.files[1].relative_path().unwrap(),
            ["set", "docs", "two.txt"].iter().collect::<PathBuf>()
        );
    }

    #[test]
    fn v3_numbered_pieces_in_any_order() {
        let xml = r#"<metalink version="3.0"><files><file name="x"><size>6</size>
              <verification><pieces length="4" type="sha1">
                <hash piece="1">c2c2c8d3cc4ce7bc6d7a1c0d4c1a3a6d1ffbb3a3</hash>
                <hash piece="0">81fe8bfe87576c3ecb22426f8e57847382917acf</hash>
              </pieces></verification>
              <resources><url type="http">http://a.example/x</url></resources>
            </file></files></metalink>"#;
        let file = parse(xml, &source()).unwrap().files.remove(0);
        let pieces = file.pieces.unwrap();
        assert_eq!(
            hex::encode(&pieces.digests[..20]),
            "81fe8bfe87576c3ecb22426f8e57847382917acf"
        );
        // A gap in the numbers: no pieces at all.
        let gap = xml.replace("piece=\"0\"", "piece=\"2\"");
        assert_eq!(parse(&gap, &source()).unwrap().files[0].pieces, None);
    }

    /// RFC 5854 §4.1.2.1: a name that leaves the download folder is refused.
    #[test]
    fn names_that_leave_the_folder_are_refused() {
        for bad in [
            "../evil.exe",
            "a/../../evil.exe",
            "/etc/passwd",
            "\\\\server\\share\\x",
            "C:\\Windows\\x.dll",
            "a\\..\\..\\x",
        ] {
            let xml = format!(
                r#"<metalink><file name="{bad}"><url>https://a.example/x</url></file></metalink>"#
            );
            assert!(
                matches!(
                    parse(&xml, &source()).unwrap_err(),
                    MetalinkError::UnsafeName { .. }
                ),
                "{bad}"
            );
        }
        let ok = r#"<metalink><file name="./a/./b..c/CON.txt"><url>https://a.example/x</url></file></metalink>"#;
        let path = parse(ok, &source()).unwrap().files[0]
            .relative_path()
            .unwrap();
        assert_eq!(path, ["a", "b..c", "_CON.txt"].iter().collect::<PathBuf>());
    }

    #[test]
    fn bytes_are_checked_before_parsing() {
        let xml =
            "\u{feff}<metalink><file name=\"a\"><url>https://a.example/a</url></file></metalink>";
        assert_eq!(
            parse_bytes(xml.as_bytes(), &source()).unwrap().files.len(),
            1
        );
        assert_eq!(
            parse_bytes(&[0xff, 0xfe, 0x3c, 0x00], &source()).unwrap_err(),
            MetalinkError::NotText
        );
        let huge = vec![b' '; MAX_MANIFEST as usize + 1];
        assert_eq!(
            parse_bytes(&huge, &source()).unwrap_err(),
            MetalinkError::TooLarge
        );
    }
}
