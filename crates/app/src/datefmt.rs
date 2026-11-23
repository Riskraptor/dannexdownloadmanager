//! Dates and times written from patterns in Unicode CLDR's letters (UTS #35: `dd.MM.y`,
//! `EEEE, d. MMMM y`, `h:mm:ss a`), and how each interface language writes them
//! (`i18n/formats/dates.tsv`, made from CLDR by `tools/i18n/dates.py`). `regional` chooses the
//! patterns in use; this module only reads and fills them.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};

/// How a language writes dates and times (a row of `i18n/formats/dates.tsv`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lang {
    /// A day in numbers: `dd.MM.y`.
    pub short: &'static str,
    /// A month and its day in numbers: `d.M.`.
    pub month_day: &'static str,
    /// A day in words, its weekday first or last: `EEEE, d. MMMM y`.
    pub long: &'static str,
    /// A time of day with seconds: `HH:mm:ss`.
    pub time: &'static str,
    /// The first day of the week, 0 = Sunday … 6 = Saturday.
    pub week_start: u8,
    /// What joins a short list's items: `, `, Arabic `، `, Chinese `、`.
    pub comma: &'static str,
    /// The months as a date writes them, January first (Russian `ноября`).
    pub months: [&'static str; 12],
    /// The weekdays as a date writes them, Sunday first.
    pub weekdays: [&'static str; 7],
}

static TABLE: &str = include_str!("../../../i18n/formats/dates.tsv");

fn table() -> &'static HashMap<&'static str, Lang> {
    static T: OnceLock<HashMap<&'static str, Lang>> = OnceLock::new();
    T.get_or_init(|| {
        TABLE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                // Fields are never trimmed: a comma is ", " and Thai's is a space.
                let c: Vec<&'static str> = l.split('\t').collect();
                let [
                    code,
                    short,
                    month_day,
                    long,
                    time,
                    week,
                    comma,
                    months,
                    weekdays,
                ] = c[..]
                else {
                    return None;
                };
                Some((
                    code,
                    Lang {
                        short,
                        month_day,
                        long,
                        time,
                        week_start: week.parse::<u8>().ok()?.min(6),
                        comma,
                        months: months.split('|').collect::<Vec<_>>().try_into().ok()?,
                        weekdays: weekdays.split('|').collect::<Vec<_>>().try_into().ok()?,
                    },
                ))
            })
            .collect()
    })
}

/// How the language `code` (a catalog's, `de`, `pt-br`) writes dates; its language alone when
/// the table has no row for the code.
pub fn lang(code: &str) -> Option<&'static Lang> {
    let t = table();
    let code = code.to_ascii_lowercase();
    t.get(code.as_str())
        .or_else(|| t.get(code.split(['-', '_']).next().unwrap_or_default()))
}

/// One part of a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// A field: its letter and how many times it is written (`MMMM` is `('M', 4)`).
    Field(char, usize),
    /// Text written as it is (quoted text and punctuation, run together).
    Text(String),
}

/// `pattern` in parts: letters `a`–`z`, `A`–`Z` are fields, text in single quotes is written as
/// it is (`''` is a quote), and so is anything else.
pub fn parse(pattern: &str) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    let push_text = |out: &mut Vec<Part>, s: &str| match out.last_mut() {
        Some(Part::Text(t)) => t.push_str(s),
        _ => out.push(Part::Text(s.to_owned())),
    };
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                push_text(&mut out, "'");
                i += 2;
                continue;
            }
            let mut quoted = String::new();
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        quoted.push('\'');
                        i += 2;
                        continue;
                    }
                    break;
                }
                quoted.push(chars[i]);
                i += 1;
            }
            i += 1;
            push_text(&mut out, &quoted);
        } else if c.is_ascii_alphabetic() {
            let n = chars[i..].iter().take_while(|&&x| x == c).count();
            out.push(Part::Field(c, n));
            i += n;
        } else {
            push_text(&mut out, c.encode_utf8(&mut [0; 4]));
            i += 1;
        }
    }
    out
}

/// `parts` as a pattern again, text quoted where it holds a letter or a quote.
pub fn pattern(parts: impl IntoIterator<Item = Part>) -> String {
    let mut out = String::new();
    for p in parts {
        match p {
            Part::Field(c, n) => out.extend(std::iter::repeat_n(c, n)),
            Part::Text(s) if s.chars().any(|c| c.is_ascii_alphabetic() || c == '\'') => {
                out.push('\'');
                out.push_str(&s.replace('\'', "''"));
                out.push('\'');
            }
            Part::Text(s) => out.push_str(&s),
        }
    }
    out
}

