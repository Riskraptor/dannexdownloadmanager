//! How a download fails (`Error`): each failure in words a person reads, whether it is worth
//! another try and after how long.

use super::*;

/// Cloneable (the network and disk errors it wraps are shared), so a snapshot can carry the
/// failure itself for an interface to word in its own language (`Snapshot::failure`).
#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", describe_http(.0))]
    Http(Arc<reqwest::Error>),
    #[error("server answered {0}")]
    Status(StatusCode),
    /// The site's bot check refused the request: it lets browsers through, not programs (#376).
    /// Cloudflare says so in its answer (`cf-mitigated: challenge`).
    #[error("the site's bot check refused the download")]
    BotCheck,
    /// 429 / 503: the server is busy; retry after its `Retry-After` when it gives one.
    #[error("server busy ({0})")]
    Throttled(StatusCode, Option<Duration>),
    #[error("server sent an unexpected range: {0}")]
    BadRange(String),
    #[error("connection closed before the segment was complete")]
    ShortBody,
    #[error("the file changed on the server")]
    FileChanged,
    /// The server ignores ranges and the probe connection can't take the file back alone.
    #[error("the server does not support ranges")]
    NoRanges,
    #[error("cannot resume: {0}")]
    BadState(String),
    #[error("disk: {0}")]
    Io(Arc<io::Error>),
    /// Checked before starting: the file and a safety margin don't fit on the drive.
    #[error("not enough disk space: {} needed, {} free", disk::human(*needed), disk::human(*free))]
    NoSpace { needed: u64, free: u64 },
    /// The drive filled up while downloading (another program took the space).
    #[error("the disk is full")]
    DiskFull,
    #[error("{} is too big for this {file_system} drive (files up to 4 GB)", disk::human(*size))]
    TooBigForDrive { size: u64, file_system: String },
    #[error("cancelled")]
    Cancelled,
    /// The chosen network adapter is down, or can't reach this server (#35).
    #[error(transparent)]
    Adapter(#[from] crate::net::NetError),
    /// Another download in this process is writing to the same file.
    #[error("already being downloaded: {0}")]
    InUse(PathBuf),
    /// The save folder can't be reached or created (a removed drive, an offline share).
    #[error("the folder {} is not available (drive removed or network share offline?)", .0.display())]
    DestinationUnavailable(PathBuf),
    /// The drive or network share went away while downloading.
    #[error("the drive or network share is no longer available")]
    DriveGone,
    /// Longer than Windows programs can open (MAX_PATH).
    #[error("the path is too long for Windows: {}", .0.display())]
    PathTooLong(PathBuf),
    /// A web page (a sign-in wall, an expired link's notice) came instead of the file.
    #[error("the server sent a web page instead of the file (sign-in needed or the link expired?)")]
    GotWebPage,
    /// A full answer to a range request for the same, unchanged file: the server (or one CDN
    /// edge) didn't serve the range this time. Retried; the file did not change.
    #[error("the server did not send the requested part")]
    RangesUnavailable,
    /// The redirected address stopped working and the original address no longer leads to this
    /// file: the user has to give a fresh address (Refresh download address).
    #[error("the download link has expired (refresh the download address)")]
    LinkExpired,
    /// This connection was far slower than the others: it reconnects for the rest of its
    /// segment (not a failure).
    #[error("much slower than the other connections; reconnecting")]
    SlowConnection,
    /// A Metalink mirror failed (refused, unreachable, without ranges, or another file): it is
    /// left out for the rest of the download and the connection asks the next source at once
    /// (not a failure).
    #[error("a mirror failed; asking the next source")]
    MirrorDropped,
    /// The finished file doesn't have the expected hash; it stays as the part file.
    #[error("the file's {0} checksum doesn't match (the data is kept; redownload it)")]
    ChecksumMismatch(&'static str),
    /// An FTP server's refusal: its reply code and text.
    #[error("the FTP server answered {code} {text}")]
    FtpReply { code: u32, text: String },
    /// A network failure outside HTTP (FTP's sessions): `cause` as the system words it.
    #[error("{}", describe_net(*.kind, .cause))]
    Net { kind: NetKind, cause: String },
    /// A site's own refusal (Google Drive's quota page, its sign-in wall, a folder link, #34):
    /// retrying now won't help, the user has to do something else.
    #[error("{0}")]
    Refused(Refusal),
    /// A secure connection (HTTPS, FTPS) on a system without trusted certificates to check
    /// the server with (a minimal Linux without `ca-certificates`, #264).
    #[error("{}", crate::tls::NO_ROOTS)]
    NoCertificates,
}

/// What failed on the network (`Error::Net`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetKind {
    Connect,
    /// The server went quiet.
    Timeout,
    /// The connection broke while receiving.
    Lost,
    /// The secure connection failed (an untrusted certificate, a handshake the server refused).
    Tls,
}

