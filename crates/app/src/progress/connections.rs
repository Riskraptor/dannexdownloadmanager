//! The connections: the segment bar, and each connection's row (or a video's or a gallery's helper
//! line), numbered.

use super::*;

/// The left edge the connections table's numbers may start at (px from the window's left): the
/// list's own edge, 1 px in. The clip's 6 px margin ([`winui::text_right`]) left two-digit
/// numbers 9 px, which cut every one from 10 up to "..." (#1, 50 connections).
pub(super) const CONN_NUMBER_LEFT: f32 = 20.0;

/// The custom-drawn segment bar (spec "Segment bar"): 13-row gradients, red start columns;
/// `segments`: each one's start and bytes done, as drawn ([`Glide`]).
pub(super) fn segment_bar(p: &egui::Painter, o: Pos2, item: &Item, segments: &[(u64, f64)]) {
    let (x0, y0) = (o.x + 17.0, o.y + 249.0);
    let border = crate::theme::colors().segment_border;
    fill(p, x0, y0, x0 + 512.0, y0, border);
    fill(p, x0, y0 + 14.0, x0 + 512.0, y0 + 14.0, border);
    fill(p, x0, y0, x0, y0 + 14.0, border);
    fill(p, x0 + 512.0, y0, x0 + 512.0, y0 + 14.0, border);
    for (x, y) in [
        (x0, y0),
        (x0 + 512.0, y0),
        (x0, y0 + 14.0),
        (x0 + 512.0, y0 + 14.0),
    ] {
        px(p, x, y, crate::theme::colors().segment_corner);
    }
    // Paint a run of columns [a, b] with one of the three row profiles.
    let run = |a: f32, b: f32, which: usize| {
        if b < a {
            return;
        }
        for (row, cols) in seg_rows().enumerate() {
            let c = match which {
                0 => cols.0,
                1 => cols.1,
                _ => cols.2,
            };
            fill(p, a, y0 + 1.0 + row as f32, b, y0 + 1.0 + row as f32, c);
        }
    };
    let ix = o.x + 18.0;
    run(ix, ix + 510.0, 0);
    let Some(total) = item.size.filter(|t| *t > 0) else {
        return;
    };
    let col = |v: f64| (v / total as f64 * 511.0).floor() as f32;
    for &(start, done) in segments {
        let sx = ix + col(start as f64);
        let n = col(done);
        if n > 1.0 {
            run(sx + 1.0, (sx + n - 1.0).min(ix + 510.0), 1);
        }
        run(sx, sx, 2);
    }
}

/// What a video DDM's own downloader leaves to its helper program (#769) is doing, as one short
/// line in plain words: no program's name, no protocol (the owner's rule: what people see says
/// what happens, never how).
pub(super) fn helper_line(why: &mdm_tools::streams::Helper) -> String {
    use mdm_tools::streams::Helper;
    match why {
        // i18n: progress window, a video's line: the video is a live broadcast, recorded as it goes on
        Helper::Live => crate::i18n::t("Recording the live broadcast").to_owned(),
        // i18n: progress window, a video's line: the video is copy-protected (DRM): it can't be saved
        Helper::Protected => {
            crate::i18n::t("This video is copy-protected and can't be saved").to_owned()
        }
        // A rare kind of video in pieces, a streaming server's own protocol, no address for
        // DDM's downloader: to the person, it downloads.
        // i18n: progress window, a video's line: the video is being downloaded (in the helper program's own way, which isn't named)
        Helper::Pieces(_) | Helper::Protocol(_) | Helper::NoAddress => {
            crate::i18n::t("Downloading the video").to_owned()
        }
    }
}

/// A running gallery's files (#769), as one short line: how many are done (saved, or there
/// already) of those listed so far; while it finds the first, that; when they come all at once by
/// the helper program (one that can't list them), how many so far. Plain words, no program's name.
pub(super) fn gallery_line(f: &mdm_tools::gallery::Files) -> String {
    let done = crate::i18n::integer(f.done);
    match f.listed {
        Some(0) if f.listing => {
            // i18n: progress window, a gallery's line: DDM is finding the files the gallery holds, which it then downloads
            crate::i18n::t("Finding the gallery's files...").to_owned()
        }
        Some(total) => crate::i18n::fmt(
            // i18n: progress window, a gallery's line: {done} = its files saved or found already there, {n} = the files it has (as listed so far)
            &crate::i18n::tn(total, "{done} of {n} file", "{done} of {n} files"),
            &[("done", &done)],
        ),
        // i18n: progress window, a gallery's line: {n} = its files saved or found already there (how many there are in all isn't known)
        None => crate::i18n::tn(f.done, "{n} file so far", "{n} files so far"),
    }
}

/// Where the connections table's numbers end (window x of their right edge), the widest of them
/// `widest` px: IDM's place (33) while it fits right of the list's edge, else as far right as the
/// widest needs (its column's line is at 48), so "10" to "128" are never cut to "...".
pub(super) fn conn_number_right(ox: f32, widest: f32) -> f32 {
    (ox + 33.0).max(ox + CONN_NUMBER_LEFT + widest.ceil() - 1.0)
}