/// What a pattern's names and digits are written with.
#[derive(Clone, Copy, Debug)]
pub struct Words<'a> {
    pub months: &'a [&'a str; 12],
    pub weekdays: &'a [&'a str; 7],
    pub am: &'a str,
    pub pm: &'a str,
    /// The digits `0`–`9` as the language writes them (`None`: `0`–`9`).
    pub digits: Option<&'a [char; 10]>,
}

/// `n` in `width` digits at least (zeros before it), in `digits`.
pub fn number(n: i64, width: usize, digits: Option<&[char; 10]>) -> String {
    let s = format!("{:0width$}", n.unsigned_abs());
    let s = if n < 0 { format!("-{s}") } else { s };
    match digits {
        Some(d) => s
            .chars()
            .map(|c| c.to_digit(10).map_or(c, |v| d[v as usize]))
            .collect(),
        None => s,
    }
}

/// The text of field `letter` written `n` times for `t` (empty for letters DDM doesn't write:
/// a week number, a time zone).
pub fn field(letter: char, n: usize, t: NaiveDateTime, w: &Words) -> String {
    let num = |v: u32, width: usize| number(i64::from(v), width, w.digits);
    let pad = |n: usize| if n >= 2 { 2 } else { 1 };
    match letter {
        'd' => num(t.day(), pad(n)),
        'M' | 'L' if n >= 3 => w.months[t.month0() as usize].to_owned(),
        'M' | 'L' => num(t.month(), pad(n)),
        'y' | 'Y' | 'u' if n == 2 => num(t.year().rem_euclid(100) as u32, 2),
        'y' | 'Y' | 'u' => number(i64::from(t.year()), n, w.digits),
        'E' | 'c' | 'e' => w.weekdays[t.weekday().num_days_from_sunday() as usize].to_owned(),
        'H' => num(t.hour(), pad(n)),
        'k' => num(if t.hour() == 0 { 24 } else { t.hour() }, pad(n)),
        'h' => num(
            if t.hour().is_multiple_of(12) {
                12
            } else {
                t.hour() % 12
            },
            pad(n),
        ),
        'K' => num(t.hour() % 12, pad(n)),
        'm' => num(t.minute(), pad(n)),
        's' => num(t.second(), pad(n)),
        'a' | 'b' | 'B' => (if t.hour() < 12 { w.am } else { w.pm }).to_owned(),
        _ => String::new(),
    }
}

/// `t` written as `pattern` says.
pub fn write(pattern: &str, t: NaiveDateTime, w: &Words) -> String {
    parse(pattern)
        .into_iter()
        .map(|p| match p {
            Part::Field(c, n) => field(c, n, t, w),
            Part::Text(s) => s,
        })
        .collect()
}

/// [`write`] for a day (its time is midnight).
pub fn write_date(pattern: &str, d: NaiveDate, w: &Words) -> String {
    write(pattern, d.and_hms_opt(0, 0, 0).unwrap_or_default(), w)
}

