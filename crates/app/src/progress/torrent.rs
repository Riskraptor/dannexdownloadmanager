//! A torrent's details: its pieces (bar, grid and legend) and its files' table, scrolled.

use super::*;

/// A progress window's table rows (px).
pub(super) const TABLE_ROW: f32 = 17.0;

/// Where a progress window's table is (px down) and its wheel's glide.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TableScroll {
    pub(super) px: f32,
    pub(super) glide: winui::WheelGlide,
}

impl Progress {
    /// A torrent's details: a tab strip where the connections caption is, the piece map in
    /// the segment bar, and the tab's table below.
    pub(super) fn torrent_details(&mut self, ui: &mut Ui, p: &egui::Painter, o: Pos2, item: &Item) {
        let d = match &item.live {
            Some(crate::model::Live {
                dl: crate::model::Job::Torrent(run),
                ..
            }) => Some(run.details()),
            // Finished and still shared (no Run): the session has it all the same, so the
            // tabs stay, the General tab's share ratio included.
            _ => self
                .host
                .as_ref()
                .and_then(|h| h.get())
                .zip(item.torrent.as_ref())
                .and_then(|(t, spec)| crate::torrents::details_of(&t, &spec.hash)),
        };
        let tabs = [
            // i18n: tab title: the torrent's files
            (TTab::Files, crate::i18n::mark("Files")),
            // i18n: tab title: the other computers sharing the torrent
            (TTab::Peers, crate::i18n::mark("Peers")),
            // i18n: tab title: the servers that list the peers
            (TTab::Trackers, crate::i18n::mark("Trackers")),
            // i18n: tab title: the parts the torrent is made of
            (TTab::Pieces, crate::i18n::mark("Pieces")),
            // i18n: tab title: general information
            (TTab::General, crate::i18n::mark("General")),
        ];
        // The usual tab control (the Scheduler's and Options' look): tabs sized to their
        // labels from the leading edge, the page under them holding the piece map and the
        // table.
        let english: Vec<&str> = tabs.iter().map(|t| t.1).collect();
        let light: Vec<winui::LightTab> = winui::tab_row(p, &english, 16.0, 530.0, 2.0, 8.0)
            .into_iter()
            .zip(tabs.iter())
            .map(|((x0, x1, _), (_, label))| winui::LightTab {
                label,
                sel: (x0 - 2.0, x1 + 2.0),
                unsel: (x0, x1),
            })
            .collect();
        let current = tabs.iter().position(|t| t.0 == self.ttab).unwrap_or(0);
        if let Some(i) = winui::tabs_light(ui, o, r(o, 14.0, 226.0, 521.0, 180.0), &light, current)
        {
            self.ttab = tabs[i].0;
        }
        let pieces = d.as_ref().map(|d| d.pieces.as_slice()).unwrap_or(&[]);
        piece_bar(p, o, pieces);
        let Some(d) = &d else {
            table(ui, p, o, "t-none", &[(25.0, "")], &[], &[], false, None);
            text(
                p,
                o.x + 25.0,
                o.y + 312.0,
                crate::i18n::t("Shown while the torrent is downloading."),
                winui::TEXT(),
            );
            return;
        };
        match self.ttab {
            TTab::Files => {
                let rows: Vec<Vec<String>> = d
                    .files
                    .iter()
                    .map(|(_, f, on, done)| {
                        let pct = if !on {
                            // i18n: a torrent file the user chose not to download
                            crate::i18n::t("Skipped").to_owned()
                        } else if f.size == 0 {
                            crate::i18n::percent(1.0, 0)
                        } else {
                            crate::i18n::percent(*done as f64 / f.size as f64, 1)
                        };
                        vec![f.path.display().to_string(), model::size(f.size), pct]
                    })
                    .collect();
                let ticks: Vec<bool> = d.files.iter().map(|f| f.2).collect();
                let clicked = files_table(ui, p, o, "t-files", &rows, &ticks, 6);
                // A box ticked or unticked (#39): the torrent downloads those files from now
                // on; the last one ticked can't be unticked (a torrent of nothing).
                if let Some(row) = clicked {
                    let mut on: Vec<bool> = ticks.clone();
                    on[row] = !on[row];
                    if on.iter().any(|b| *b) {
                        let chosen = d.files.iter().zip(&on).filter(|(_, b)| **b);
                        self.pending_choose = Some(Action::Choose {
                            files: chosen.clone().map(|(f, _)| f.0).collect(),
                            all: on.iter().all(|b| *b),
                            size: chosen.map(|(f, _)| f.1.size).sum(),
                        });
                    }
                }
            }
            TTab::Peers => {
                let rows: Vec<Vec<String>> = d
                    .peers
                    .iter()
                    .map(|q| {
                        vec![
                            q.addr.clone(),
                            q.client.clone().unwrap_or_default(),
                            model::size(q.fetched),
                            model::size(q.uploaded),
                        ]
                    })
                    .collect();
                table(
                    ui,
                    p,
                    o,
                    "t-peers",
                    &[
                        // i18n: column title: the web address (URL)
                        (25.0, crate::i18n::mark("Address")),
                        // i18n: column title: the peer's torrent program
                        (190.0, crate::i18n::mark("Client")),
                        // i18n: column title: how much came from the peer
                        (340.0, crate::i18n::mark("Received")),
                        // i18n: column title: how much went to the peer
                        (425.0, crate::i18n::mark("Sent")),
                    ],
                    &[185.0, 335.0, 420.0],
                    &rows,
                    false,
                    None,
                );
            }
            TTab::Trackers => {
                let rows: Vec<Vec<String>> = if d.trackers.is_empty() {
                    vec![vec![
                        crate::i18n::t("None: peers come from the DHT and other peers").into(),
                    ]]
                } else {
                    d.trackers.iter().map(|t| vec![t.clone()]).collect()
                };
                // i18n: column title: a server that lists the peers
                let head = [(25.0, crate::i18n::mark("Tracker"))];
                table(ui, p, o, "t-trackers", &head, &[], &rows, false, None);
            }
            TTab::Pieces => {
                let have = pieces.iter().filter(|b| **b).count();
                let head = crate::i18n::fmt(
                    // i18n: {have} and {total} = numbers of pieces; {size} = one piece's size
                    crate::i18n::t("{have} of {total} pieces ({size} each)"),
                    &[
                        ("have", &crate::i18n::integer(have as u64)),
                        ("total", &crate::i18n::integer(pieces.len() as u64)),
                        ("size", &model::size(u64::from(d.piece_length))),
                    ],
                );
                table(
                    ui,
                    p,
                    o,
                    "t-pieces",
                    &[(25.0, head.as_str())],
                    &[],
                    &[],
                    false,
                    None,
                );
                piece_legend(p, o);
                piece_grid(p, o, pieces);
            }
            TTab::General => {
                let st = d.stats.as_ref();
                let have = pieces.iter().filter(|b| **b).count();
                // What it sent in all its runs (#39; librqbit counts only this one), against what
                // it has: the same after a pause or a restart.
                let spec = item.torrent.as_ref();
                let sent = spec.map_or(0, |t| t.uploaded);
                let done = st.map_or(item.downloaded, |s| s.done);
                let ratio = crate::torrents::share_ratio_text(sent, done);
                let shared = spec.map_or(0, |t| t.seeded_secs);
                let hash = item
                    .torrent
                    .as_ref()
                    .map_or(String::new(), |t| t.hash.clone());
                let folder = item
                    .torrent
                    .as_ref()
                    .map_or(String::new(), |t| t.folder.display().to_string());
                let t = |s: &str| crate::i18n::t(s).to_owned();
                let rows = vec![
                    // i18n: the torrent's identifier
                    vec![t("Info hash"), hash],
                    vec![
                        t("Pieces"),
                        crate::i18n::fmt(
                            // i18n: {have} and {total} = numbers of pieces; {size} = one piece's size
                            crate::i18n::t("{have} of {total} ({size} each)"),
                            &[
                                ("have", &crate::i18n::integer(have as u64)),
                                ("total", &crate::i18n::integer(pieces.len() as u64)),
                                ("size", &model::size(u64::from(d.piece_length))),
                            ],
                        ),
                    ],
                    vec![
                        t("Peers"),
                        st.map_or(String::new(), |s| {
                            crate::i18n::fmt(
                                // i18n: {live} = peers connected now; {known} = peers heard of
                                crate::i18n::t("{live} connected, {known} known"),
                                &[
                                    ("live", &crate::i18n::integer(s.peers_live as u64)),
                                    ("known", &crate::i18n::integer(s.peers_seen as u64)),
                                ],
                            )
                        }),
                    ],
                    vec![
                        // i18n: how much was sent to other peers
                        t("Sent"),
                        model::size(sent),
                    ],
                    // i18n: sent divided by received
                    vec![t("Share ratio"), ratio],
                    vec![
                        // i18n: a torrent's General tab: how long it has been shared once finished, in all
                        t("Shared for"),
                        model::time(shared as f64),
                    ],
                    // i18n: label before the folder the torrent is saved in
                    vec![t("Saved in"), folder],
                    vec![
                        // i18n: noun: sharing the torrent with others after it is downloaded
                        t("Sharing"),
                        if self.keep_sharing {
                            // i18n: a torrent's sharing, when Torrent settings keep it on
                            t("Goes on after the download finishes")
                        } else {
                            t("Stops when the download finishes")
                        },
                    ],
                ];
                table(
                    ui,
                    p,
                    o,
                    "t-general",
                    &[
                        (25.0, crate::i18n::mark("Property")),
                        (130.0, crate::i18n::mark("Value")),
                    ],
                    &[125.0],
                    &rows,
                    false,
                    None,
                );
            }
        }
    }
}

