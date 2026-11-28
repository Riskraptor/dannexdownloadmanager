//! labserver: an HTTP Range server with a request log and fault injection.
//! Used in-process by Dannex Download Manager's tests and standalone for the behaviour lab (notes/BEHAVIOR-LAB.md).

pub mod content;
pub mod debrid;
pub mod debrid_torrents;
pub mod drive;
pub mod faults;
mod feeds;
pub mod ftp;
mod media;
mod metalinks;
mod paced;
mod pages;
mod server;
mod site;
mod streams;
pub mod web;

pub use faults::Faults;
pub use server::{Config, HELP, Held, LabServer, LogEntry, LogSink, SlotHold};
