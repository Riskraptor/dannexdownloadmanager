//! The finished file checked (its size, its hash, its pieces), the bad pieces downloaded again from
//! other sources (`verify_and_repair`), and the file put in its place.

use super::*;

/// What fetching bad pieces again needs (`verify_and_repair`): the download's last round of
/// connections, its resume state as last written, and the download's own token.
pub(super) struct Repair<'a> {
    pub(super) ctx: &'a Arc<Ctx>,
    pub(super) saved: &'a ResumeState,
    pub(super) cancel: &'a CancellationToken,
}

/// Rounds of fetching pieces that failed their check again before the download fails.
pub(super) const PIECE_ROUNDS: usize = 3;

/// Check the finished part file against `hash` (at most two files at once: it reads them
/// whole). No hash, nothing to do.
pub(super) async fn verify_part(
    shared: &Shared,
    part: &Path,
    hash: Option<&crate::verify::Hash>,
) -> Result<(), Error> {
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let Some(hash) = hash.cloned() else {
        return Ok(());
    };
    shared.set_state(State::Verifying);
    let _slot = SLOTS.acquire().await.map_err(|_| Error::Cancelled)?;
    let part = part.to_owned();
    let algo = hash.algo;
    let ok = tokio::task::spawn_blocking(move || crate::verify::matches(&part, &hash))
        .await
        .expect("verifier panicked")?;
    if ok {
        shared.event(format!("Checksum verified ({})", algo.name()));
        Ok(())
    } else {
        Err(Error::ChecksumMismatch(algo.name()))
    }
}

/// Check the finished part file (`size` bytes): against the whole-file `checksum` first, and
/// when that fails or there is none, piece by piece against `pieces`. Bad pieces are fetched
/// again (`repair`: a round of connections over them alone, from other sources than those that
/// sent them, `Ctx::blame`), up to `PIECE_ROUNDS` times. Without `repair` (one connection, no
/// ranges) bad pieces can only fail the download. A file whose pieces are all good but whose
/// whole-file hash isn't fails: no piece can be told then.
pub(super) async fn verify_and_repair(
    shared: &Arc<Shared>,
    part: &Path,
    size: u64,
    checksum: Option<&crate::verify::Hash>,
    pieces: Option<&Arc<crate::verify::Pieces>>,
    repair: Option<Repair<'_>>,
) -> Result<(), Error> {
    let pieces = pieces.filter(|p| {
        let fits = p.fits(size);
        if !fits {
            shared.event("The piece hashes don't fit the file's size; not used".to_owned());
        }
        fits
    });
    let Some(pieces) = pieces else {
        return verify_part(shared, part, checksum).await;
    };
    let mut ctx = repair.as_ref().map(|r| r.ctx.clone());
    for round in 1..=PIECE_ROUNDS + 1 {
        if checksum.is_some() {
            match verify_part(shared, part, checksum).await {
                Ok(()) => return Ok(()),
                Err(Error::ChecksumMismatch(_)) => {}
                Err(e) => return Err(e),
            }
        }
        shared.set_state(State::Verifying);
        let bad = {
            let (part, p) = (part.to_owned(), pieces.clone());
            tokio::task::spawn_blocking(move || crate::verify::bad_pieces(&part, &p, size))
                .await
                .expect("verifier panicked")?
        };
        let algo = pieces.algo.name();
        if bad.is_empty() {
            if let Some(h) = checksum {
                return Err(Error::ChecksumMismatch(h.algo.name()));
            }
            shared.event(format!("All {} pieces verified ({algo})", pieces.count()));
            return Ok(());
        }
        shared.event(format!(
            "{} of {} pieces failed their {algo} check",
            bad.len(),
            pieces.count()
        ));
        let (Some(r), Some(last)) = (repair.as_ref(), ctx.as_ref()) else {
            return Err(Error::ChecksumMismatch(algo));
        };
        if round > PIECE_ROUNDS {
            return Err(Error::ChecksumMismatch(algo));
        }
        last.blame(&bad, pieces);
        // The bad pieces' bytes are no longer downloaded: the planner hands them out again.
        let segments = reopened(&bad, pieces, size);
        let mut planner = Planner::resume(size, last.opts.min_split, &segments)
            .map_err(|_| Error::BadState("bad pieces don't tile the file".into()))?;
        planner.set_preempt_after(PREEMPT_MIN);
        *shared.planner.lock().unwrap() = Some(planner);
        {
            let downloaded = shared.with_planner(|p| p.downloaded());
            let mut v = shared.view.lock().unwrap();
            v.state = State::Downloading;
            v.downloaded = downloaded;
        }
        shared.event(format!("Downloading {} pieces again", bad.len()));
        let next = Arc::new(last.again(r.cancel.child_token()));
        segmented(next.clone(), r.saved.clone(), None).await?;
        ctx = Some(next);
    }
    Err(Error::ChecksumMismatch(pieces.algo.name()))
}

