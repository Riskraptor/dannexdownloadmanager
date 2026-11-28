//! How dates and times are written (Options → Regional), as numbers are (`i18n`): the interface
//! language's way (Unicode CLDR, `datefmt`), or the system region's own when it is in that same
//! language (its customised formats on Windows); English keeps IDM's: the system region's date
//! order and clock. Or the user's own choice of date order, 12- or 24-hour clock and first day
//! of the week. The download list, the Scheduler's date and time boxes, its calendar and
//! Statistics read it here.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::datefmt::{self, Words};

/// The order of a date's day, month and year.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateOrder {
    Dmy,
    #[default]
    Mdy,
    Ymd,
}

/// What Options → Regional holds (saved in the settings by these names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Regional {
    /// Write dates and times the user's way rather than the system's.
    pub custom: bool,
    pub order: DateOrder,
    pub clock24: bool,
    /// The first day of the week, 0 = Sunday … 6 = Saturday.
    pub week_start: u8,
}

impl Default for Regional {
    fn default() -> Self {
        let system = system();
        Regional {
            custom: false,
            ..system
        }
    }
}

/// The formats in use: the custom ones, or the system's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    /// The user's own choice (else the system's).
    pub custom: bool,
    pub order: DateOrder,
    pub clock24: bool,
    pub week_start: u8,
}

static CURRENT: RwLock<Option<Formats>> = RwLock::new(None);

/// Use `r` from now on (the settings loaded, or Options → OK).
pub fn set(r: &Regional) {
    *CURRENT.write().unwrap() = Some(effective(r));
}

/// The formats in use now (the system's until `set`).
pub fn current() -> Formats {
    CURRENT
        .read()
        .unwrap()
        .unwrap_or_else(|| effective(&Regional::default()))
}

pub fn effective(r: &Regional) -> Formats {
    let custom = r.custom;
    let r = if r.custom { *r } else { system() };
    Formats {
        custom,
        order: r.order,
        clock24: r.clock24,
        week_start: r.week_start.min(6),
    }
}

/// The system's region formats: Windows' settings; elsewhere the language's convention
/// (`LC_ALL`, `LC_TIME`, `LANG`): the United States and a few others month first and 12-hour,
/// East Asia and Hungary year first, the rest day first and 24-hour.
pub fn system() -> Regional {
    #[cfg(windows)]
    if let Some((order, clock24, week_start)) = crate::win32::region_formats() {
        return Regional {
            custom: false,
            order,
            clock24,
            week_start,
        };
    }
    let lang = ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let lang = lang.split('.').next().unwrap_or("").replace('-', "_");
    let us_like = ["en_us", "en_ph", "en_ca", "es_us"].contains(&lang.as_str());
    let year_first = ["ja", "zh", "ko", "hu", "lt", "mn"]
        .iter()
        .any(|p| lang.starts_with(p));
    Regional {
        custom: false,
        order: if us_like {
            DateOrder::Mdy
        } else if year_first {
            DateOrder::Ymd
        } else {
            DateOrder::Dmy
        },
        clock24: !(us_like || lang.starts_with("en_au") || lang.starts_with("en_in")),
        week_start: if us_like || lang.starts_with("ja") || lang.starts_with("he") {
            0
        } else {
            1
        },
    }
}

/// The user's country (ISO 3166-1 alpha-2, upper case), as the system's region says: Windows'
/// own setting; elsewhere the locale's territory (`LC_ALL`, `LANG`: `de_AT.UTF-8` is AT). A
/// Metalink's servers there come first among those of one priority (`metalinkdlg`). None when
/// the system doesn't say (the `C` locale).
pub fn country() -> Option<String> {
    #[cfg(windows)]
    if let Some(c) = crate::win32::user_country() {
        return Some(c);
    }
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))?;
    country_of_locale(&locale)
}

/// The territory of a POSIX locale name (`pt_BR.UTF-8@euro` is BR).
fn country_of_locale(locale: &str) -> Option<String> {
    let name = locale.split(['.', '@']).next()?;
    let (_, territory) = name.split_once(['_', '-'])?;
    (territory.len() == 2 && territory.chars().all(|c| c.is_ascii_alphabetic()))
        .then(|| territory.to_ascii_uppercase())
}

