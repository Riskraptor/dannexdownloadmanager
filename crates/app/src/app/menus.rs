//! The menus: the menu bar's (IDM 6.42's, notes/ui-spec/menus.md), the queues' drop-downs and the
//! list's right-click menus on a row or on blank space.

use super::*;

/// The open popup menu (other than the menu bar's); notes/ui-spec/menus.md "Context menus".
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Popup {
    /// List header right-click: the column chooser, and the column right-clicked (its filter).
    Columns(Option<usize>),
    /// List right-click on a row, or on blank space (None).
    Item(Option<u64>),
    /// The Start Queue (true) / Stop Queue drop-down arrows.
    QueueDrop(bool),
    /// The Add Torrent button's drop-down arrow.
    TorrentDrop,
    /// Right-click on the drop target.
    DropTarget,
    /// Categories right-click: the node's view (None: the "Queues" root).
    Node(Option<View>),
}

impl App {
    // ---- menus -----------------------------------------------------------------------------

    /// IDM 6.42's menus (notes/ui-spec/menus.md); unbuilt features are greyed out.
    pub(super) fn menu_entries(&self, index: usize) -> Vec<MenuEntry<Cmd>> {
        use MenuEntry::{Item as I, Sep, Sub};
        let sel = self.selected_item();
        let picked = self.chosen(None);
        let running = picked.iter().any(|i| i.live.is_some());
        let resumable = picked.iter().any(|i| i.can_start());
        let deletable = picked.iter().any(|i| i.live.is_none());
        let any_running = self.items.iter().any(|i| i.live.is_some() && i.listed());
        #[allow(unused_mut)]
        let mut menu = match index {
            0 => vec![
                I("&Add new download", Cmd::AddUrl, true, false),
                I("Open &torrent file...", Cmd::OpenTorrentFile, true, false),
                I("Add &magnet link...", Cmd::AddMagnet, true, false),
                I("Torrent setti&ngs...", Cmd::TorrentSettings, true, false),
                // i18n: Tasks menu: open a Metalink file (.metalink, .meta4: a list of files and the servers that have them)
                I("Open Metalin&k file...", Cmd::OpenMetalinkFile, true, false),
                I("Add &batch download", Cmd::Batch(false), true, false),
                I(
                    "Add batch download from &clipboard",
                    Cmd::Batch(true),
                    true,
                    false,
                ),
                I(
                    "Download &video from a page...",
                    Cmd::VideoPage,
                    true,
                    false,
                ),
                // i18n: Tasks menu: open Link grabber (links in packages, to tick before adding)
                I("&Link grabber...", Cmd::LinkGrabber, true, false),
                // i18n: Tasks menu: open Site grabber (explore a web site and download its files)
                I("Run &site grabber...", Cmd::SiteGrabber, true, false),
                // i18n: Tasks menu: the latest Site grabber projects, each opening its window
                Sub(
                    "Grabber pro&jects",
                    !self.grabs.store.projects.is_empty(),
                    self.grab_menu_entries(),
                ),
                // i18n: Tasks menu: the RSS feeds window (feeds of torrents, and rules that add them)
                I("&RSS feeds...", Cmd::RssFeeds(false), true, false),
                Sep,
                I(
                    "Show &drop target",
                    Cmd::ToggleDropTarget,
                    true,
                    self.settings.drop_target,
                ),
                Sep,
                // i18n: verb: save the download list to a file
                Sub(
                    "E&xport",
                    !self.items.is_empty(),
                    vec![
                        // i18n: Tasks → Export: save the list as an IDM list file (.ef2), which IDM imports; keep "IDM" and ".ef2"
                        I(
                            "As an &IDM list (.ef2)...",
                            Cmd::Export { ef2: true },
                            true,
                            false,
                        ),
                        // i18n: Tasks → Export: save the list's addresses as a text file, one per line
                        I(
                            "As a &text file...",
                            Cmd::Export { ef2: false },
                            true,
                            false,
                        ),
                    ],
                ),
                // i18n: verb: load downloads from a file
                Sub(
                    "&Import",
                    true,
                    vec![
                        // i18n: Tasks → Import: read an IDM list file (.ef2, as IDM exports it); keep "IDM" and ".ef2"
                        I(
                            "From an &IDM list (.ef2)...",
                            Cmd::Import { ef2: true },
                            true,
                            false,
                        ),
                        // i18n: Tasks → Import: read the addresses in a text file (or a web page saved as a file)
                        I(
                            "From a &text file...",
                            Cmd::Import { ef2: false },
                            true,
                            false,
                        ),
                    ],
                ),
                Sep,
                I("&Exit", Cmd::Exit, true, false),
            ],
            1 => vec![
                I("&Stop Download", Cmd::Stop, running, false),
                // i18n: verb: removes the selected download from the list
                I("&Remove", Cmd::Remove, deletable, false),
                // i18n: verb: removes the selected download from the list and its file from the disk
                I("Remove with &files", Cmd::RemoveWithFiles, deletable, false),
                I("&Download Now", Cmd::Resume, resumable, false),
                // i18n: verb: download the file again from the start
                I(
                    "R&edownload",
                    Cmd::Redownload,
                    sel.is_some_and(|i| self.can_redownload(i.id)),
                    false,
                ),
            ],
            2 => vec![
                I("&Pause All", Cmd::StopAll, any_running, false),
                I("&Stop All", Cmd::StopAll, any_running, false),
                Sep,
                I(
                    "&Delete All Completed",
                    Cmd::DeleteCompleted,
                    self.items
                        .iter()
                        .any(|i| i.status == Status::Complete && i.listed()),
                    false,
                ),
                // i18n: Downloads menu: removes the part files of downloads no longer in the list (left by a crash, a power cut, a removed download)
                I(
                    "Clean &Up Temporary Files",
                    Cmd::CleanUpTemp,
                    self.leftovers.is_none(),
                    false,
                ),
                Sep,
                I("Find (Ctrl-F)", Cmd::Find, true, false),
                I("Find Next (F3)", Cmd::FindNext, true, false),
                I(
                    "Select &all",
                    Cmd::SelectAll,
                    !self.shown_ids().is_empty(),
                    false,
                ),
                Sep,
                I("S&cheduler", Cmd::Scheduler, true, false),
                Sub("Start &queue", true, self.queue_drop(true)),
                Sub("S&top queue", true, self.queue_drop(false)),
                Sep,
                Sub(
                    "Speed &Limiter",
                    true,
                    vec![
                        I(
                            "Turn o&n",
                            Cmd::LimiterOn,
                            true,
                            self.settings.speed_limit_on,
                        ),
                        I(
                            "Turn of&f",
                            Cmd::LimiterOff,
                            true,
                            !self.settings.speed_limit_on,
                        ),
                        Sep,
                        I("S&ettings...", Cmd::LimiterSettings, true, false),
                    ],
                ),
                // i18n: Downloads menu: opens the Statistics window (what was downloaded today, this week, this month; the busiest days, sites and file types)
                I("Stat&istics...", Cmd::Statistics, true, false),
                Sep,
                I("&Options", Cmd::Options, true, false),
            ],
            3 => vec![
                I(
                    if self.show_categories {
                        crate::i18n::mark("Hide &categories")
                    } else {
                        crate::i18n::mark("Show &categories")
                    },
                    Cmd::ToggleCategories,
                    true,
                    false,
                ),
                self.arrange_menu(),
                self.toolbar_menu(),
                self.tray_menu(),
                // i18n: View menu: opens the window choosing which columns the download list shows, and their order
                I("Choose c&olumns...", Cmd::CustomizeColumns, true, false),
                I(
                    // i18n: View menu: a check item; dark mode on while the system (Windows, the desktop) is set to dark
                    "Follow the system's &dark mode",
                    Cmd::ToggleDarkMode,
                    true,
                    self.settings.dark_mode_support,
                ),
                // DDM's (#60): dark whatever the system says (a desktop that reports nothing).
                I(
                    // i18n: View menu: a check item; dark mode on, whatever the system's setting
                    "Always dark",
                    Cmd::ToggleAlwaysDark,
                    true,
                    self.settings.always_dark,
                ),
                self.font_menu(),
                Sep,
                // IDM's place for it; a window rather than IDM's submenu, which 86 languages
                // would make taller than the screen (langpick.rs).
                I("&Language...", Cmd::ChooseLanguage, true, false),
            ],
            // IDM's Help menu, ours (D-020): pages of the user guide (docs/); "Video downloads" stands
            // where IDM has "Grabber Help"; "Report a problem" where "Contact IDM Support";
            // Keyboard shortcuts added; no Tip of the Day.
            _ => vec![
                I("H&elp contents", Cmd::HelpPage("README.md"), true, false),
                I(
                    "&Tutorials",
                    Cmd::HelpPage("README.md#tutorials"),
                    true,
                    false,
                ),
                I(
                    "&Scheduler and queues",
                    Cmd::HelpPage("queues-and-scheduler.md"),
                    true,
                    false,
                ),
                I("&Video downloads", Cmd::HelpPage("video.md"), true, false),
                I("&Keyboard shortcuts", Cmd::Shortcuts, true, false),
                Sep,
                I("DDM &Home Page", Cmd::HelpPage(""), true, false),
                I("&Report a problem", Cmd::ReportProblem, true, false),
                Sep,
                I("Check for &updates...", Cmd::CheckUpdates, true, false),
                Sep,
                I("&About DDM", Cmd::About, true, false),
                I("Tell a &friend", Cmd::TellFriend, true, false),
                // i18n: Help menu: opens Dannex's donation page (GitHub Sponsors) in the browser; keep the short name DDM as it is
                I("Su&pport DDM", Cmd::Sponsor, true, false),
            ],
        };
        // View → Theme (DDM's, #45), after Always dark; then Draw with (#89, #206), where wgpu is
        // built (Windows, Linux, macOS: renderer.rs).
        if let Some(at) = menu
            .iter()
            .position(|e| matches!(e, I(_, Cmd::ToggleAlwaysDark, ..)))
        {
            menu.insert(at + 1, self.theme_menu());
            menu.insert(at + 2, self.interface_size_menu());
            #[cfg(windows)]
            menu.insert(at + 3, self.text_menu());
            #[cfg(gpu)]
            menu.insert(
                at + if cfg!(windows) { 4 } else { 3 },
                self.draw_with_menu(),
            );
        }
        menu
    }

