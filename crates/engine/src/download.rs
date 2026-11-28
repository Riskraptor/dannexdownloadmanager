//! One download: probe, parallel connections driven by the planner, positional writes, progress.
//!
//! The first connection is also the probe: a plain `GET` (no Range, as IDM does — D-007) that
//! keeps streaming from byte 0 while further connections split the file (notes/ENGINE.md §1).
//! Range support is learned from the first range answer: 206 confirms it; 200 means the server
//! ignores ranges, so the file goes back to the probe connection alone.
//! One HTTP client per download, shared by its connections: HTTP/2 is negotiated with HTTPS
//! servers that offer it and HTTP/1.1 remains the fallback.  The pool has at most
//! `connections()` requests at a time, since each worker has one request in flight.

use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::disk;
use crate::dns;
use crate::html;
use crate::limiter::Limiter;
use crate::naming;
use crate::net;
use crate::planner::{Claim, Planner, WorkerId};
use crate::sites::{self, PageSays, Refusal};
use crate::sparse;
use crate::state::{ResumeState, Validators, state_path};

// What a download is given and what it tells: the public API (`mdm_engine::download::…`, and
// lib.rs's re-exports).
mod error;
mod events;
mod handle;
mod options;
mod probing;
// How it runs: its clients, the planner's segments and their connections, the first request,
// mirrors, resuming, the finished file's checks and repairs.
mod clients;
mod connections;
mod first;
mod mirrors;
mod pages;
mod plan;
mod repair;
mod resume;
#[cfg(test)]
mod tests;
mod transfer;

pub use self::clients::*;
pub use self::error::*;
pub use self::events::*;
pub use self::handle::*;
pub use self::options::*;
pub use self::probing::*;

// What the parts share among themselves (each takes it, and this module's imports, with
// `use super::*`).
use self::connections::*;
use self::first::*;
use self::mirrors::*;
use self::pages::*;
use self::plan::*;
use self::repair::*;
use self::resume::*;
use self::transfer::*;

// What the engine's other modules use (browserlike.rs, ftp.rs).
pub(crate) use self::connections::{Body, Head};
#[cfg(feature = "browser-connection")]
pub(crate) use self::first::{add_cookies, set_cookies};
pub(crate) use self::pages::{is_bot_check, is_web_page, named};
pub(crate) use self::transfer::{cancellable, retry_after};