impl Error {
    /// The network failed, not the server or the file: no connection, one gone quiet or broken
    /// off. A download that ended on it can go on by itself once the network is back.
    pub fn network_lost(&self) -> bool {
        match self {
            Error::Http(h) => h.is_connect() || h.is_timeout() || h.is_request() || h.is_body(),
            Error::ShortBody => true,
            Error::Net { kind, .. } => *kind != NetKind::Tls,
            _ => false,
        }
    }

    /// The address no longer serves this file (or refuses us): retrying won't help.
    pub(super) fn link_dead(&self) -> bool {
        matches!(self, Error::Status(s) if matches!(s.as_u16(), 401 | 403 | 404 | 410 | 416))
            || matches!(self, Error::GotWebPage | Error::LinkExpired | Error::BotCheck)
            // A site's refusal; a service's, unless waiting helps (#44).
            || matches!(self, Error::Refused(r) if !matches!(r, Refusal::Service { fault, .. } if fault.retry_wait().is_some()))
            // Login refused; no such file (or no permission); a name the server won't take.
            || matches!(self, Error::FtpReply { code: 530 | 532 | 550 | 553, .. })
    }

    /// Errors that stop the whole download instead of being retried by a connection.
    pub(super) fn is_fatal(&self) -> bool {
        matches!(
            self,
            Error::FileChanged
                | Error::NoRanges
                | Error::BadState(_)
                | Error::Io(_)
                | Error::NoSpace { .. }
                | Error::DiskFull
                | Error::TooBigForDrive { .. }
                | Error::DestinationUnavailable(_)
                | Error::DriveGone
                | Error::PathTooLong(_)
                | Error::Adapter(_)
                | Error::ChecksumMismatch(_)
                | Error::Cancelled
                | Error::InUse(_)
                | Error::NoCertificates
        )
    }

    /// A failure of the first request that a later try may not have: a busy server (429, 503),
    /// a server error (500, 502, 504), a network that failed to connect or went quiet.
    pub(super) fn transient(&self) -> bool {
        match self {
            Error::Throttled(..) => true,
            Error::Status(s) => matches!(s.as_u16(), 500 | 502 | 504),
            Error::Http(h) => h.is_connect() || h.is_timeout() || h.is_request() || h.is_body(),
            Error::ShortBody => true,
            // Too many sessions; no data connection; transfer aborted; file busy; local error.
            Error::FtpReply { code, .. } => matches!(code, 421 | 425 | 426 | 450 | 451),
            Error::Net { kind, .. } => *kind != NetKind::Tls,
            Error::Refused(Refusal::Service { fault, .. }) => fault.retry_wait().is_some(),
            _ => false,
        }
    }