    /// Start Queue / Stop Queue drop-downs (and the Downloads menu's submenus).
    pub(super) fn queue_drop(&self, start: bool) -> Vec<MenuEntry<Cmd>> {
        let label = |q: usize| {
            let template = if start {
                crate::i18n::t("Start {queue}")
            } else {
                crate::i18n::t("Stop {queue}")
            };
            crate::chrome::label(&crate::i18n::fmt(
                template,
                // A name's "&" is text, not an access key (Tom & Jerry lost it).
                &[(
                    "queue",
                    &crate::i18n::t(&self.queues[q].name).replace('&', "&&"),
                )],
            ))
        };
        (0..self.queues.len())
            .map(|q| {
                let running = self.queues[q].running;
                if start {
                    MenuEntry::Item(label(q), Cmd::StartQueue(q), !running, false)
                } else {
                    MenuEntry::Item(label(q), Cmd::StopQueue(q), running, false)
                }
            })
            .collect()
    }

    /// IDM 6.42's list menu (notes/ui-spec/menus.md "Context menus"): on a row, or on blank
    /// space (`None`: nothing selected, almost everything greyed).
    /// The list's right-click menu. With several downloads selected it acts on all of them
    /// (Resume, Stop, Remove, queues); commands for one download need one.
    /// View → Arrange files, also on the list's blank space.
    pub(super) fn arrange_menu(&self) -> MenuEntry<Cmd> {
        use MenuEntry::{Item as I, Sub};
        Sub("Arrange files", true, {
            let sorted = self.list.sort.map(|s| s.0);
            let by = |label, col: Option<usize>| I(label, Cmd::Arrange(col), true, sorted == col);
            vec![
                by("By &Order Of Addition", None),
                by("By &File Name", Some(0)),
                by("By &Size", Some(2)),
                by("By St&atus", Some(3)),
                by("By &Time Left", Some(4)),
                by("By Transfer &Rate", Some(5)),
                by("By &Last Try Date", Some(6)),
                by("By Date &Added", Some(list::ADDED)),
                by("By &Description", Some(7)),
                by("By Save &Path", Some(list::BY_SAVE_PATH)),
                by("By R&eferer", Some(list::BY_REFERER)),
                by("By Parent &Web Page", Some(list::BY_PAGE)),
            ]
        })
    }

