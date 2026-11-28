//! IDM's small dialogs as separate OS windows, laid out to the pixel from
//! notes/ui-spec/dialogs.md: "Add a download", "Download File Info",
//! "Download complete". All coordinates below are client pixels from the spec.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{Pos2, Ui, ViewportBuilder, ViewportId, vec2};
use mdm_engine::ProbeInfo;
use url::Url;

use crate::model::{self, Category};
use crate::winui::{self, ButtonKind, r};

mod add_url;
mod complete;
mod confirm;
mod file_info;
mod files;
mod folders;
mod speed;
mod turn_off;
mod viewport;

pub use self::add_url::*;
pub use self::complete::*;
pub use self::confirm::*;
pub use self::file_info::*;
pub use self::files::*;
pub use self::folders::*;
pub use self::speed::*;
pub use self::turn_off::*;
pub use self::viewport::*;

pub enum Outcome<T> {
    Open,
    Done(T),
    Closed,
}

#[cfg(test)]
mod tests;
