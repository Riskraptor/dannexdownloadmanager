//! What a download tells while it runs: its state, each connection's and segment's view, the
//! snapshot the progress window draws, and its events.

use super::*;

/// What the server's range answers have shown so far.
pub(super) const RANGES_UNKNOWN: u8 = 0;

pub(super) const RANGES_YES: u8 = 1;

pub(super) const RANGES_NO: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Probing,
    Downloading,
    /// Checking the finished file against its checksum.
    Verifying,
    Completing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub enum ConnStatus {
    Connecting,
    Receiving,
    /// Waiting before a retry, with the last error.
    Retrying(Error),
    Finished,
    Failed(Error),
    /// Closed for good: `host` allows `cap` connections and the others fill them (learned).
    Retired {
        host: String,
        cap: u32,
    },
}

#[derive(Clone, Debug)]
pub struct ConnView {
    pub id: WorkerId,
    pub downloaded: u64,
    pub status: ConnStatus,
    /// Bytes/s over the current response (0 while not receiving).
    pub speed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentView {
    pub start: u64,
    pub cursor: u64,
    pub end: u64,
}

/// A point-in-time view for progress displays.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: State,
    pub url: Url,
    pub file: Option<PathBuf>,
    pub size: Option<u64>,
    pub downloaded: u64,
    pub resumable: Option<bool>,
    pub segments: Vec<SegmentView>,
    pub connections: Vec<ConnView>,
    /// Why the download failed (state `Failed`), in English (the command line, logs).
    pub error: Option<String>,
    /// The same failure as data, for an interface that words it in its own language.
    pub failure: Option<Error>,
    /// What happened, oldest first (the last `EVENTS`): the download's event log. Never
    /// cookies or credentials.
    pub events: Vec<Event>,
    /// While a service account prepares the file's link (#44): the service, and how far it is
    /// (0 to 1) when it says. The list shows it instead of "Connecting...".
    pub at_service: Option<(String, Option<f32>)>,
    /// Where its server is, as its connections saw it (#140).
    pub server: Option<crate::Server>,
}

/// One entry of a download's event log; `seq` counts up within one `Download`.
#[derive(Clone, Debug)]
pub struct Event {
    pub seq: u64,
    pub at: std::time::SystemTime,
    pub text: String,
}

/// Events kept per download.
pub const EVENTS: usize = 50;
