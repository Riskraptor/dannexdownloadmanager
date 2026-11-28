"""The installer in every interface language (D-019): writes tools/installer/languages.iss, which
mdm.iss includes, from i18n/languages.tsv.

    python tools/installer/languages.py            # translate what is missing, write the files
    python tools/installer/languages.py --dry-run  # only count
    python tools/installer/languages.py --export DIR   # no key: what to translate, per language
    python tools/installer/languages.py --import DIR   # read DIR/<code>.out.json, check, write

- A language with an official Inno Setup translation (languages.tsv `inno` column) uses it.
- Any other gets tools/installer/languages/<code>.isl: Inno's Default.isl messages translated by
  the same model, rules and checks as the app's catalogs (tools/i18n/translate.py), its
  LanguageID from Windows' tag, RightToLeft for right-to-left scripts.
- mdm.iss's own messages ([CustomMessages] `lang_en.*`) are translated for every language. Each
  translation is written under a `;en Key=...` line giving the English it was made from, so a
  message whose English changes is translated again (`--redo Key,...` forces it for files from
  before those lines).
Rows already translated are kept; a re-run only sends what is new. Inno's `%1`, `%n` and `[name]`
go to the model as `{1}`, a line break and `{name}`, and come back.

Without an OpenRouter key, `--export DIR` writes DIR/<code>.json per language (the model's rules
and the texts, each with its id), a translator writes DIR/<code>.out.json (id -> translation),
and `--import DIR` checks every row as a model's answer is checked and writes the files.
"""

import argparse
import ctypes
import json
import re
import sys
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "i18n"))
import translate as T  # noqa: E402

HERE = Path(__file__).resolve().parent
OUT = HERE / "languages"
ISS = HERE / "languages.iss"
RTL = {"ar", "fa", "he", "ug", "ur"}
INSTALLER_NOTE = ("\nThese texts are the program's Setup wizard and uninstaller (Inno Setup), not the "
                  "program itself. Inno Setup adds a final period to some messages itself: never add "
                  "one the English doesn't have.\n")


def inno_dir():
    for d in (Path.home() / "AppData/Local/Programs/Inno Setup 6", Path("C:/Program Files (x86)/Inno Setup 6"),
              Path("C:/Program Files/Inno Setup 6")):
        if (d / "Default.isl").exists():
            return d
    sys.exit("Inno Setup 6 not found")


def to_model(s):
    s = s.replace("%n", "\n").replace("{#AppName}", "{AppName}")
    s = re.sub(r"%(\d)", r"{\1}", s)
    return re.sub(r"\[(name|name/ver|kb|mb|gb)\]", lambda m: "{" + m.group(1).replace("/", "_") + "}", s)


def from_model(s):
    s = re.sub(r"\{(name|name_ver|kb|mb|gb)\}", lambda m: "[" + m.group(1).replace("_", "/") + "]", s)
    # .isl files aren't preprocessed: the name goes in as text.
    s = re.sub(r"\{(\d)\}", r"%\1", s).replace("{AppName}", "Dannex Download Manager")
    return s.replace("\r", "").replace("\n", "%n")


def read_messages(path, section="Messages"):
    """key -> value of one section of an .isl/.iss file."""
    out, inside = {}, False
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        s = line.strip()
        if s.startswith("["):
            inside = s.lower() == f"[{section.lower()}]"
            continue
        if inside and s and not s.startswith(";") and "=" in s:
            k, v = s.split("=", 1)
            out[k.strip()] = v
    return out


# The English a custom message's translation was made from: a comment line above it.
SOURCE_MARK = ";en "


def read_sources(path):
    """key -> the English each [CustomMessages] translation of an .isl file was made from."""
    out, inside = {}, False
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        s = line.strip()
        if s.startswith("["):
            inside = s.lower() == "[custommessages]"
            continue
        if inside and s.startswith(SOURCE_MARK) and "=" in s:
            k, v = s[len(SOURCE_MARK):].split("=", 1)
            out[k.strip()] = v
    return out


def text_id(key):
    """A message's id for a translator: its kind and name (`c:TaskBrowser`, `m:WelcomeLabel1`)."""
    return f"{key[0]}:{key[1]}"


