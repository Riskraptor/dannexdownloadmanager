//! Torrent RSS feeds (#39): fetched in the background, their rules' articles looked up and added.

use super::*;

/// What an RSS fetch or look-up sends back to the app (`App::rss_tick`).
pub(super) enum RssEvent {
    Fetched(u64, Result<crate::rss::Parsed, String>),
    /// An article a rule took, looked up: what its torrent holds.
    Looked {
        rule: u64,
        feed: u64,
        title: String,
        res: Result<mdm_torrent::Meta, String>,
    },
}

impl App {
    /// A torrent action (#39) on download `id`, in the background; what it did goes in the
    /// download's event log (`torrent_outcomes`). One at a time per torrent.
    /// RSS (#39), every 2 s: what fetches and look-ups sent back (new articles through the
    /// rules, the torrents they took added), then the feeds that are due fetched.
    pub(super) fn rss_tick(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let events = std::mem::take(&mut *self.rss_events.lock().unwrap());
        let mut save = false;
        for ev in events {
            match ev {
                RssEvent::Fetched(id, res) => {
                    self.rss_busy.remove(&id);
                    save = true;
                    match res {
                        Ok(parsed) => {
                            for (rule, article) in self.rss.fetched(id, parsed, now) {
                                self.rss_look_up(rule, id, article);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("rss: feed {id}: {e}");
                            self.rss.failed(id, e, now);
                        }
                    }
                }
                RssEvent::Looked {
                    rule,
                    feed,
                    title,
                    res,
                } => self.rss_add(rule, feed, &title, res),
            }
        }
        for id in self.rss.due(now) {
            self.rss_fetch(id);
        }
        if save {
            self.save_rss();
        }
        if let Some(Err(e)) = self.rss_saver.outcome() {
            tracing::warn!("rss: not saved: {e}");
        }
    }

    /// On the RSS saver's thread; how it went is logged at the next tick (`rss_tick`).
    pub(super) fn save_rss(&self) {
        let Some(path) = crate::rss::Store::path() else {
            return;
        };
        match self.rss.encode() {
            Ok(json) => self.rss_saver.queue(path, json),
            Err(e) => tracing::warn!("rss: {} not saved: {e}", path.display()),
        }
    }

    /// Fetch feed `id` in the background (once at a time), with DDM's own connection settings.
    pub(super) fn rss_fetch(&mut self, id: u64) {
        let Some(feed) = self.rss.feeds.iter().find(|f| f.id == id) else {
            return;
        };
        if !self.rss_busy.insert(id) {
            return;
        }
        let url = feed.url.clone();
        let out = self.rss_events.clone();
        self.rt.spawn(async move {
            let res = match url::Url::parse(&url) {
                Ok(url) => {
                    let remote = crate::torrents::Remote {
                        url,
                        parts: crate::app::RequestParts {
                            headers: Default::default(),
                            opts: crate::settings::own_net(),
                        },
                    };
                    match crate::torrents::fetch_small(remote, crate::rss::MAX_FEED).await {
                        Ok(Some(bytes)) => crate::rss::parse(&String::from_utf8_lossy(&bytes)),
                        // i18n: RSS: a feed bigger than DDM reads (5 MB)
                        Ok(None) => Err(crate::i18n::t("larger than a feed can be").to_owned()),
                        Err(e) => Err(e),
                    }
                }
                Err(e) => Err(e.to_string()),
            };
            out.lock().unwrap().push(RssEvent::Fetched(id, res));
        });
    }

    /// A rule took an article: what its torrent holds is looked up (a magnet asks the swarm, a
    /// .torrent's address is fetched), then `rss_add` adds it.
    pub(super) fn rss_look_up(&mut self, rule: u64, feed: u64, article: crate::rss::Article) {
        let (host, known, out) = (
            self.torrent_host.clone(),
            self.session_hashes(),
            self.rss_events.clone(),
        );
        self.rt.spawn(async move {
            let res = async {
                let src = match mdm_torrent::Source::magnet(&article.torrent) {
                    Some(m) => m,
                    None => {
                        let url =
                            url::Url::parse(article.torrent.trim()).map_err(|e| e.to_string())?;
                        let remote = crate::torrents::Remote {
                            url,
                            parts: crate::app::RequestParts {
                                headers: Default::default(),
                                opts: crate::settings::own_net(),
                            },
                        };
                        mdm_torrent::Source::File(crate::torrents::fetch_torrent(remote).await?)
                    }
                };
                let t = host.session(known).await?;
                t.metadata(&src, std::time::Duration::from_secs(180), None)
                    .await
                    .map_err(|e| crate::errors::torrent(&e))
            }
            .await;
            out.lock().unwrap().push(RssEvent::Looked {
                rule,
                feed,
                title: article.title,
                res,
            });
        });
    }

    /// Add what a rule took, as Add torrent's Download Now (or Later) would: every file, to the
    /// rule's folder (else the Torrents category's). Not when the list has it, or files of its
    /// names are already in the folder (#40: nothing is written into them unasked).
    pub(super) fn rss_add(
        &mut self,
        rule: u64,
        feed: u64,
        title: &str,
        res: Result<mdm_torrent::Meta, String>,
    ) {
        let Some(r) = self.rss.rules.iter().find(|r| r.id == rule).cloned() else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let title_i = crate::i18n::isolate(title);
        let meta = match res {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("rss: rule {:?}: {title}: {e}", r.name);
                self.rss.note(
                    now,
                    crate::i18n::fmt(
                        // i18n: RSS feeds window, under the rules: a rule's article could not be added, with why
                        crate::i18n::t("{title}: not added: {error}"),
                        &[("title", &title_i), ("error", &e)],
                    ),
                );
                self.save_rss();
                return;
            }
        };
        if self.torrent_hashes().contains(&meta.info_hash) {
            self.rss.note(
                now,
                crate::i18n::fmt(
                    // i18n: RSS feeds window, under the rules: a rule's article is a torrent the list already has
                    crate::i18n::t("{title}: already in the list"),
                    &[("title", &title_i)],
                ),
            );
            self.save_rss();
            return;
        }
        let dir = if r.folder.trim().is_empty() {
            self.settings
                .category_dir(Category::Torrents, &self.downloads)
        } else {
            PathBuf::from(r.folder.trim())
        };
        // Whether files of its names are in its folder already is looked at beside the window
        // (#743); it is added, or said not to be, once the answer comes (`rss_checked`).
        let add = crate::torrentdlg::TorrentAdd {
            meta,
            dir,
            files: None,
            category: Category::Torrents,
            start_now: !r.later,
            from_magnet: false,
            // RSS rules download as torrents usually do: the rarest pieces first.
            in_order: false,
            queue: None,
            quiet: false,
        };
        let (rule, title) = (r.name.clone(), title.to_owned());
        self.check_torrent_beside(move || TorrentChecked::Rss {
            there: !add.meta.existing_in(&add.dir).is_empty(),
            add: Box::new(add),
            rule,
            feed,
            title,
        });
    }