/// The segment bar as a piece map: a column is filled when the pieces under it are there.
pub(super) fn piece_bar(p: &egui::Painter, o: Pos2, pieces: &[bool]) {
    let (x0, y0) = (o.x + 17.0, o.y + 249.0);
    let border = crate::theme::colors().segment_border;
    fill(p, x0, y0, x0 + 512.0, y0, border);
    fill(p, x0, y0 + 14.0, x0 + 512.0, y0 + 14.0, border);
    fill(p, x0, y0, x0, y0 + 14.0, border);
    fill(p, x0 + 512.0, y0, x0 + 512.0, y0 + 14.0, border);
    let ix = o.x + 18.0;
    // Runs of equal columns, painted as one rectangle per row.
    let have = |c: usize| {
        if pieces.is_empty() {
            return false;
        }
        let a = c * pieces.len() / 511;
        let b = ((c + 1) * pieces.len() / 511).max(a + 1).min(pieces.len());
        pieces[a..b].iter().all(|x| *x)
    };
    let mut c = 0usize;
    while c < 511 {
        let on = have(c);
        let mut e = c;
        while e + 1 < 511 && have(e + 1) == on {
            e += 1;
        }
        for (row, cols) in seg_rows().enumerate() {
            let v = if on { cols.1 } else { cols.0 };
            let y = y0 + 1.0 + row as f32;
            fill(p, ix + c as f32, y, ix + e as f32, y, v);
        }
        c = e + 1;
    }
}