// The names in English, translated where they are shown (the Scheduler's weekdays and its
// calendar's title; a date's own names come from `datefmt` in other languages).
pub const WEEKDAYS: [&str; 7] = [
    crate::i18n::mark("Sunday"),
    crate::i18n::mark("Monday"),
    crate::i18n::mark("Tuesday"),
    crate::i18n::mark("Wednesday"),
    crate::i18n::mark("Thursday"),
    crate::i18n::mark("Friday"),
    crate::i18n::mark("Saturday"),
];
pub const MONTHS: [&str; 12] = [
    crate::i18n::mark("January"),
    crate::i18n::mark("February"),
    crate::i18n::mark("March"),
    crate::i18n::mark("April"),
    // i18n: the month (the fifth of the year)
    crate::i18n::mark("May"),
    crate::i18n::mark("June"),
    crate::i18n::mark("July"),
    crate::i18n::mark("August"),
    crate::i18n::mark("September"),
    crate::i18n::mark("October"),
    crate::i18n::mark("November"),
    crate::i18n::mark("December"),
];

/// How dates and times are written now: patterns in CLDR's letters ([`datefmt`]), the names and
/// digits they are written with, the week's first day.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// A day in numbers: `19.11.2026`.
    pub short: String,
    /// A month and its day in numbers (the Statistics chart's axis): `19.11.`.
    pub month_day: String,
    /// A day in words (the Scheduler's date box): `Donnerstag, 19. November 2026`.
    pub long: String,
    /// A time of day with seconds: `23:00:00`, `11:00:00 PM`.
    pub time: String,
    /// The first day of the week, 0 = Sunday … 6 = Saturday.
    pub week_start: u8,
    /// The language's own month and weekday names (CLDR); `None`: the catalog's (English).
    pub names: Option<&'static datefmt::Lang>,
    /// The language's digits where they aren't `0`–`9` (Persian `۱۹`), as its numbers have them.
    pub digits: Option<[char; 10]>,
}

impl Style {
    /// Is the clock a 24-hour one?
    pub fn clock24(&self) -> bool {
        datefmt::clock24(&self.time)
    }

    /// The month and weekday names, digits and AM/PM marks this style writes with.
    pub fn words(&self) -> OwnedWords {
        let (months, weekdays) = match self.names {
            Some(l) => (l.months, l.weekdays),
            None => (MONTHS.map(crate::i18n::t), WEEKDAYS.map(crate::i18n::t)),
        };
        OwnedWords {
            months,
            weekdays,
            am: am(),
            pm: pm(),
            digits: self.digits,
        }
    }
}

/// [`Words`] that own their arrays (the catalog's names are looked up when asked for).
#[derive(Clone, Debug)]
pub struct OwnedWords {
    pub months: [&'static str; 12],
    pub weekdays: [&'static str; 7],
    pub am: &'static str,
    pub pm: &'static str,
    pub digits: Option<[char; 10]>,
}

impl OwnedWords {
    pub fn get(&self) -> Words<'_> {
        Words {
            months: &self.months,
            weekdays: &self.weekdays,
            am: self.am,
            pm: self.pm,
            digits: self.digits.as_ref(),
        }
    }
}

/// Before noon, after a time of day on a 12-hour clock (translated).
pub fn am() -> &'static str {
    // i18n: before noon, after a time of day (12-hour clock). Its pair is "PM": write both the same way (this language's own short marks, or both kept as AM and PM)
    crate::i18n::t("AM")
}

/// After noon, after a time of day on a 12-hour clock (translated).
pub fn pm() -> &'static str {
    // i18n: after noon, after a time of day (12-hour clock). Its pair is "AM": write both the same way (this language's own short marks, or both kept as AM and PM)
    crate::i18n::t("PM")
}

/// The patterns of an order and a clock (Options → Regional's own choice, and English's, as IDM
/// writes it): `dd/MM/y`, `MM/dd/y` or `y-MM-dd`; `HH:mm:ss` or `h:mm:ss a`.
fn chosen(f: &Formats) -> [&'static str; 4] {
    let (short, month_day, long) = match f.order {
        DateOrder::Dmy => ("dd/MM/y", "d/M", "EEEE, d MMMM y"),
        DateOrder::Mdy => ("MM/dd/y", "M/d", "EEEE, MMMM d, y"),
        DateOrder::Ymd => ("y-MM-dd", "MM-dd", "y MMMM d, EEEE"),
    };
    let time = if f.clock24 { "HH:mm:ss" } else { "h:mm:ss a" };
    [short, month_day, long, time]
}