    /// A server turning this connection away while others work: its connection limit. FTP
    /// servers say it at login, with 421 or (vsftpd, ProFTPD) 530.
    pub(super) fn refused(&self) -> bool {
        match self {
            Error::Throttled(..) => true,
            Error::Status(s) => s.as_u16() == 403,
            Error::Http(h) => h.is_connect(),
            Error::FtpReply { code, .. } => matches!(code, 421 | 530),
            Error::Net { kind, .. } => *kind == NetKind::Connect,
            _ => false,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        // NOT_READY, BAD_NETPATH, DEV_NOT_EXIST, NETNAME_DELETED, BAD_NET_NAME,
        // DEVICE_NOT_CONNECTED: the drive or share is gone, not a disk fault.
        #[cfg(windows)]
        if matches!(e.raw_os_error(), Some(21 | 53 | 55 | 64 | 67 | 1167)) {
            return Error::DriveGone;
        }
        // ENXIO, ENODEV (both systems); ENOMEDIUM (Linux): the device is gone.
        #[cfg(unix)]
        if matches!(e.raw_os_error(), Some(6 | 19))
            || (cfg!(target_os = "linux") && e.raw_os_error() == Some(123))
        {
            return Error::DriveGone;
        }
        match e.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Error::DiskFull,
            _ => Error::Io(Arc::new(e)),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if crate::tls::is_no_roots(&e) {
            return Error::NoCertificates;
        }
        Error::Http(Arc::new(e))
    }
}

impl From<disk::Refusal> for Error {
    fn from(r: disk::Refusal) -> Self {
        match r {
            disk::Refusal::NoSpace { needed, free } => Error::NoSpace { needed, free },
            disk::Refusal::TooBigForFileSystem { size, file_system } => {
                Error::TooBigForDrive { size, file_system }
            }
        }
    }
}

/// How long to wait before try number `failures + 1` after `e`: the first retry at once (IDM
/// reconnects within ~12 ms, lab S5), then `retry_delay` doubling up to 30 s; a busy server's
/// Retry-After wins (capped at 5 minutes). One policy for the first request and every connection.
pub(super) fn retry_wait(opts: &Options, failures: u32, e: &Error) -> Option<Duration> {
    match e {
        Error::Throttled(_, Some(after)) => Some((*after).min(Duration::from_secs(300))),
        _ if failures > 1 => {
            Some((opts.retry_delay * 2u32.pow((failures - 2).min(10))).min(Duration::from_secs(30)))
        }
        _ => None,
    }
}

/// A network error in words a user can act on, with its deepest cause ("connection refused",
/// "dns error: …") but not the address, which the dialog already shows.
pub(super) fn describe_http(e: &reqwest::Error) -> String {
    let what = if e.is_timeout() {
        "the server did not answer in time"
    } else if e.is_connect() {
        "cannot connect to the server"
    } else if e.is_body() || e.is_decode() {
        "the connection was lost while receiving data"
    } else if e.is_redirect() {
        "too many redirects"
    } else {
        "network error"
    };
    let mut cause = None;
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        cause = Some(s.to_string());
        source = s.source();
    }
    match cause.map(|c| strip_os_error_code(&c)) {
        Some(c) if !c.is_empty() && !c.contains("://") => format!("{what} ({c})"),
        _ => what.to_owned(),
    }
}

/// A network failure outside HTTP in words a user can act on, with its cause.
pub(super) fn describe_net(kind: NetKind, cause: &str) -> String {
    let what = match kind {
        NetKind::Connect => "cannot connect to the server",
        NetKind::Timeout => "the server did not answer in time",
        NetKind::Lost => "the connection was lost while receiving data",
        NetKind::Tls => "the secure connection failed",
    };
    if cause.is_empty() {
        what.to_owned()
    } else {
        format!("{what} ({cause})")
    }
}

/// "… refused it. (os error 10061)" → "… refused it".
pub(crate) fn strip_os_error_code(s: &str) -> String {
    let s = match s.rfind(" (os error ") {
        Some(i) if s.ends_with(')') => &s[..i],
        _ => s,
    };
    s.trim().trim_end_matches('.').to_owned()
}
