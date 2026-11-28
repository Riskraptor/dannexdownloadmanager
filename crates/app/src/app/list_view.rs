//! What the download list shows: the view's downloads, their order and filters, each row's cells,
//! and the downloads a command acts on.

use super::*;

/// The view's rows, each one's texts made when the list draws it ([`list::Rows`]): a frame
/// with 10,000 downloads makes the twenty or so in view, not ten thousand.
pub(super) struct LazyRows<'a> {
    pub(super) items: &'a [Item],
    /// The rows' downloads, as places in `items`, in the list's order.
    pub(super) order: &'a [usize],
    pub(super) queued: &'a std::collections::HashSet<u64>,
    pub(super) extracts: &'a crate::extractq::Extracts,
}

impl list::Rows for LazyRows<'_> {
    fn len(&self) -> usize {
        self.order.len()
    }
    fn row(&self, i: usize) -> list::Row {
        App::list_row(&self.items[self.order[i]], self.queued, self.extracts)
    }
    fn name(&self, i: usize) -> String {
        self.items[self.order[i]].file_name.clone()
    }
}

impl App {
    pub(super) fn item(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// The one download single-download commands act on (Properties, Open…).
    pub(super) fn selected_item(&self) -> Option<&Item> {
        self.selection
            .one()
            .and_then(|id| self.items.iter().find(|i| i.id == id))
    }

    /// The downloads a command acts on: those selected, in list order; a right-clicked one
    /// outside the selection alone.
    pub(super) fn targets(&self, target: Option<u64>) -> Vec<u64> {
        match target {
            Some(t) if !self.selection.contains(t) => vec![t],
            _ => self.selection.ids().to_vec(),
        }
    }

    /// The downloads the selection-wide commands would act on.
    pub(super) fn chosen(&self, target: Option<u64>) -> Vec<&Item> {
        self.targets(target)
            .into_iter()
            .filter_map(|id| self.items.iter().find(|i| i.id == id))
            .collect()
    }

    /// The list's rows as drawn: the view's downloads in the list's order that pass the column
    /// filters (Find, Select all and scrolling count these, so they match what is shown).
    pub(super) fn shown_ids(&self) -> Vec<u64> {
        if self.list.filters.iter().all(Option::is_none) {
            return self.view_ids();
        }
        self.list_rows().0
    }

    /// The rows the list draws ([`Self::shown_ids`]) with their cells.
    pub(super) fn list_rows(&self) -> (Vec<u64>, Vec<list::Row>) {
        let queued = self.queued_ids();
        self.view_items()
            .into_iter()
            .map(|it| (it.id, Self::list_row(it, &queued, &self.extracts)))
            .filter(|(_, r)| list::matches(&self.list, r))
            .unzip()
    }

    /// The downloads in a queue (worked out once for all the rows).
    pub(super) fn queued_ids(&self) -> std::collections::HashSet<u64> {
        self.queues
            .iter()
            .flat_map(|q| q.items.iter().copied())
            .collect()
    }

    /// The list's rows as [`Self::shown_ids`] has them, for drawing: their ids, and where they
    /// are in `items` when no column filter is on (each row's texts are then made only as the
    /// list draws it, [`LazyRows`]); else every row made, the filters having looked at them.
    pub(super) fn list_order(&self) -> (Vec<u64>, Result<Vec<usize>, Vec<list::Row>>) {
        if self.list.filters.iter().all(Option::is_none) {
            let order = self.view_order();
            let ids = order.iter().map(|&k| self.items[k].id).collect();
            (ids, Ok(order))
        } else {
            let (ids, rows) = self.list_rows();
            (ids, Err(rows))
        }
    }

    /// One download's row in the list. `queued`: the downloads in a queue.
    pub(super) fn list_row(
        it: &Item,
        queued: &std::collections::HashSet<u64>,
        extracts: &crate::extractq::Extracts,
    ) -> list::Row {
        list::Row {
            status_group: match it.status {
                // i18n: a download's status (state it is in, not a command): being downloaded now
                Status::Downloading => "Downloading",
                // i18n: a download's status (state it is in, not a command): stopped by the user
                Status::Stopped => "Stopped",
                // i18n: a download's status (state it is in, not a command): waiting in a queue for its turn
                Status::Queued => "Waiting",
                // i18n: a download's status (state it is in, not a command): finished, the whole file is there
                Status::Complete => "Complete",
                // i18n: a download's status (state it is in, not a command): it failed
                Status::Error => "Error",
            },
            icon: file_icon(&it.file_name),
            queued: queued.contains(&it.id),
            cells: [
                it.file_name.clone(),
                String::new(),
                it.size.map_or(String::new(), |s| {
                    model::approx_size(model::size(s), it.size_approx)
                }),
                if extracts.extracting(it.id) {
                    // i18n: a download's status: its archive is being extracted (#37)
                    crate::i18n::t("Extracting...").to_owned()
                } else {
                    it.status_text()
                },
                it.time_left(),
                if it.live.is_some() {
                    model::rate(it.rate())
                } else {
                    String::new()
                },
                it.last_try
                    .as_deref()
                    .map_or(String::new(), model::date_text),
                it.error.clone().unwrap_or_else(|| it.description.clone()),
                it.added.as_deref().map_or(String::new(), model::date_text),
                // A torrent through a debrid account is a torrent too (#44).
                if it.torrent.is_some() || it.remote.is_some() {
                    // i18n: where a download comes from: BitTorrent (shown on a small colored label)
                    "Torrent".into()
                } else {
                    // i18n: where a download comes from: a web or FTP server (shown on a small colored label)
                    "Web".into()
                },
                // A torrent's own numbers (#357), in their slots' order (list::PEERS, RATIO, then
                // UPLOAD, added last so saved layouts keep their meaning): the peers it has, what
                // it gave back against what it took (as the General tab works it out), what it sends.
                it.torrent
                    .as_ref()
                    .map_or(String::new(), |t| crate::i18n::integer(t.peers as u64)),
                it.torrent.as_ref().map_or(String::new(), |t| {
                    crate::torrents::share_ratio_text(t.uploaded, it.downloaded)
                }),
                it.torrent
                    .as_ref()
                    .map_or(String::new(), |t| model::rate(t.up_speed)),
            ],
        }
    }

    /// The view's downloads in the list's order, before the column filters.
    pub(super) fn view_ids(&self) -> Vec<u64> {
        self.view_order()
            .into_iter()
            .map(|k| self.items[k].id)
            .collect()
    }

    /// [`Self::view_ids`]'s downloads themselves.
    pub(super) fn view_items(&self) -> Vec<&Item> {
        self.view_order()
            .into_iter()
            .map(|k| &self.items[k])
            .collect()
    }

    /// [`Self::view_ids`]'s downloads as places in `items` (each sort key worked out once, not
    /// at every comparison).
    pub(super) fn view_order(&self) -> Vec<usize> {
        let shown = (0..self.items.len()).filter(|&k| self.in_view(self.view, &self.items[k]));
        // Not by a column this view doesn't show (a torrent column out of the Torrents view).
        let Some((col, asc)) = self.list.sort.filter(|(c, _)| self.list.is_shown(*c)) else {
            return shown.collect();
        };
        let queued = self.queued_ids();
        let mut keyed: Vec<((f64, String), usize)> = shown
            .map(|k| {
                let i = &self.items[k];
                (sort_key(i, col, queued.contains(&i.id)), k)
            })
            .collect();
        keyed.sort_by(|(ka, _), (kb, _)| {
            let ord = ka.0.total_cmp(&kb.0).then_with(|| ka.1.cmp(&kb.1));
            if asc { ord } else { ord.reverse() }
        });
        keyed.into_iter().map(|(_, i)| i).collect()
    }

    /// Select the next download matching `q` after the selection: in the list as shown, else
    /// in all downloads (switching the view to All Downloads).
    pub(super) fn find_next(&mut self, q: &crate::find::Query) {
        let shown = self.shown_ids();
        let found = q
            .next(&shown, &self.items, self.selection.focus())
            .or_else(|| {
                self.view = View::All;
                q.next(&self.shown_ids(), &self.items, self.selection.focus())
            });
        match found {
            Some(id) => {
                self.selection.set(id);
                self.list.scroll_to = self.shown_ids().iter().position(|x| *x == id);
            }
            None => {
                self.notice = Some((
                    crate::i18n::mark("Find file"),
                    crate::i18n::fmt(
                        crate::i18n::t("Cannot find \"{text}\""),
                        &[("text", q.text.trim())],
                    ),
                ))
            }
        }
    }
}

/// The icon for a file name's extension (empty: none).
pub(super) fn file_icon(name: &str) -> IconKey {
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => IconKey::Ext(ext.to_ascii_lowercase()),
        _ => IconKey::Ext(String::new()),
    }
}