/// Connection `id`'s number (from 1) in its row, ending at `right`; whether it had to be cut.
pub(super) fn conn_number(p: &egui::Painter, ox: f32, right: f32, baseline: f32, id: u32) -> bool {
    winui::text_right_within(
        p,
        right,
        baseline,
        &(id + 1).to_string(),
        winui::TEXT(),
        ox + CONN_NUMBER_LEFT,
    )
    .1
}

/// The connections table (spec "Connections table"): N. · Downloaded · Info.
pub(super) fn connections(ui: &mut Ui, p: &egui::Painter, o: Pos2, snap: Option<&Snapshot>) {
    let outer = r(o, 17.0, 273.0, 513.0, 128.0);
    winui::solid(p, outer, winui::FIELD_EDGE());
    fill(
        p,
        o.x + 18.0,
        o.y + 274.0,
        o.x + 528.0,
        o.y + 399.0,
        // Every list's background (IDM's light one is mauve, #beb4be: the only one of its kind).
        winui::FIELD(),
    );
    fill(
        p,
        o.x + 19.0,
        o.y + 275.0,
        o.x + 510.0,
        o.y + 298.0,
        winui::LIST_HEADER(),
    );
    for x in [48.0, 148.0, 498.0] {
        fill(
            p,
            o.x + x,
            o.y + 275.0,
            o.x + x,
            o.y + 298.0,
            crate::theme::colors().header_sep,
        );
    }
    // Column titles end in "..." before their column's line, whole on hover; a long "N." starts
    // at the list's edge.
    let head = |x: f32, right: f32, label: &str| {
        let (rc, cut) = winui::text_fit(p, o.x + x, o.y + 290.0, label, winui::TEXT(), o.x + right);
        if cut {
            winui::full_on_hover(ui, rc, ("connections-head", x as i32), label);
        }
    };
    // i18n: column title: connection number (abbreviation of Number)
    let number = crate::i18n::t("N.");
    let x = if winui::text_width(p, number) > 21.0 {
        21.0
    } else {
        25.0
    };
    head(x, 46.0, number);
    // i18n: column title: bytes this connection received
    head(55.0, 146.0, crate::i18n::t("Downloaded"));
    // i18n: column title: what the connection is doing
    head(155.0, 496.0, crate::i18n::t("Info"));
    // Listed by connection number, like IDM (the engine reports them in arrival order).
    let mut conns: Vec<_> = snap
        .map(|s| s.connections.iter().collect())
        .unwrap_or_default();
    conns.sort_by_key(|c| c.id);
    let key = egui::Id::unique(("conn-scroll", ui.ctx().viewport_id()));
    let visible = 6usize;
    let body = r(o, 19.0, 299.0, 492.0, 100.0);
    let (first, offset) = table_scroll(ui, key, body, conns.len(), visible);
    let clip = p.with_clip_rect(body);
    // Every number whole, however many connections (up to 128): all end where the widest does.
    let widest = conns
        .last()
        .map_or(0.0, |c| winui::text_width(p, &(c.id + 1).to_string()));
    let number_right = conn_number_right(o.x, widest);
    for (k, c) in conns.iter().skip(first).take(visible + 2).enumerate() {
        let top = o.y + 299.0 + TABLE_ROW * k as f32 - offset;
        conn_number(&clip, o.x, number_right, top + 12.0, c.id);
        text(
            &clip,
            o.x + 56.0,
            top + 12.0,
            &size3(c.downloaded),
            winui::TEXT(),
        );
        let info = match &c.status {
            ConnStatus::Connecting => crate::i18n::t("Connecting...").to_owned(),
            ConnStatus::Receiving => crate::i18n::t("Receiving data...").to_owned(),
            ConnStatus::Retrying(e) => crate::i18n::fmt(
                // i18n: a connection's state: it failed and tries again, with the reason
                crate::i18n::t("Retrying: {error}"),
                &[("error", &crate::errors::describe(e))],
            ),
            ConnStatus::Finished => String::new(),
            ConnStatus::Failed(e) => crate::i18n::fmt(
                crate::i18n::t("Error: {error}"),
                &[("error", &crate::errors::describe(e))],
            ),
            ConnStatus::Retired { host, cap } => crate::i18n::fmt(
                // i18n: a connection closed because the server allows only that many
                &crate::i18n::tn(
                    u64::from(*cap),
                    "{host} allows {n} connection",
                    "{host} allows {n} connections",
                ),
                &[("host", &crate::i18n::ltr(host))],
            ),
        };
        // The Info column ends at the scrollbar.
        let (rect, cut) = winui::text_fit(
            &clip,
            o.x + 155.0,
            top + 12.0,
            &info,
            winui::TEXT(),
            o.x + 507.0,
        );
        if cut {
            winui::full_on_hover(ui, rect.intersect(body), ("conn", c.id), &info);
        }
    }
    // The shared scroll bar (a track and thumb were drawn here, the thumb not draggable).
    table_scroll_bar(
        ui,
        key,
        r(o, 511.0, 275.0, 17.0, 124.0),
        conns.len(),
        visible,
    );
}