    /// A torrent RSS rule `rule` found in `feed` (`rss_add`), its folder looked at beside the
    /// window: added, unless files of its names are there (`there`) or the list has it by now.
    pub(super) fn rss_checked(
        &mut self,
        add: crate::torrentdlg::TorrentAdd,
        there: bool,
        rule: &str,
        feed: u64,
        title: &str,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let title_i = crate::i18n::isolate(title);
        // Added another way while its folder was looked at.
        if self.torrent_hashes().contains(&add.meta.info_hash) {
            self.rss.note(
                now,
                crate::i18n::fmt(
                    crate::i18n::t("{title}: already in the list"),
                    &[("title", &title_i)],
                ),
            );
            self.save_rss();
            return;
        }
        let dir = add.dir.clone();
        if there {
            tracing::warn!(
                "rss: rule {:?}: {title}: its files are already in {}",
                rule,
                dir.display()
            );
            self.rss.note(
                now,
                crate::i18n::fmt(
                    // i18n: RSS feeds window, under the rules: not added, because files of its names are already in the folder (DDM never writes into them unasked)
                    crate::i18n::t(
                        "{title}: not added: files of its names are already in {folder}",
                    ),
                    &[
                        ("title", &title_i),
                        ("folder", &crate::i18n::ltr(&dir.display().to_string())),
                    ],
                ),
            );
            self.save_rss();
            return;
        }
        let hash = add.meta.info_hash.clone();
        self.add_torrent(add);
        let feed = self
            .rss
            .feeds
            .iter()
            .find(|f| f.id == feed)
            .map(|f| f.name().to_owned())
            .unwrap_or_default();
        self.rss.note(
            now,
            crate::i18n::fmt(
                // i18n: RSS feeds window, under the rules: {title} was added by the rule {rule}
                crate::i18n::t("{title}: added by {rule}"),
                &[("title", &title_i), ("rule", &crate::i18n::isolate(rule))],
            ),
        );
        self.save_rss();
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|i| i.torrent.as_ref().is_some_and(|t| t.hash == hash))
        {
            item.note(crate::i18n::fmt(
                // i18n: event log entry: an RSS download rule added this torrent; {rule} is the rule's name, {feed} the feed's
                crate::i18n::t("Added by the RSS rule {rule} from {feed}"),
                &[
                    ("rule", &crate::i18n::isolate(rule)),
                    ("feed", &crate::i18n::isolate(&feed)),
                ],
            ));
        }
    }
}