/// The Referer a download was asked with (the page its link was on), or "".
pub(super) fn referer_of(it: &Item) -> &str {
    it.context.referer.as_deref().unwrap_or_default()
}

/// The web page a download came from, as File Properties shows it ([`Item::web_page`]: a
/// video's or a gallery's own page, else the page its link was on), or "".
pub(super) fn page_of(it: &Item) -> String {
    it.web_page().unwrap_or_default()
}

/// Sort key for list column col: numbers where IDM sorts numerically, else text.
/// `queued`: the download is in a queue (the Q column's mark).
pub(super) fn sort_key(it: &Item, col: usize, queued: bool) -> (f64, String) {
    let frac = it.fraction().unwrap_or(0.0);
    match col {
        0 => (0.0, it.file_name.to_lowercase()),
        // As the column shows it: in a queue or not.
        1 => (f64::from(u8::from(queued)), String::new()),
        2 => (it.size.unwrap_or(0) as f64, String::new()),
        3 => (
            if it.status == Status::Complete {
                2.0
            } else {
                frac
            },
            String::new(),
        ),
        4 => (
            it.size.map_or(f64::MAX, |s| {
                (s.saturating_sub(it.downloaded)) as f64 / it.rate().max(1.0)
            }),
            String::new(),
        ),
        5 => (it.rate(), String::new()),
        // By time, not by the text ("Apr" would come before "Oct").
        6 => (
            it.last_try
                .as_deref()
                .and_then(model::stamp_key)
                .map_or(0.0, |t| t as f64),
            String::new(),
        ),
        // Added: the order of addition itself (ids only grow), exact even without a time.
        list::ADDED => (it.id as f64, String::new()),
        list::SOURCE => (f64::from(u8::from(it.torrent.is_some())), String::new()),
        // A torrent's upload, its peers, and its share ratio (not the text: "10" would come
        // before "9"). Anything but a torrent, and a torrent that has taken nothing, sorts as
        // zero.
        list::UPLOAD => (
            it.torrent.as_ref().map_or(0.0, |t| t.up_speed),
            String::new(),
        ),
        list::PEERS => (
            it.torrent.as_ref().map_or(0.0, |t| t.peers as f64),
            String::new(),
        ),
        list::RATIO => (
            it.torrent.as_ref().map_or(0.0, |t| {
                crate::torrents::share_ratio(t.uploaded, it.downloaded)
            }),
            String::new(),
        ),
        // View → Arrange files' own orders: the folder (then the name in it), the Referer the
        // download was asked with, the web page it belongs to (a video's or a gallery's page,
        // else the page its link was on: its Referer). None sorts first.
        list::BY_SAVE_PATH => (0.0, it.dest.to_string_lossy().to_lowercase()),
        list::BY_REFERER => (0.0, referer_of(it).to_lowercase()),
        list::BY_PAGE => (0.0, page_of(it).to_lowercase()),
        _ => (0.0, it.description.to_lowercase()),
    }
}

/// Silk icons standing in for IDM's XP-era category icons.
/// A theme's icon set may replace them (`crate::themeicons`).
pub(super) fn category_icon(c: Category) -> IconKey {
    let (need, silk) = match c {
        Category::Compressed => ("cat-compressed", "page_white_zip"),
        Category::Documents => ("cat-documents", "page_white"),
        Category::Music => ("cat-music", "music"),
        Category::Programs => ("cat-programs", "application_xp"),
        Category::Video => ("cat-video", "film"),
        Category::General => ("cat-documents", "page_white"),
        Category::Torrents => ("cat-torrents", "folder_link"),
        Category::Custom(_) => ("cat-folder", "folder"),
    };
    IconKey::Themed(need, silk)
}