/// A finished file's segments with pieces `bad` (in order) not downloaded: the planner's state
/// for fetching them again, tiling `[0, size)`.
pub(super) fn reopened(
    bad: &[usize],
    pieces: &crate::verify::Pieces,
    size: u64,
) -> Vec<crate::planner::SavedSegment> {
    use crate::planner::SavedSegment;
    let mut segments = Vec::new();
    let mut at = 0;
    for &i in bad {
        let (from, to) = pieces.range(i, size);
        if from > at {
            segments.push(SavedSegment {
                start: at,
                cursor: from,
                end: from,
            });
        }
        let start = from.max(at);
        if to > start {
            segments.push(SavedSegment {
                start,
                cursor: start,
                end: to,
            });
        }
        at = at.max(to);
    }
    if at < size {
        segments.push(SavedSegment {
            start: at,
            cursor: size,
            end: size,
        });
    }
    segments
}

/// Flush and rename the part file (`part`) to its final name; copied there when it is on
/// another drive (Options → Save to → Temporary directory).
pub(super) fn finish(
    shared: &Shared,
    file: Arc<File>,
    part: &Path,
    dest: &Path,
    size: u64,
    stamp: Option<std::time::SystemTime>,
) -> Result<Outcome, Error> {
    shared.set_state(State::Completing);
    let file = Arc::try_unwrap(file).map_err(|_| io::Error::other("part file still in use"))?;
    file.set_len(size)?;
    // Every byte is written: a normal file again (nothing left to allocate).
    if size >= sparse::MIN_SIZE {
        sparse::set(&file, false);
    }
    file.sync_all()?;
    // The server's date (Options::server_time), set before the rename moves the file.
    if let Some(t) = stamp {
        let times = std::fs::FileTimes::new().set_modified(t).set_accessed(t);
        #[cfg(windows)]
        let times = {
            use std::os::windows::fs::FileTimesExt;
            times.set_created(t)
        };
        let _ = file.set_times(times);
    }
    drop(file);
    // Never replace a file that appeared at the destination meanwhile (rename replaces on
    // Windows): take the next free numbered name instead.
    let original = dest;
    let dest = if dest.exists() {
        let dir = dest.parent().unwrap_or(Path::new("."));
        let name = dest
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download");
        naming::unique_path(dir, name)
    } else {
        dest.to_owned()
    };
    // Antivirus scanners and indexers briefly lock new files on Windows: retry the rename.
    let renamed = craft_atomic::retry_rename(craft_atomic::RenameRetry::platform(), || {
        std::fs::rename(part, &dest)
    });
    match renamed {
        Ok(()) => {}
        // From the temporary directory on another drive: copied over (a rename can't cross
        // drives), the part removed after.
        Err(_) if part.parent() != dest.parent() => {
            shared.event("Copying the file from the temporary directory");
            naming::move_part(part, &dest)?;
        }
        Err(e) => return Err(e.into()),
    }
    // Only now is the resume state obsolete.
    let _ = std::fs::remove_file(state_path(original));
    Ok(Outcome { path: dest, size })
}
