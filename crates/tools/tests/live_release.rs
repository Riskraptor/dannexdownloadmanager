//! A published release, taken as DDM's updater takes it: its SHA256SUMS.txt checked against the
//! release key the program carries, then a release file downloaded and checked against that list,
//! with the same code (`update::fetch`) over the internet. Not run by default (it downloads from
//! GitHub); after a release:
//!
//!     MDM_LIVE_RELEASE=v0.6.2 cargo test -p mdm-tools --test live_release -- --ignored
//!
//! MDM_LIVE_FILE picks the file (default: the Windows portable zip, the smallest Windows one).

use mdm_tools::update::{Progress, fetch};

#[tokio::test]
#[ignore = "downloads a published release from GitHub: MDM_LIVE_RELEASE=vX.Y.Z"]
async fn a_published_release_is_taken_by_the_updater() {
    let tag = std::env::var("MDM_LIVE_RELEASE").expect("MDM_LIVE_RELEASE=vX.Y.Z");
    let version = tag.trim_start_matches('v');
    let name = std::env::var("MDM_LIVE_FILE")
        .unwrap_or_else(|_| format!("DannexDownloadManager-{version}-portable.zip"));
    let base =
        format!("https://github.com/Riskraptor/dannexdownloadmanager/releases/download/{tag}");
    let assets: Vec<(String, String)> = ["SHA256SUMS.txt", "SHA256SUMS.txt.sig", name.as_str()]
        .iter()
        .map(|n| (n.to_string(), format!("{base}/{n}")))
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let net = mdm_tools::net(mdm_engine::Proxy::None);
    let progress = Progress::default();
    let file = fetch(&assets, &name, dir.path(), &net, &progress)
        .await
        .unwrap_or_else(|e| panic!("{tag}: the updater refuses {name}: {e}"));
    let len = std::fs::metadata(&file).unwrap().len();
    assert!(len > 1_000_000, "{name}: {len} bytes");
    println!("{tag}: {name} ({len} bytes) signed by the release key and matching its checksum");
}
