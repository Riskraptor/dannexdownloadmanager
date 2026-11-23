"""Write i18n/formats/dates.tsv: how each interface language writes dates and times (Unicode
CLDR, through Babel), for crates/app/src/datefmt.rs.

    python tools/i18n/dates.py [--check]

One row per language of i18n/languages.tsv (the language as Windows tags it, `de-DE`, so a
country's conventions where CLDR has them: its first day of the week). The patterns are CLDR's
(UTS #35 letters: d, M, y, E, H, h, m, s, a; text in quotes), made fit for DDM:

  * short: the short date with a four-digit year (`dd.MM.y`), as Statistics and the Scheduler's
    notes write a day;
  * month_day: the numeric month and day (`d.M.`), the Statistics chart's axis;
  * long: the full date with the weekday (`EEEE, d. MMMM y`), the Scheduler's date box; an era
    (`G`) is written in as text, standalone names (`LLLL`, `cccc`) as the format ones;
  * time: the time with seconds (`HH:mm:ss`, `h:mm:ss a`); a flexible day period (`B`) is `a`;
  * week_start: the first day of the week, 0 = Sunday … 6 = Saturday;
  * comma: what joins the items of a short list (`, `; Arabic `، `; Chinese `、`);
  * months, weekdays: the names a full date uses (format context: Russian `ноября`), January and
    Sunday first, `|` between them.

Bidi marks in CLDR's patterns are taken out: DDM isolates dates itself (`i18n::ltr`). `--check`
fails when the file is not what this script writes (CI).
"""

import re
import sys
from pathlib import Path

from babel import Locale

ROOT = Path(__file__).resolve().parents[2]
LANGUAGES = ROOT / "i18n" / "languages.tsv"
OUT = ROOT / "i18n" / "formats" / "dates.tsv"

BIDI = dict.fromkeys(map(ord, "‎‏؜‪‫‬‭‮"))
ARABIC_SCRIPT = re.compile("[؀-ۿݐ-ݿࢠ-ࣿ]")


def languages():
    """(code, Windows tag, own name) of every interface language."""
    rows = []
    for line in LANGUAGES.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        rows.append((cols[0], cols[5].strip(), cols[2]))
    return rows


def locale(code, windows):
    """Babel's locale for the language: its Windows tag, else its code, else its language."""
    for tag in (windows, code, code.split("-")[0]):
        try:
            return Locale.parse(tag.replace("-", "_"))
        except (ValueError, LookupError):
            continue
    raise SystemExit(f"{code}: no CLDR locale")


def tokens(pattern):
    """A CLDR pattern as (kind, text): ("field", "MMMM") or ("text", "de")."""
    out, i = [], 0
    while i < len(pattern):
        c = pattern[i]
        if c == "'":
            if pattern[i + 1 : i + 2] == "'":
                out.append(("text", "'"))
                i += 2
                continue
            end = pattern.find("'", i + 1)
            end = len(pattern) if end < 0 else end
            out.append(("text", pattern[i + 1 : end].replace("''", "'") or "'"))
            i = end + 1
        elif c.isascii() and c.isalpha():
            j = i
            while j < len(pattern) and pattern[j] == c:
                j += 1
            out.append(("field", pattern[i:j]))
            i = j
        else:
            out.append(("text", c))
            i += 1
    return out


def written(toks):
    """Tokens back into a pattern, text quoted where it holds letters or a quote."""
    out = []
    for kind, text in toks:
        if kind == "field":
            out.append(text)
        elif text == "'":
            out.append("''")
        elif re.search(r"[A-Za-z]", text):
            out.append("'" + text.replace("'", "''") + "'")
        else:
            out.append(text)
    return "".join(out)


def tidy(pattern, loc):
    """`pattern` with what DDM writes: no bidi marks, an era as text, a four-digit year, the
    format names for standalone ones and `a` for a flexible day period."""
    era = loc.eras["abbreviated"][1]
    toks = []
    for kind, text in tokens(pattern.translate(BIDI)):
        if kind == "field":
            letter = text[0]
            if letter == "G":
                toks.append(("text", era))
                continue
            if letter == "y":
                text = "y"
            elif letter == "L":
                text = "M" * len(text)
            elif letter == "c":
                text = "E" * len(text)
            elif letter in "Bb":
                text = "a"
            elif letter == "k":
                text = "H" * len(text)
            elif letter == "K":
                text = "h" * len(text)
        toks.append((kind, text))
    return written(toks)


def comma(loc, own_name):
    """What joins a short list's items: CLDR's list middle, unless it is a word ("و"); Arabic
    script's comma for the languages written in it."""
    middle = loc.list_patterns["standard"].get("middle") or "{0}, {1}"
    sep = middle.replace("{0}", "").replace("{1}", "").translate(BIDI)
    if not sep or any(c.isalpha() for c in sep):
        sep = ", "
    if ARABIC_SCRIPT.search(own_name) and sep.strip() == ",":
        sep = "، "
    return sep


def row(code, windows, own_name):
    loc = locale(code, windows)
    short = tidy(loc.date_formats["short"].pattern, loc)
    md = loc.datetime_skeletons.get("Md")
    md = getattr(md, "pattern", md) or "M/d"
    month_day = tidy(md, loc)
    long = tidy(loc.date_formats["full"].pattern, loc)
    time = tidy(loc.time_formats["medium"].pattern, loc)
    # Babel counts the week from Monday (0).
    week_start = (loc.first_week_day + 1) % 7
    months = [loc.months["format"]["wide"][m] for m in range(1, 13)]
    # Babel's days are Monday (0) … Sunday (6).
    days = loc.days["format"]["wide"]
    weekdays = [days[(d + 6) % 7] for d in range(7)]
    return [
        code,
        short,
        month_day,
        long,
        time,
        str(week_start),
        comma(loc, own_name),
        "|".join(m.translate(BIDI) for m in months),
        "|".join(d.translate(BIDI) for d in weekdays),
    ]


HEADER = """\
# How each interface language writes dates and times: Unicode CLDR (Unicode License) through
# Babel, written by tools/i18n/dates.py (run it again rather than editing this file). Read by
# crates/app/src/datefmt.rs. Patterns in CLDR's letters (UTS #35); week_start 0 = Sunday;
# comma joins a short list's items; months and weekdays (January, Sunday first) in a date's form.
# code\tshort\tmonth_day\tlong\ttime\tweek_start\tcomma\tmonths\tweekdays
"""


def main():
    rows = [row(*lang) for lang in languages()]
    for r in rows:
        assert all("\t" not in c and "\n" not in c for c in r), r
    text = HEADER + "".join("\t".join(r) + "\n" for r in rows)
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text(encoding="utf-8") != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is out of date: python tools/i18n/dates.py")
        return
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"{OUT.relative_to(ROOT)}: {len(rows)} languages")


if __name__ == "__main__":
    main()
