//! The main window (notes/ui-spec/main-window.md): menu bar, toolbar, Categories tree and the
//! download list at IDM 6.42's measured positions, plus the download actions behind them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine as _;
use egui::{Pos2, Ui, pos2};
use mdm_engine::state::state_path;
use mdm_engine::{Destination, Download, Options};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

use crate::chrome::{
    self, IconKey, MenuBarEvent, MenuEntry, MenuKind, PopupResult, ShellIcons, ToolbarEvent,
    TreeEvent, TreeRow,
};
use crate::dialogs::{self, AddUrl, Complete, FileInfo, Outcome, Start};
use crate::icons::Icon;
use crate::leftovers;
use crate::list;
use crate::model::{self, Category, Item, Live, Status};
use crate::progress::{self, Progress};
use crate::queue::{self, Member, Queue};
use crate::settings::Settings;
use crate::store;
use crate::winui;
use mdm_ipc as ipc;

#[path = "player.rs"]
mod player;
/// Site grabber's part of the main window (its projects in the tree, the wizard, the files added).
mod sitegrab;

// The window's state, and what runs at every frame.
mod frame;
mod state;
// The main window drawn: the list, the Categories tree, the menus and their commands.
mod commands;
mod list_view;
mod menus;
mod paint;
mod tree;
mod view_menu;
// File downloads: added, checked for duplicates, started, limited, moved, renamed, removed.
mod disk;
mod downloads;
mod duplicates;
mod moves;
mod removal;
mod renames;
mod speed;
// Videos and galleries, torrents and their feeds.
mod rss_feeds;
mod torrent_cmds;
mod video_cmds;
// The windows and their answers, what comes in from outside, queues, the tray and the watches.
mod answers;
mod beside;
mod import_export;
mod incoming;
mod options_apply;
mod queues;
#[cfg(test)]
mod tests;
mod tray_icon;
mod watch;
mod windows;

// The types and helpers the modules share: each module takes them, and this module's imports,
// with `use super::*`.
use self::beside::*;
use self::commands::*;
use self::disk::*;
use self::downloads::*;
use self::duplicates::*;
use self::import_export::*;
use self::incoming::*;
use self::list_view::*;
use self::menus::*;
use self::moves::*;
use self::options_apply::*;
use self::removal::*;
use self::renames::*;
use self::rss_feeds::*;
use self::torrent_cmds::*;
use self::tray_icon::*;
use self::tree::*;
use self::video_cmds::*;
use self::view_menu::*;
use self::windows::*;

// What the rest of the app uses.
pub(crate) use self::downloads::{RequestParts, mark_hook};
pub(crate) use self::state::App;

/// A node of the Categories tree, what the list shows (saved: the last one comes back).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum View {
    All,
    Cat(Category),
    Unfinished(Option<Category>),
    Finished(Option<Category>),
    /// A queue (index into `App::queues`).
    Queue(usize),
    /// A Site grabber project's downloads (its id, app/sitegrab.rs); never saved
    /// (`remember_layout`).
    Grabber(u64),
}
