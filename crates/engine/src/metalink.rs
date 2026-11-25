//! Metalink 3 and 4 manifests: safe mirror lists, sizes and whole-file hashes.
//!
//! A manifest is data, not a web page.  This reader only parses its XML: it never follows a
//! link, expands an entity, or honours a local path from the document.  Callers decide which
//! listed sources to use and use [`File::checksum`] to verify the result.

use std::collections::HashSet;

use roxmltree::{Document, Node};
use url::Url;

use crate::verify::Hash;

/// A Metalink document.  One document may describe several files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub files: Vec<File>,
}

/// One file described by a Metalink document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// The publisher's file name, when the document supplied one.
    pub name: Option<String>,
    /// The expected byte size, when supplied.
    pub size: Option<u64>,
    /// Download locations, in the publisher's preference order.  Duplicate or unsupported
    /// locations are removed before the list reaches the downloader.
    pub urls: Vec<Url>,
    /// The strongest whole-file hash that DDM understands.
    pub checksum: Option<Hash>,
}

/// Reads a Metalink v3 or v4 XML document.
///
/// Relative `url` values resolve relative to the manifest itself.  A valid manifest needs at
/// least one downloadable URL for every advertised file; this prevents accidentally adding an
/// empty or HTML error document as a download manifest.
pub fn parse(xml: &str, source: &Url) -> Result<Manifest, String> {
    let doc = Document::parse(xml).map_err(|e| format!("invalid Metalink XML: {e}"))?;
    let files: Vec<_> = doc
        .descendants()
        .filter(|n| element_named(*n, "file"))
        .map(|n| parse_file(n, source))
        .collect::<Result<_, _>>()?;
    if files.is_empty() {
        return Err("the Metalink has no files".into());
    }
    Ok(Manifest { files })
}

fn parse_file(file: Node<'_, '_>, source: &Url) -> Result<File, String> {
    let name = file
        .attribute("name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
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
        let Some(raw) = text(node) else { continue };
        let Ok(url) = source.join(raw) else { continue };
        if crate::supported(&url) {
            // Metalink's lower priority number wins; omitted is the normal, middle priority.
            let priority = node
                .attribute("priority")
                .and_then(|n| n.trim().parse::<u32>().ok())
                .unwrap_or(999_999);
            ordered.push((priority, index, url));
        }
    }
    ordered.sort_by_key(|(priority, index, _)| (*priority, *index));
    let mut seen = HashSet::new();
    let urls: Vec<_> = ordered
        .into_iter()
        .filter_map(|(_, _, url)| seen.insert(url.to_string()).then_some(url))
        .collect();
    if urls.is_empty() {
        let shown = name.as_deref().unwrap_or("file");
        return Err(format!(
            "the Metalink's {shown} has no HTTP, HTTPS or FTP URL"
        ));
    }

    let checksum = file
        .descendants()
        .filter(|n| element_named(*n, "hash"))
        .filter_map(parse_hash)
        .max_by_key(strength);
    Ok(File {
        name,
        size,
        urls,
        checksum,
    })
}

fn element_named(node: Node<'_, '_>, wanted: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(wanted)
}

fn text<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    let text = node.text()?.trim();
    (!text.is_empty()).then_some(text)
}

fn parse_hash(node: Node<'_, '_>) -> Option<Hash> {
    let kind = node.attribute("type")?.trim().to_ascii_lowercase();
    let kind = match kind.as_str() {
        "crc32" => "crc32",
        "md5" => "md5",
        "sha1" | "sha-1" => "sha1",
        "sha256" | "sha-256" => "sha256",
        "sha512" | "sha-512" => "sha512",
        _ => return None,
    };
    let hex: String = text(node)?.chars().filter(|c| !c.is_whitespace()).collect();
    Hash::parse(&format!("{kind}:{hex}"))
}

fn strength(hash: &Hash) -> u8 {
    use crate::verify::Algo;
    match hash.algo {
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
        assert_eq!(file.urls.len(), 2);
        assert_eq!(file.checksum.as_ref().unwrap().algo, Algo::Sha256);
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
        assert!(parse(xml, &source()).unwrap_err().contains("no HTTP"));
    }
}