/// The Pieces tab's key, at the header's right: have / some / missing.
pub(super) fn piece_legend(p: &egui::Painter, o: Pos2) {
    let (done, partial, missing) = piece_colors();
    let mut x = o.x + 505.0;
    for (c, label) in [
        // i18n: legend: pieces not downloaded yet
        (missing, crate::i18n::mark("Missing")),
        // i18n: legend: pieces partly downloaded
        (partial, crate::i18n::mark("Some")),
        // i18n: legend: pieces downloaded
        (done, crate::i18n::mark("Have")),
    ] {
        let label = crate::i18n::t(label);
        x -= winui::text_width(p, label);
        text(p, x, o.y + 290.0, label, winui::TEXT());
        x -= 10.0;
        p.rect_filled(
            winui::rect(egui::Rect::from_min_size(
                egui::pos2(x, o.y + 282.0),
                egui::vec2(7.0, 7.0),
            )),
            1.0,
            c,
        );
        x -= 14.0;
    }
}

/// Pieces there, partly there (a square standing for several), missing.
pub(super) fn piece_colors() -> (Color32, Color32, Color32) {
    let c = crate::theme::colors();
    (c.piece, c.piece_partial, c.piece_missing)
}

/// The Pieces tab: one square per piece (or per group of pieces when there are many).
pub(super) fn piece_grid(p: &egui::Painter, o: Pos2, pieces: &[bool]) {
    if pieces.is_empty() {
        return;
    }
    let (left, top, w, h) = (o.x + 22.0, o.y + 302.0, 498.0, 94.0);
    let cols = (w / 7.0) as usize;
    let rows = (h / 7.0) as usize;
    let cells = (cols * rows).min(pieces.len());
    let (done, partial, missing) = piece_colors();
    for k in 0..cells {
        let a = k * pieces.len() / cells;
        let b = ((k + 1) * pieces.len() / cells)
            .max(a + 1)
            .min(pieces.len());
        let got = pieces[a..b].iter().filter(|x| **x).count();
        let c = if got == b - a {
            done
        } else if got > 0 {
            partial
        } else {
            missing
        };
        let (x, y) = (
            left + (k % cols) as f32 * 7.0,
            top + (k / cols) as f32 * 7.0,
        );
        p.rect_filled(
            winui::rect(egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(6.0, 6.0),
            )),
            1.0,
            c,
        );
    }
}