def lcid(tag):
    n = ctypes.windll.kernel32.LocaleNameToLCID(tag, 0)
    if not n:
        sys.exit(f"Windows doesn't know the locale {tag}")
    return n


def inno_name(code):
    """Inno's language name: letters, digits and `_`, and not `not`, `and` or `or` (Odia's code),
    so every name gets a prefix. win32::take_install_language reads it back."""
    return "lang_" + code.replace("-", "_")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default=T.MODEL)
    ap.add_argument("--budget", type=float, default=5.0)
    ap.add_argument("--workers", type=int, default=96)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--langs", help="only these codes (comma-separated); the others keep their files")
    ap.add_argument("--redo", help="translate these custom messages again (comma-separated keys)")
    ap.add_argument("--export", metavar="DIR",
                    help="no request: write DIR/<code>.json per language, the rules and the texts to "
                         "translate, for a translator working outside OpenRouter")
    ap.add_argument("--import", dest="import_dir", metavar="DIR",
                    help="no request: read DIR/<code>.out.json (each text's id -> its translation), "
                         "check every row as a model's answer is checked, and write the files")
    args = ap.parse_args()
    only = set(args.langs.split(",")) if args.langs else None
    redo = set(args.redo.split(",")) if args.redo else set()

    inno = inno_dir()
    default = read_messages(inno / "Default.isl")
    english = inno_name("en") + "."
    custom = {k[len(english):]: v for k, v in read_messages(HERE / "mdm.iss", "CustomMessages").items()
              if k.startswith(english)}
    langs = T.languages()
    OUT.mkdir(exist_ok=True)

    # Per language: (kind, key) -> English, and what its files already have.
    state, jobs = {}, []
    for lang in langs:
        code = lang["code"]
        if code == "en":
            continue
        want = {("c", k): v for k, v in custom.items()}
        if lang["inno"] == "-":
            # Messages empty in English (BeveledLabel, TranslatorNote...) stay empty.
            want.update({("m", k): v for k, v in default.items() if v.strip()})
        path = OUT / f"{code}.isl"
        have, made_from = {}, {}
        if path.exists():
            have.update({("m", k): v for k, v in read_messages(path).items()})
            have.update({("c", k): v for k, v in read_messages(path, "CustomMessages").items()})
            made_from = read_sources(path)
        forms = T.plural_forms(code)
        done = {}
        todo = []
        for key, en in want.items():
            old = have.get(key)
            # A saved translation has the product's name where the English has {#AppName}
            # (from_model writes it out): compare like with like.
            english = to_model(en).replace("{AppName}", T.APP)
            # A custom message whose English changed since it was translated, or asked again.
            stale = key[0] == "c" and (key[1] in redo or made_from.get(key[1], en) != en)
            if old is not None and not stale and not T.check(code, english, to_model(old)):
                done[key] = old
            else:
                todo.append(key)
        state[code] = {"lang": lang, "want": want, "done": done, "forms": forms}
        if only and code not in only:
            continue
        for k in range(0, len(todo), 60):
            jobs.append((code, todo[k:k + 60], None))
    print(f"{len(state)} languages, {len(jobs)} request(s), "
          f"{sum(len(b) for _, b, _ in jobs)} messages to translate")
    if args.dry_run:
        return

    todo_of = {}
    for code, batch, _ in jobs:
        todo_of.setdefault(code, []).extend(batch)
    if args.export:
        out = Path(args.export)
        out.mkdir(parents=True, exist_ok=True)
        for code, keys in todo_of.items():
            st = state[code]
            data = {
                "code": code,
                "language": f"{st['lang']['english']} ({st['lang']['native']})",
                "system": T.system_prompt(st["lang"], st["forms"]) + INSTALLER_NOTE,
                "texts": [{"id": text_id(k), "context": k[1], "text": to_model(st["want"][k])} for k in keys],
            }
            (out / f"{code}.json").write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n",
                                              encoding="utf-8")
        print(f"exported {sum(map(len, todo_of.values()))} texts of {len(todo_of)} language(s) to {out}")
        return
    if args.import_dir:
        src = Path(args.import_dir)
        for code, keys in todo_of.items():
            path = src / f"{code}.out.json"
            if not path.exists():
                continue
            got = json.loads(path.read_text(encoding="utf-8"))
            st = state[code]
            for k in keys:
                t = T.clean(code, got.get(text_id(k)))
                why = T.check(code, to_model(st["want"][k]), t)
                if why:
                    print(f"  {code}: stays as it was: {k[1]}: {why}")
                    continue
                st["done"][k] = from_model(t)
        jobs = []

    if jobs:
        key = T.api_key()
        spend = T.Spend(args.budget)

        def run(job):
            code, batch, reasons = job
            st = state[code]
            items = []
            for i, k in enumerate(batch):
                it = {"id": str(i), "context": k[1], "text": to_model(st["want"][k])}
                if reasons:
                    it["previous_attempt_rejected_because"] = reasons[k]
                items.append(it)
            prompt = T.system_prompt(st["lang"], st["forms"]) + INSTALLER_NOTE
            out = T.call(key, args.model, prompt, items, spend)
            retry = {}
            for i, k in enumerate(batch):
                t = T.clean(code, out.get(str(i)))
                why = T.check(code, to_model(st["want"][k]), t)
                if why:
                    retry[k] = why
                else:
                    st["done"][k] = from_model(t)
            return code, retry, reasons is not None

        started, pending = time.time(), jobs
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            while pending and not spend.over():
                futures = [pool.submit(run, j) for j in pending]
                pending = []
                for f in as_completed(futures):
                    try:
                        code, retry, was_retry = f.result()
                    except Exception as e:
                        print(f"request failed: {e}")
                        continue
                    if retry and not was_retry:
                        pending.append((code, list(retry), retry))
                    elif retry:
                        why = Counter(retry.values()).most_common(3)
                        print(f"{code}: kept English for {len(retry)} message(s): {why}")
        print(f"${spend.cost:.4f} over {spend.requests} requests, {round(time.time() - started)} s")

    # The files: one .isl per language Inno has none for, and languages.iss.
    entries, customs = [], []
    for lang in langs:
        code = lang["code"]
        name = inno_name(code)
        if code == "en":
            entries.append(f'Name: "{name}"; MessagesFile: "compiler:Default.isl"')
            continue
        st = state[code]
        if lang["inno"] != "-":
            entries.append(f'Name: "{name}"; MessagesFile: "compiler:Languages\\{lang["inno"]}.isl,languages\\{code}.isl"')
        else:
            entries.append(f'Name: "{name}"; MessagesFile: "languages\\{code}.isl"')
        lines = [f"; Dannex Download Manager's installer in {lang['english']}: tools/installer/languages.py",
                 "; (machine-translated; a native speaker may correct any line)."]
        if lang["inno"] == "-":
            lines += ["", "[LangOptions]", f"LanguageName={lang['native']}",
                      f"LanguageID=${lcid(lang['windows']):04X}", "LanguageCodePage=0"]
            if code in RTL:
                lines.append("RightToLeft=yes")
            lines += ["", "[Messages]"]
            for k, en in default.items():
                lines.append(f"{k}={st['done'].get(('m', k), from_model(to_model(en)))}")
        lines += ["", "[CustomMessages]"]
        for k, en in custom.items():
            translated = st["done"].get(("c", k))
            # The English it was made from, so a change of the English is translated again; a
            # message still in English gets no such line.
            if translated is not None:
                lines.append(f"{SOURCE_MARK}{k}={en}")
            lines.append(f"{k}={translated if translated is not None else from_model(to_model(en))}")
        (OUT / f"{code}.isl").write_text("\n".join(lines) + "\n", encoding="utf-8-sig", newline="\r\n")
    ISS.write_text(
        "; Generated by tools/installer/languages.py from i18n/languages.tsv: don't edit.\n"
        "[Languages]\n" + "\n".join(entries) + "\n", encoding="utf-8-sig", newline="\r\n")
    print(f"wrote {ISS.relative_to(ROOT)} and {len(state)} files in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
