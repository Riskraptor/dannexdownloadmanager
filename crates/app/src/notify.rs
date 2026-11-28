//! Notify when downloads complete (beyond IDM, whose Download complete window is its only
//! word): a small, quiet notification by the tray icon. Downloads that end close together (a
//! queue's, Link grabber's batch) make one notification, not one each: they are gathered until
//! none has ended for a moment, or for a few seconds at most.

use std::time::{Duration, Instant};

/// How long after the last download ended the notification goes, and at most after the first.
const QUIET: Duration = Duration::from_millis(1500);
const LONGEST: Duration = Duration::from_secs(6);

/// The longest list of names a notification gives, in characters (the rest counted).
const NAMES_MAX: usize = 180;

#[derive(Default)]
pub struct Notifier {
    names: Vec<String>,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Notifier {
    /// Download `name` completed at `now`.
    pub fn done(&mut self, name: &str, now: Instant) {
        self.names.push(name.to_owned());
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// Whether a notification waits to go (the caller looks again soon).
    pub fn waiting(&self) -> bool {
        !self.names.is_empty()
    }

    /// The notification due at `now` (its title and text), once no download has ended for a
    /// moment or the first waited long enough.
    pub fn due(&mut self, now: Instant) -> Option<(String, String)> {
        let (first, last) = (self.first?, self.last?);
        if now.duration_since(last) < QUIET && now.duration_since(first) < LONGEST {
            return None;
        }
        self.first = None;
        self.last = None;
        let names = std::mem::take(&mut self.names);
        Some(message(&names))
    }
}

/// The title and text for these downloads: one's name, or how many and their names, as many as
/// fit, the rest counted.
fn message(names: &[String]) -> (String, String) {
    if let [one] = names {
        // i18n: a notification's title when one download is complete (its text is the file's name)
        return (crate::i18n::t("Download complete").to_owned(), one.clone());
    }
    let title = crate::i18n::tn(
        names.len() as u64,
        // i18n: a notification's title when several downloads completed together (its text lists their names)
        "{n} download complete",
        "{n} downloads complete",
    );
    let mut body = String::new();
    for (k, name) in names.iter().enumerate() {
        let sep = if body.is_empty() {
            ""
        } else {
            crate::regional::comma()
        };
        if body.chars().count() + sep.chars().count() + name.chars().count() > NAMES_MAX {
            let more = crate::i18n::tn((names.len() - k) as u64, "+{n} more", "+{n} more");
            body.push_str(sep);
            body.push_str(&more);
            break;
        }
        body.push_str(sep);
        body.push_str(name);
    }
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_ending_together_make_one_notification() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut n = Notifier::default();
        assert_eq!(n.due(t0), None);
        n.done("a.zip", ms(0));
        assert_eq!(n.due(ms(1000)), None, "another may follow");
        let (title, body) = n.due(ms(1600)).unwrap();
        assert_eq!(
            (title.as_str(), body.as_str()),
            ("Download complete", "a.zip")
        );
        assert!(!n.waiting());
        // A batch: gathered, and gone after a few seconds even while more end.
        for k in 0..40 {
            n.done(
                &format!("file-{k:02}-with-a-long-name.bin"),
                ms(2000 + k * 200),
            );
        }
        assert_eq!(n.due(ms(7000)), None);
        let (title, body) = n.due(ms(8000)).unwrap();
        assert_eq!(title, "40 downloads complete");
        assert!(
            body.starts_with("file-00-with-a-long-name.bin, file-01"),
            "{body}"
        );
        assert!(
            body.ends_with("more") && body.chars().count() < NAMES_MAX + 20,
            "{body}"
        );
    }
}