/// Is `pattern` a 24-hour clock (`H`, `k`) rather than a 12-hour one (`h`, `K`)?
pub fn clock24(pattern: &str) -> bool {
    parse(pattern)
        .iter()
        .any(|p| matches!(p, Part::Field('H' | 'k', _)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN_MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const EN_DAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];

    fn at(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 19)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    fn words(l: &Lang) -> Words<'_> {
        Words {
            months: &l.months,
            weekdays: &l.weekdays,
            am: "AM",
            pm: "PM",
            digits: None,
        }
    }

    #[test]
    fn patterns_are_read_as_cldr_writes_them() {
        assert_eq!(
            parse("EEEE, d 'de' MMMM 'de' y"),
            [
                Part::Field('E', 4),
                Part::Text(", ".into()),
                Part::Field('d', 1),
                Part::Text(" de ".into()),
                Part::Field('M', 4),
                Part::Text(" de ".into()),
                Part::Field('y', 1),
            ]
        );
        // A doubled quote is a quote, inside quoted text too; letters outside A–Z are text.
        assert_eq!(
            parse("d 'ta''' MMMM"),
            [
                Part::Field('d', 1),
                Part::Text(" ta' ".into()),
                Part::Field('M', 4)
            ]
        );
        assert_eq!(
            parse("y年M月d日"),
            [
                Part::Field('y', 1),
                Part::Text("年".into()),
                Part::Field('M', 1),
                Part::Text("月".into()),
                Part::Field('d', 1),
                Part::Text("日".into()),
            ]
        );
    }

    #[test]
    fn every_language_has_a_row_that_reads() {
        let languages = include_str!("../../../i18n/languages.tsv");
        for line in languages
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            let code = line.split('\t').next().unwrap();
            let l = lang(code).unwrap_or_else(|| panic!("{code}: no row in dates.tsv"));
            let w = words(l);
            for p in [l.short, l.month_day, l.long, l.time] {
                let s = write(p, at(23, 5, 9), &w);
                assert!(!s.trim().is_empty(), "{code}: {p}");
                assert!(!s.contains('\''), "{code}: {p} gives {s}");
            }
            // A day in words has its weekday and year (its month by name or number).
            let long = write(l.long, at(0, 0, 0), &w);
            assert!(long.contains(l.weekdays[4]), "{code}: {long}");
            assert!(long.contains("2026"), "{code}: {long}");
            let named = parse(l.long).contains(&Part::Field('M', 4));
            assert!(!named || long.contains(l.months[10]), "{code}: {long}");
            // Each short form has the day, the month and (but the axis) the year in numbers.
            assert!(write(l.short, at(0, 0, 0), &w).contains("19"), "{code}");
            assert!(write(l.month_day, at(0, 0, 0), &w).contains("11"), "{code}");
        }
    }

    /// The interface languages' own ways, from CLDR: German writes the day first with dots and a
    /// 24-hour clock, Arabic its own comma, Japanese the year first.
    #[test]
    fn languages_write_dates_their_way() {
        let de = lang("de").unwrap();
        let w = words(de);
        assert_eq!(write(de.short, at(23, 0, 0), &w), "19.11.2026");
        assert_eq!(write(de.month_day, at(0, 0, 0), &w), "19.11.");
        assert_eq!(
            write(de.long, at(0, 0, 0), &w),
            "Donnerstag, 19. November 2026"
        );
        assert_eq!(write(de.time, at(23, 0, 0), &w), "23:00:00");
        assert_eq!(write(de.time, at(7, 30, 0), &w), "07:30:00");
        assert!(clock24(de.time));
        assert_eq!(de.week_start, 1);
        assert_eq!(de.comma, ", ");

        let ar = lang("ar").unwrap();
        assert_eq!(
            write(ar.long, at(0, 0, 0), &words(ar)),
            "الخميس، 19 نوفمبر 2026"
        );
        assert_eq!(ar.comma, "، ");
        assert!(!clock24(ar.time));

        let ja = lang("ja").unwrap();
        assert_eq!(write(ja.short, at(0, 0, 0), &words(ja)), "2026/11/19");
        assert_eq!(ja.comma, "、");
        // A country's form of a language, and a code the table has only as a language.
        assert_eq!(lang("pt-br").unwrap().short, "dd/MM/y");
        assert_eq!(lang("DE").unwrap().short, "dd.MM.y");
        assert_eq!(lang("de-AT"), lang("de"));
        // Russian writes the month's genitive in a date.
        let ru = lang("ru").unwrap();
        assert!(write(ru.long, at(0, 0, 0), &words(ru)).contains("ноября"));
    }

    #[test]
    fn fields_are_written_with_the_languages_digits() {
        let persian = [
            '\u{6f0}', '\u{6f1}', '\u{6f2}', '\u{6f3}', '\u{6f4}', '\u{6f5}', '\u{6f6}', '\u{6f7}',
            '\u{6f8}', '\u{6f9}',
        ];
        let w = Words {
            months: &EN_MONTHS,
            weekdays: &EN_DAYS,
            am: "AM",
            pm: "PM",
            digits: Some(&persian),
        };
        assert_eq!(write("y/M/d", at(0, 0, 0), &w), "۲۰۲۶/۱۱/۱۹");
        let w = Words { digits: None, ..w };
        assert_eq!(
            write("EEEE, MMMM d, y", at(0, 0, 0), &w),
            "Thursday, November 19, 2026"
        );
        assert_eq!(write("h:mm:ss a", at(0, 5, 0), &w), "12:05:00 AM");
        assert_eq!(write("h:mm:ss a", at(12, 0, 0), &w), "12:00:00 PM");
        assert_eq!(write("hh:mm a", at(21, 41, 0), &w), "09:41 PM");
        assert_eq!(write("yy-MM-dd", at(0, 0, 0), &w), "26-11-19");
    }
}