/// The Files tab's table of a torrent's files, ticked or not: the progress window's (`shown`: 6
/// rows) and the Files window's (torrentfiles.rs, taller). The row whose box was clicked.
pub(crate) fn files_table(
    ui: &mut Ui,
    p: &egui::Painter,
    o: Pos2,
    id: &str,
    rows: &[Vec<String>],
    checks: &[bool],
    shown: usize,
) -> Option<usize> {
    table_sized(
        ui,
        p,
        o,
        id,
        &[
            (25.0, crate::i18n::mark("File")),
            (335.0, crate::i18n::mark("Size")),
            (430.0, crate::i18n::mark("Done")),
        ],
        &[330.0, 425.0],
        rows,
        true,
        Some(checks),
        shown,
    )
}

/// The height of a [`table_sized`] of `shown` rows' body (the progress window's 6: 100 px).
pub(crate) fn table_body_height(shown: usize) -> f32 {
    17.0 * shown as f32 - 2.0
}

/// A table in the connections table's frame: headers at x, column lines at `seps`, rows of
/// text cut to their column (whole on hover), scrolled by the wheel. `checks`: a check box
/// before each row's icon, ticked or not (the Files tab); the row whose box was clicked.
#[allow(clippy::too_many_arguments)]
pub(super) fn table(
    ui: &mut Ui,
    p: &egui::Painter,
    o: Pos2,
    id: &str,
    heads: &[(f32, &str)],
    seps: &[f32],
    rows: &[Vec<String>],
    icons: bool,
    checks: Option<&[bool]>,
) -> Option<usize> {
    table_sized(ui, p, o, id, heads, seps, rows, icons, checks, 6)
}

/// [`table`] showing `shown` rows: it grows down from the same top.
#[allow(clippy::too_many_arguments)]
pub(super) fn table_sized(
    ui: &mut Ui,
    p: &egui::Painter,
    o: Pos2,
    id: &str,
    heads: &[(f32, &str)],
    seps: &[f32],
    rows: &[Vec<String>],
    icons: bool,
    checks: Option<&[bool]>,
    shown: usize,
) -> Option<usize> {
    let mut clicked = None;
    let body_h = table_body_height(shown);
    let outer = r(o, 17.0, 273.0, 513.0, body_h + 28.0);
    winui::solid(p, outer, winui::FIELD_EDGE());
    let (bg, head, line) = (
        winui::FIELD(),
        winui::LIST_HEADER(),
        crate::theme::colors().header_sep,
    );
    fill(
        p,
        o.x + 18.0,
        o.y + 274.0,
        o.x + 528.0,
        o.y + 299.0 + body_h,
        bg,
    );
    fill(p, o.x + 19.0, o.y + 275.0, o.x + 527.0, o.y + 298.0, head);
    for x in seps {
        fill(p, o.x + x, o.y + 275.0, o.x + x, o.y + 298.0, line);
    }
    for (x, h) in heads {
        text(p, o.x + x, o.y + 290.0, crate::i18n::t(h), winui::TEXT());
    }
    let visible = shown;
    // Per window: each progress window scrolls its own tables.
    let key = egui::Id::unique(("torrent-table", id, ui.ctx().viewport_id()));
    let body = r(o, 19.0, 299.0, 508.0, body_h);
    let (first, offset) = table_scroll(ui, key, body, rows.len(), visible);
    let clip = p.with_clip_rect(body);
    let right_of = |c: usize| seps.get(c).map_or(o.x + 520.0, |x| o.x + x - 4.0);
    for (k, row) in rows.iter().skip(first).take(visible + 2).enumerate() {
        let top = o.y + 299.0 + TABLE_ROW * k as f32 - offset;
        // Only rows wholly shown take a click (not one cut off at an edge while it glides).
        let whole = top >= body.top() - 0.01
            && top + TABLE_ROW <= body.top() + TABLE_ROW * visible as f32 + 0.01;
        for (c, cell) in row.iter().enumerate() {
            let mut x = heads.get(c).map_or(o.x + 25.0, |h| o.x + h.0);
            // A table of files (the Files tab): each name after its type's icon, and its
            // check box first when files can be chosen (#39). Only rows wholly shown take a
            // click (the one cut off under the frame doesn't).
            if icons
                && c == 0
                && let Some(&ticked) = checks.and_then(|t| t.get(first + k))
            {
                let mut on = ticked;
                let at = egui::pos2(x - 4.0, top + 2.0);
                if whole && winui::row_check(ui, &clip, at, &mut on, cell, (id, first + k)) {
                    clicked = Some(first + k);
                }
                x += 17.0;
            }
            if icons && c == 0 {
                crate::fileicons::paint_at(
                    ui,
                    &clip,
                    crate::fileicons::Kind::File(cell),
                    egui::pos2(x - 4.0, top),
                    16.0,
                );
                x += crate::fileicons::INDENT - 4.0;
            }
            let (rect, cut) =
                winui::text_fit(&clip, x, top + 12.0, cell, winui::TEXT(), right_of(c));
            // Cells hold info hashes, folders, trackers, peers: copyable (whole on hover).
            winui::copyable(ui, rect.intersect(body), (id, first + k, c), cell, cut);
        }
    }
    // The shared scroll bar (it was a thumb drawn alone, that couldn't be dragged).
    table_scroll_bar(
        ui,
        key,
        r(o, 511.0, 275.0, 17.0, body_h + 24.0),
        rows.len(),
        visible,
    );
    clicked
}