    /// Right-click on the list's blank space: what applies to the whole list (IDM greys the
    /// download menu there).
    pub(super) fn blank_entries(&self) -> Vec<MenuEntry<Cmd>> {
        use MenuEntry::{Item as I, Sep};
        vec![
            I("&Add new download...", Cmd::AddUrl, true, false),
            Sep,
            I(
                "Select &all",
                Cmd::SelectAll,
                !self.shown_ids().is_empty(),
                false,
            ),
            self.arrange_menu(),
            I(
                "Clear all &filters",
                Cmd::ClearFilters,
                self.list.filters.iter().any(Option::is_some),
                false,
            ),
            Sep,
            I(
                "&Delete All Completed",
                Cmd::DeleteCompleted,
                self.items
                    .iter()
                    .any(|i| i.status == Status::Complete && i.listed()),
                false,
            ),
        ]
    }

    pub(super) fn item_entries(&self, id: Option<u64>) -> Vec<MenuEntry<Cmd>> {
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let chosen = if id.is_some() {
            self.chosen(id)
        } else {
            Vec::new()
        };
        let it = (chosen.len() == 1).then(|| chosen[0]);
        let sel = it.is_some();
        let done = it.is_some_and(|i| i.status == Status::Complete);
        let live = it.is_some_and(|i| i.live.is_some());
        let any = !chosen.is_empty();
        let any_live = chosen.iter().any(|i| i.live.is_some());
        let any_resumable = chosen.iter().any(|i| i.can_start());
        let any_idle = chosen.iter().any(|i| i.live.is_none());
        let all_done = any && chosen.iter().all(|i| i.status == Status::Complete);
        let queue_of = |q: &Queue| chosen.iter().all(|i| q.items.contains(&i.id)) && any;
        let in_queue = self
            .queues
            .iter()
            .any(|q| chosen.iter().any(|i| q.items.contains(&i.id)));
        // The other queues they can go to; finished files only to synchronization queues.
        let targets: Vec<MenuEntry<Cmd>> = self
            .queues
            .iter()
            .enumerate()
            .filter(|(_, q)| !queue_of(q) && (!all_done || q.sync))
            .map(|(q, queue)| {
                I(
                    crate::chrome::label(&crate::i18n::t(&queue.name).replace('&', "&&")),
                    Cmd::AddToQueue(q),
                    true,
                    false,
                )
            })
            .chain([I("Create new queue", Cmd::NewQueue, true, false)])
            .collect();
        let open = self.settings.double_click_open;
        // Several selected: enabled when one of them can go; the others are said to stay (#333).
        let movable = chosen.iter().any(|i| can_move(i));
        let redo = it.is_some_and(|i| self.can_redownload(i.id));
        let extractable = chosen.iter().any(|i| {
            i.status == Status::Complete
                && i.torrent.is_none()
                && i.video.is_none()
                && crate::extractq::archive_name(&i.file_name)
        });
        let torrent = it.and_then(|i| i.torrent.as_ref().map(|t| (i, t)));
        let mut entries = vec![
            // i18n: verb: opens the downloaded file
            I("Open", Cmd::Open, done, false),
            I("Open with...", Cmd::OpenWith, done, false),
            // A torrent's folder is there while it downloads (#354).
            I(
                "Open folder",
                Cmd::OpenFolder,
                done || it.is_some_and(|i| i.torrent.is_some()),
                false,
            ),
            Sep,
            I("Move/Rename (Ctrl-M)", Cmd::MoveRename, movable, false),
            Sep,
            // i18n: verb: download the file again from the start
            I("Redownload", Cmd::Redownload, redo, false),
            Sep,
            I("Resume Download", Cmd::Resume, any_resumable, false),
            I("Stop Download", Cmd::Stop, any_live, false),
            // Its progress window, opened or back from the notification area: a queue's
            // downloads start with theirs hidden, and a window hidden or closed comes back here.
            I(
                // i18n: right-click menu of a download: opens its progress window (or brings it back from the notification area)
                "Show progress",
                Cmd::ShowProgress,
                it.is_some_and(|i| i.live.is_some() || i.status != Status::Complete),
                false,
            ),
            Sep,
            I(
                "Refresh download address",
                Cmd::RefreshAddress,
                sel && !live && !done,
                false,
            ),
        ];
        // The site's bot check refused it (#376): the browser it came from downloads it.
        if it.is_some_and(crate::handback::offered) {
            entries.push(
                // i18n: right-click menu of a download the site's bot check (Cloudflare) refused: open its address in the web browser, which downloads the file itself (and keeps it)
                I("Download with the browser", Cmd::WithBrowser, true, false),
            );
        }
        entries.extend([
            I("Verify checksum...", Cmd::VerifyChecksum, sel, false),
            // i18n: verb: unpack the downloaded archive (zip, 7z, rar) into its folder
            I("Extract", Cmd::Extract, extractable, false),
            Sep,
            // i18n: verb: removes the download from the list
            I("Remove", Cmd::Remove, any_idle, false),
            // i18n: verb: removes the download from the list and its file from the disk
            I("Remove with files", Cmd::RemoveWithFiles, any_idle, false),
            Sep,
            Sub(
                if in_queue {
                    crate::i18n::mark("Move to queue")
                } else {
                    crate::i18n::mark("Add to queue")
                },
                any,
                targets,
            ),
            I("Delete from queue", Cmd::RemoveFromQueue, in_queue, false),
            Sep,
            Sub(
                "On Double click",
                true,
                vec![
                    // i18n: what double-clicking a download does — verb: open the file
                    Radio("Open", Cmd::DoubleClickOpen(true), true, open),
                    // i18n: what double-clicking a download does — noun: show its properties
                    Radio("Properties", Cmd::DoubleClickOpen(false), true, !open),
                ],
            ),
            Sep,
            // i18n: noun: opens the download's Properties window
            I("Properties", Cmd::Properties, sel, false),
        ]);
        // A torrent's own actions (#39), after Verify checksum.
        if let Some((i, spec)) = torrent {
            let busy = self.torrent_host.busy(&spec.hash);
            // Running or shared now (the session has it going): Reannounce has someone to ask.
            let going = self
                .torrent_host
                .get()
                .and_then(|t| t.seen(&spec.hash))
                .is_some_and(|s| {
                    matches!(
                        s.phase,
                        mdm_torrent::Phase::Downloading | mdm_torrent::Phase::Finished
                    )
                });
            let at = entries
                .iter()
                .position(|e| matches!(e, I(_, Cmd::VerifyChecksum, _, _)))
                .map_or(entries.len(), |p| p + 1);
            entries.insert(
                at,
                Sub(
                    // i18n: right-click menu: the submenu of a torrent's own commands
                    "Torrent",
                    true,
                    vec![
                        // i18n: right-click → Torrent submenu: open the selected torrent's file list
                        I("Files...", Cmd::TorrentFiles, true, false),
                        // i18n: right-click → Torrent submenu: opens a browser media player that reads the torrent while its pieces arrive
                        I(
                            "Play while downloading",
                            Cmd::PlayTorrent,
                            going && player::has_playable_file(&self.torrent_host, &spec.hash),
                            false,
                        ),
                        Sep,
                        // i18n: right-click → Torrent: read every piece of the torrent's files again and check it
                        I(
                            "Recheck files",
                            Cmd::TorrentRecheck,
                            !busy && i.status != Status::Queued,
                            false,
                        ),
                        // i18n: right-click → Torrent: ask the torrent's trackers for peers again now
                        I("Reannounce", Cmd::TorrentReannounce, !busy && going, false),
                        I("Add trackers...", Cmd::TorrentAddTrackers, !busy, false),
                        Sep,
                        // i18n: right-click → Torrent, a choice with a check mark: download the torrent's files from their start, to watch a video while it downloads
                        I(
                            "Download in order",
                            Cmd::TorrentInOrder,
                            true,
                            spec.in_order,
                        ),
                        // i18n: right-click → Torrent, a choice with a check mark (with Download in order): each file's first and last pieces first, which a video player needs to start
                        I(
                            "First and last pieces first",
                            Cmd::TorrentEndsFirst,
                            spec.in_order,
                            spec.in_order && spec.ends_first,
                        ),
                        Sep,
                        I("Copy magnet link", Cmd::TorrentCopyMagnet, true, false),
                    ],
                ),
            );
        }
        entries
    }
}