/// The style for the interface language `code` and the formats `f` (Options → Regional's).
pub fn resolve(code: &str, f: &Formats) -> Style {
    let english = code == "en";
    let names = if english { None } else { datefmt::lang(code) };
    let digits = if english { None } else { language_digits() };
    let own = |[short, month_day, long, time]: [&'static str; 4], week_start: u8| Style {
        short: short.to_owned(),
        month_day: month_day.to_owned(),
        long: long.to_owned(),
        time: time.to_owned(),
        week_start,
        names,
        digits,
    };
    let Some(lang) = names.filter(|_| !f.custom) else {
        // The user's way; English (IDM's text on the system's clock and date order); a language
        // DDM has no conventions for.
        return own(chosen(f), f.week_start);
    };
    if let Some([short, long, time]) = system_patterns(code) {
        // A picture Windows left empty is the language's.
        let pick = |own: &str, cldr: &'static str| {
            if own.trim().is_empty() {
                cldr.to_owned()
            } else {
                own.to_owned()
            }
        };
        return Style {
            short: pick(&short, lang.short),
            month_day: lang.month_day.to_owned(),
            long: pick(&long, lang.long),
            time: pick(&time, lang.time),
            week_start: f.week_start,
            names,
            digits,
        };
    }
    own(
        [lang.short, lang.month_day, lang.long, lang.time],
        lang.week_start,
    )
}