/// This frame's place of a table of `n` rows, `visible` of them shown in `body`, kept under
/// `key`: as the download list, the wheel glides it a pixel at a time to where its notches take
/// it (a touchpad's travel at once), where it jumped whole rows. Gives its first row drawn and
/// how far that row is above the table's top (px, to the screen pixel so the text stays sharp).
pub(super) fn table_scroll(
    ui: &egui::Ui,
    key: egui::Id,
    body: egui::Rect,
    n: usize,
    visible: usize,
) -> (usize, f32) {
    let mut st = ui
        .ctx()
        .data(|d| d.get_temp::<TableScroll>(key))
        .unwrap_or_default();
    let max = n.saturating_sub(visible) as f32 * TABLE_ROW;
    let (wheel, direct) = if ui.rect_contains_pointer(body) {
        winui::wheel_travel(ui, TABLE_ROW, visible as f32)
    } else {
        (0.0, 0.0)
    };
    st.px += direct * TABLE_ROW;
    st.glide.step(ui.ctx(), &mut st.px, wheel * TABLE_ROW, max);
    st.px = st.px.clamp(0.0, max);
    ui.ctx().data_mut(|d| d.insert_temp(key, st));
    let first = (st.px / TABLE_ROW + 1e-4).floor() as usize;
    let ppp = ui.ctx().pixels_per_point();
    let offset = ((st.px - first as f32 * TABLE_ROW) * ppp).round().max(0.0) / ppp;
    (first, offset)
}

/// The scroll bar of a [`table_scroll`] table, when its rows overflow: by whole rows.
pub(super) fn table_scroll_bar(
    ui: &mut egui::Ui,
    key: egui::Id,
    track: egui::Rect,
    n: usize,
    visible: usize,
) {
    if n <= visible {
        return;
    }
    let mut st = ui
        .ctx()
        .data(|d| d.get_temp::<TableScroll>(key))
        .unwrap_or_default();
    let mut pos = st.px / TABLE_ROW;
    if crate::scrollbar::bar(
        ui,
        key.with("bar"),
        track,
        true,
        n as f32,
        visible as f32,
        1.0,
        &mut pos,
    ) {
        let max = n.saturating_sub(visible) as f32 * TABLE_ROW;
        st.px = (pos.round() * TABLE_ROW).clamp(0.0, max);
        st.glide.stop(st.px);
        ui.ctx().data_mut(|d| d.insert_temp(key, st));
        ui.ctx().request_repaint();
    }
}
