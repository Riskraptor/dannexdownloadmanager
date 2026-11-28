//! robots.txt (RFC 9309): which paths a site asks robots not to visit, and how long it asks them
//! to wait between requests (`Crawl-delay`, not in the RFC but widely written).

use std::time::Duration;

/// The name DDM's grabber goes by in robots.txt groups (`User-agent: DannexDownloadManager`).
pub const PRODUCT: &str = "DannexDownloadManager";

/// The most of a robots.txt read (RFC 9309 asks for at least 500 KiB).
pub const MAX_SIZE: usize = 512 * 1024;

/// One `User-agent` group: its agents and rules.
#[derive(Clone, Debug, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<(bool, String)>,
    delay: Option<f64>,
}

/// A parsed robots.txt.
#[derive(Clone, Debug, Default)]
pub struct Robots {
    groups: Vec<Group>,
}

/// The rules that apply to one robot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rules {
    /// `(allow, pattern)`.
    rules: Vec<(bool, String)>,
    /// Asked to wait this long between requests.
    pub delay: Option<Duration>,
    /// Everything disallowed (the file couldn't be read for a server's error: RFC 9309 2.3.1.4).
    pub closed: bool,
}

impl Rules {
    /// No robots.txt (404, or not one at all): everything allowed.
    pub fn open() -> Rules {
        Rules::default()
    }

    /// The server failed answering for it: nothing allowed, as the RFC says.
    pub fn closed() -> Rules {
        Rules {
            closed: true,
            ..Rules::default()
        }
    }

    /// Whether `path` (the path and query, starting with `/`) may be visited: the longest
    /// matching rule decides, `Allow` winning a tie; no rule allows it. `/robots.txt` itself
    /// always may.
    pub fn allows(&self, path: &str) -> bool {
        if path == "/robots.txt" {
            return true;
        }
        if self.closed {
            return false;
        }
        let mut best: Option<(usize, bool)> = None;
        for (allow, pattern) in &self.rules {
            if pattern.is_empty() || !matches(pattern, path) {
                continue;
            }
            let len = pattern.len();
            best = match best {
                Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                _ => Some((len, *allow)),
            };
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

impl Robots {
    pub fn parse(text: &str) -> Robots {
        let mut groups: Vec<Group> = Vec::new();
        // A group's `User-agent` lines come first; a rule after them ends the run.
        let mut collecting_agents = false;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    if !collecting_agents {
                        groups.push(Group::default());
                        collecting_agents = true;
                    }
                    if let Some(g) = groups.last_mut() {
                        g.agents.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    collecting_agents = false;
                    if let Some(g) = groups.last_mut() {
                        g.rules.push((key == "allow", normalize(value)));
                    }
                }
                "crawl-delay" => {
                    collecting_agents = false;
                    if let Some(g) = groups.last_mut() {
                        g.delay = value
                            .parse::<f64>()
                            .ok()
                            .filter(|d| d.is_finite() && *d >= 0.0);
                    }
                }
                _ => {}
            }
        }
        Robots { groups }
    }

    /// The rules for robot `product` (the groups naming it, else the `*` groups, merged).
    pub fn rules_for(&self, product: &str) -> Rules {
        let product = product.to_ascii_lowercase();
        let named: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| {
                g.agents
                    .iter()
                    // A blank `User-agent:` names no robot (every name starts with "").
                    .any(|a| !a.is_empty() && *a != "*" && product.starts_with(a.as_str()))
            })
            .collect();
        let chosen = if named.is_empty() {
            self.groups
                .iter()
                .filter(|g| g.agents.iter().any(|a| a == "*"))
                .collect()
        } else {
            named
        };
        let mut rules = Rules::default();
        for g in chosen {
            rules.rules.extend(g.rules.iter().cloned());
            if let Some(d) = g.delay {
                rules.delay = Some(Duration::from_secs_f64(d.min(3600.0)));
            }
        }
        rules
    }
}

/// A rule's path as paths are compared: percent-encoding of the characters that must be
/// encoded, so `/a b` matches `/a%20b`.
fn normalize(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for c in pattern.chars() {
        if c.is_ascii_graphic() {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// `path` matched by a rule's `pattern` from its start: `*` any run of characters, a final `$`
/// the end of the path.
fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            at = part.len();
            continue;
        }
        let last = i == parts.len() - 1;
        if last && anchored {
            return path.len() >= at + part.len() && path.ends_with(part);
        }
        match path[at..].find(part) {
            Some(j) => at += j + part.len(),
            None => return false,
        }
    }
    !anchored || at == path.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# a comment
User-agent: *
Disallow: /private/
Allow: /private/public.html
Disallow: /*.pdf$
Crawl-delay: 2

User-agent: OtherBot
User-agent: dannexdownloadmanager
Disallow: /only-us/
Disallow:

User-agent: BadBot
Disallow: /
";

    #[test]
    fn the_longest_rule_wins_and_allow_wins_a_tie() {
        let rules = Robots::parse(FILE).rules_for("SomeBot");
        assert!(rules.allows("/"));
        assert!(!rules.allows("/private/x.html"));
        assert!(rules.allows("/private/public.html"));
        assert!(!rules.allows("/docs/a.pdf"));
        assert!(rules.allows("/docs/a.pdf?download=1"));
        assert!(rules.allows("/robots.txt"));
        assert_eq!(rules.delay, Some(Duration::from_secs(2)));
        let tie = Rules {
            rules: vec![(false, "/a".into()), (true, "/a".into())],
            ..Rules::default()
        };
        assert!(tie.allows("/a/b"));
    }

    #[test]
    fn a_group_naming_ddm_replaces_the_star_group() {
        let rules = Robots::parse(FILE).rules_for(PRODUCT);
        assert!(rules.allows("/private/x.html"), "the * group doesn't apply");
        assert!(!rules.allows("/only-us/a"));
        assert_eq!(rules.delay, None);
    }

    #[test]
    fn wildcards_and_anchors() {
        assert!(matches("/a*/c", "/abc/c/d"));
        assert!(matches("/*.php$", "/x/y.php"));
        assert!(!matches("/*.php$", "/x/y.php?q"));
        assert!(matches("/x$", "/x"));
        assert!(!matches("/x$", "/xy"));
        assert!(matches("/", "/anything"));
        assert!(!matches("/b", "/a"));
        assert_eq!(normalize("/a b/é"), "/a%20b/%C3%A9");
    }

    #[test]
    fn a_blank_user_agent_names_no_robot() {
        let file = "User-agent:\nDisallow:\n\nUser-agent: *\nDisallow: /\n";
        let rules = Robots::parse(file).rules_for(PRODUCT);
        assert!(!rules.allows("/x"), "the * group applies");
    }

    #[test]
    fn no_file_allows_everything_a_failing_server_nothing() {
        assert!(Rules::open().allows("/x"));
        assert!(Robots::parse("").rules_for(PRODUCT).allows("/x"));
        assert!(!Rules::closed().allows("/x"));
        assert!(Rules::closed().allows("/robots.txt"));
    }
}