/// The style in use now: the interface language's, Options → Regional's.
pub fn style() -> Style {
    type Cached = Option<(&'static str, Formats, Style)>;
    static CACHE: RwLock<Cached> = RwLock::new(None);
    let (code, f) = (crate::i18n::code(), current());
    if let Some((c, cf, s)) = CACHE.read().unwrap().as_ref()
        && *c == code
        && *cf == f
    {
        return s.clone();
    }
    let s = resolve(code, &f);
    *CACHE.write().unwrap() = Some((code, f, s.clone()));
    s
}

/// The digits the interface language writes its numbers in, where they aren't `0`–`9`.
fn language_digits() -> Option<[char; 10]> {
    let f = crate::i18n::i18n().format();
    let mut d = ['0'; 10];
    for (k, c) in d.iter_mut().enumerate() {
        *c = f
            .number_ungrouped(k as f64, 0)
            .chars()
            .next()
            .unwrap_or('0');
    }
    (d != ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']).then_some(d)
}

/// The system region's own patterns (short date, long date, time) where it writes the
/// interface language `code` (#128, as numbers): a German region's customised `dd.MM.yyyy` in
/// German. Windows only (elsewhere CLDR's for the language are its own).
fn system_patterns(code: &str) -> Option<[String; 3]> {
    let language = |tag: &str| {
        tag.split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    if language(&craft_i18n::Region::system().time) != language(code) {
        return None;
    }
    #[cfg(windows)]
    {
        static PICTURES: std::sync::OnceLock<Option<[String; 3]>> = std::sync::OnceLock::new();
        PICTURES
            .get_or_init(|| crate::win32::region_pictures().map(|p| p.map(|s| from_windows(&s))))
            .clone()
    }
    #[cfg(not(windows))]
    None
}

/// A Windows date or time picture (`dddd, d. MMMM yyyy`, `h:mm:ss tt`) in CLDR's letters
/// (`EEEE, d. MMMM y`, `h:mm:ss a`): its era (`g`) left out. Only Windows has such pictures
/// (`system_patterns`); its tests run everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn from_windows(picture: &str) -> String {
    use datefmt::Part;
    let parts = datefmt::parse(picture).into_iter().filter_map(|p| match p {
        Part::Field('d', n) if n >= 3 => Some(Part::Field('E', n)),
        Part::Field('y', 2) => Some(Part::Field('y', 2)),
        Part::Field('y', _) => Some(Part::Field('y', 1)),
        Part::Field('t', _) => Some(Part::Field('a', 1)),
        Part::Field('g', _) => None,
        p => Some(p),
    });
    datefmt::pattern(parts)
}

/// `d` as a day in numbers: `19.11.2026`, `11/19/2026`.
pub fn short_date(d: chrono::NaiveDate) -> String {
    let s = style();
    datefmt::write_date(&s.short, d, &s.words().get())
}

/// `d`'s month and day in numbers: `19.11.`, `11/19`.
pub fn month_day(d: chrono::NaiveDate) -> String {
    let s = style();
    datefmt::write_date(&s.month_day, d, &s.words().get())
}

/// A time of day in the style in use: `21:41:00`, or `9:41:00 PM` (AM and PM translated).
pub fn time_text(h: u32, m: u32, s: u32) -> String {
    let style = style();
    time_in(&style.time, h, m, s, &style.words())
}

/// A time of day on the 12- or 24-hour clock as Options → Regional writes one (IDM's text in
/// English): `21:41:00`, `9:41:00 PM`.
pub fn clock_text(h: u32, m: u32, s: u32, clock24: bool) -> String {
    let time = if clock24 { "HH:mm:ss" } else { "h:mm:ss a" };
    time_in(time, h, m, s, &style().words())
}

fn time_in(pattern: &str, h: u32, m: u32, s: u32, w: &OwnedWords) -> String {
    let t = chrono::NaiveDate::default()
        .and_hms_opt(h.min(23), m.min(59), s.min(59))
        .unwrap_or_default();
    datefmt::write(pattern, t, &w.get())
}

/// `t` as a short date and time in the style in use: `19.11.2026 23:00:00`,
/// `10/30/2026 9:41:00 PM`, `2026-10-30 21:41:00`.
pub fn short_date_time(t: chrono::NaiveDateTime) -> String {
    date_time_in(&style(), t)
}

/// [`short_date_time`] in the style Options → Regional's `r` would give (its example).
pub fn sample(r: &Regional, t: chrono::NaiveDateTime) -> String {
    date_time_in(&resolve(crate::i18n::code(), &effective(r)), t)
}

fn date_time_in(s: &Style, t: chrono::NaiveDateTime) -> String {
    let w = s.words();
    format!(
        "{} {}",
        datefmt::write(&s.short, t, &w.get()),
        datefmt::write(&s.time, t, &w.get())
    )
}

/// What joins a short list's items in the interface language: `, `, Arabic `، `, Chinese `、`.
pub fn comma() -> &'static str {
    match crate::i18n::code() {
        "en" => ", ",
        code => datefmt::lang(code).map_or(", ", |l| l.comma),
    }
}

/// `items` as a short list in the interface language: `2 files, 48.40 MB`, `2 ملفان، 48.40`.
pub fn list(items: &[&str]) -> String {
    items.join(comma())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locales_territory_is_its_country() {
        assert_eq!(country_of_locale("de_AT.UTF-8").as_deref(), Some("AT"));
        assert_eq!(country_of_locale("pt_br@euro").as_deref(), Some("BR"));
        assert_eq!(country_of_locale("en-KE").as_deref(), Some("KE"));
        assert_eq!(country_of_locale("C"), None);
        assert_eq!(country_of_locale("C.UTF-8"), None);
        assert_eq!(country_of_locale("sr_Latn"), None);
    }

    #[test]
    fn custom_formats_win_over_the_system() {
        let r = Regional {
            custom: true,
            order: DateOrder::Ymd,
            clock24: true,
            week_start: 9,
        };
        let f = effective(&r);
        assert_eq!(f.order, DateOrder::Ymd);
        assert!(f.clock24);
        assert_eq!(f.week_start, 6, "clamped to a weekday");
        let system = system();
        let auto = effective(&Regional { custom: false, ..r });
        assert_eq!(auto.order, system.order, "not custom: the system's");
    }

    fn date_time(s: &Style, t: chrono::NaiveDateTime) -> String {
        date_time_in(s, t)
    }

    fn formats(custom: bool, order: DateOrder, clock24: bool) -> Formats {
        Formats {
            custom,
            order,
            clock24,
            week_start: 1,
        }
    }

    #[test]
    fn dates_and_times_are_written_in_each_order() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 10, 30)
            .unwrap()
            .and_hms_opt(21, 41, 5)
            .unwrap();
        let cases = [
            (DateOrder::Dmy, true, "30/10/2026 21:41:05"),
            (DateOrder::Ymd, true, "2026-10-30 21:41:05"),
            (DateOrder::Mdy, false, "10/30/2026 9:41:05 PM"),
        ];
        for (order, clock24, want) in cases {
            let s = resolve("en", &formats(true, order, clock24));
            assert_eq!(date_time(&s, t), want);
        }
        let en = resolve("en", &formats(true, DateOrder::Mdy, false));
        assert_eq!(time_in(&en.time, 0, 5, 0, &en.words()), "12:05:00 AM");
        assert_eq!(time_in(&en.time, 12, 0, 0, &en.words()), "12:00:00 PM");
    }

    /// English writes dates as before, IDM's way on the system's order and clock, whatever the
    /// order: `Thursday, November 19, 2026`, `11/19/2026`, the axis `11/19`.
    #[test]
    fn english_keeps_its_dates() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        for (order, short, axis, long) in [
            (
                DateOrder::Mdy,
                "11/19/2026",
                "11/19",
                "Thursday, November 19, 2026",
            ),
            (
                DateOrder::Dmy,
                "19/11/2026",
                "19/11",
                "Thursday, 19 November 2026",
            ),
            (
                DateOrder::Ymd,
                "2026-11-19",
                "11-19",
                "2026 November 19, Thursday",
            ),
        ] {
            let s = resolve("en", &formats(false, order, false));
            let w = s.words();
            assert_eq!(datefmt::write_date(&s.short, d, &w.get()), short);
            assert_eq!(datefmt::write_date(&s.month_day, d, &w.get()), axis);
            assert_eq!(datefmt::write_date(&s.long, d, &w.get()), long);
            assert_eq!(time_in(&s.time, 23, 0, 0, &w), "11:00:00 PM");
            assert!(s.names.is_none() && s.digits.is_none());
        }
    }

    /// Other languages write dates their own way (CLDR) unless the user chose one, whatever the
    /// system's order and clock (this machine's region isn't in German or Arabic).
    #[test]
    fn dates_follow_the_interface_language() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        let t = d.and_hms_opt(23, 0, 0).unwrap();
        if craft_i18n::Region::system().time.starts_with("de") {
            return;
        }
        let de = resolve("de", &formats(false, DateOrder::Mdy, false));
        let w = de.words();
        assert_eq!(datefmt::write_date(&de.short, d, &w.get()), "19.11.2026");
        assert_eq!(datefmt::write_date(&de.month_day, d, &w.get()), "19.11.");
        assert_eq!(
            datefmt::write_date(&de.long, d, &w.get()),
            "Donnerstag, 19. November 2026"
        );
        assert_eq!(date_time(&de, t), "19.11.2026 23:00:00");
        assert!(de.clock24());
        assert_eq!(de.week_start, 1);
        // The user's own way still wins: its order and clock, the language's names.
        let own = resolve("de", &formats(true, DateOrder::Mdy, false));
        assert_eq!(datefmt::write_date(&own.short, d, &w.get()), "11/19/2026");
        assert!(!own.clock24());
        let ar = resolve("ar", &formats(false, DateOrder::Mdy, false));
        assert_eq!(
            datefmt::write_date(&ar.long, d, &ar.words().get()),
            "الخميس، 19 نوفمبر 2026"
        );
    }

    #[test]
    fn windows_pictures_read_as_cldr_patterns() {
        assert_eq!(from_windows("dddd, d. MMMM yyyy"), "EEEE, d. MMMM y");
        assert_eq!(from_windows("dd.MM.yy"), "dd.MM.yy");
        assert_eq!(from_windows("h:mm:ss tt"), "h:mm:ss a");
        assert_eq!(from_windows("HH:mm:ss"), "HH:mm:ss");
        assert_eq!(
            from_windows("dddd, d' de 'MMMM' de 'yyyy"),
            "EEEE, d' de 'MMMM' de 'y"
        );
        assert_eq!(from_windows("gg yyyy/MM/dd"), " y/MM/dd");
    }
}
