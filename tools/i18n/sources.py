"""The app's Rust sources (crates/app/src), by module.

A module split into a folder of files (`app.rs` with `app/*.rs`, `winui.rs` with `winui/*.rs`)
is one module to the catalog: its files are read as one text under the top-level file's name, so
a text's note says where it shows ("in the main window") wherever in the folder its code sits,
and a closure or helper of the module keeps translating its labels in all of its files.
"""

from pathlib import Path

SRC = Path(__file__).resolve().parents[2] / "crates" / "app" / "src"


def module_of(path):
    """The top-level file a source belongs to: `app.rs` for `app.rs` and `app/menus.rs`."""
    rel = Path(path).resolve().relative_to(SRC)
    return rel.parts[0] if len(rel.parts) == 1 else rel.parts[0] + ".rs"


def files():
    """Every .rs file under crates/app/src: each top-level file, then its folder's files."""
    out = []
    for top in sorted(SRC.glob("*.rs")):
        out.append(top)
        folder = SRC / top.stem
        if folder.is_dir():
            out.extend(sorted(folder.rglob("*.rs")))
    # A folder without a top-level file of its own name (none today) is read too.
    seen = set(out)
    out.extend(f for f in sorted(SRC.rglob("*.rs")) if f not in seen)
    return out


def modules():
    """{top-level file name: the module's text}, its files joined in `files()` order."""
    texts = {}
    for f in files():
        name = module_of(f)
        text = f.read_text(encoding="utf-8")
        texts[name] = texts[name] + "\n" + text if name in texts else text
    return texts
