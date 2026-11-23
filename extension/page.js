// Dannex Download Manager's link watcher (a content script, in every page and frame):
//  - a link clicked with Options' keys for it held (Options → General → Keys...; Alt unless changed)
//    is the browser's to download: background.js leaves its download alone;
//  - a link clicked with Options' keys to force it held (Ctrl+Alt unless changed; Command+Option on
//    a Mac) goes to DDM whatever its type, as Download with DDM in the right-click menu does: the
//    page and the browser do nothing else with the click (message `force`);
//  - Safari only: it has no downloads API, so background.js can't take a download from the
//    browser. The click on a link to a file type Options lists is held here instead and offered
//    to the app (message `link`); if the app doesn't take it, the click goes on as if never held.
(() => {
  // One watcher per page: a copy cut off from the extension (after an update) gives way.
  const prev = globalThis.__mdmPage;
  if (prev && typeof prev === "object" && prev.alive()) return;
  if (prev && typeof prev.stop === "function") prev.stop();
  const ext = globalThis.browser ?? globalThis.chrome;
  const SAFARI = Boolean(ext.runtime.getManifest?.().browser_specific_settings?.safari);
  // A Mac: Options' Ctrl is the Command key there (a Control-click is the secondary click, and the
  // browsers give Command what Ctrl does elsewhere: a new tab).
  const nav = globalThis.navigator || {};
  const MAC = /^Mac/i.test(nav.platform || "") || /^mac/i.test((nav.userAgentData && nav.userAgentData.platform) || "");
  // The top frame asks for fresh rules; the page's frames go by the same stored copy.
  let top = true;
  try {
    top = globalThis.top === globalThis.window;
  } catch {
    top = false; // another site's frame
  }

  const tell = (message) => {
    try {
      return ext.runtime.sendMessage(message).catch(() => null);
    } catch {
      // The extension was updated or removed since this page loaded: nothing to tell.
      return Promise.resolve(null);
    }
  };

  // ---- the rules, as background.js keeps them: the keys; Safari's file types -----------------

  // The combinations Options offers (mdm_ipc::capture::KEY_CHOICES), "" none.
  const KEY_CHOICES = ["", "alt", "ctrl", "shift", "ctrl+alt", "alt+shift", "ctrl+shift", "ctrl+alt+shift"];
  // {prevent, force}: before the rules are known (or from an app before Keys...), Alt leaves a link
  // to the browser, as it always did, and nothing forces one.
  let keys = { prevent: "alt", force: "" };
  let patterns = [];
  const learn = (rules) => {
    const k = rules && rules.keys && typeof rules.keys === "object" ? rules.keys : null;
    const choice = (c) => (KEY_CHOICES.includes(c) ? c : "");
    keys = k ? { prevent: choice(k.prevent), force: choice(k.force) } : { prevent: "alt", force: "" };
    if (keys.force === keys.prevent) keys.force = "";
    // As background.js typePatterns reads the list: names separated by spaces, `*` any run.
    patterns =
      SAFARI && rules && rules.capture
        ? String(rules.types || "")
            .split(/\s+/)
            .filter(Boolean)
            .map((t) => new RegExp("^" + t.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$", "i"))
        : [];
  };
  const onStorage = (changes, area) => {
    if (area === "local" && changes.rules) learn(changes.rules.newValue);
  };
  try {
    ext.storage.local.get({ rules: null }).then((v) => learn(v.rules), () => {});
    ext.storage.onChanged.addListener(onStorage);
  } catch {
    // Cut off from the extension already: Alt only.
  }
  // The rules refreshed when old (keys changed in Options apply at the next click). Safari's
  // background script sees no downloads, so nothing else would ask the app for them there.
  if (top || SAFARI) tell({ type: "rules" });

  // The keys held for a click, as Options names them ("ctrl+alt"); "?" with one Options has no name
  // for (Windows' key, a Mac's Control).
  const held = (e) => {
    if (MAC ? e.ctrlKey : e.metaKey) return "?";
    const on = [MAC ? e.metaKey : e.ctrlKey, e.altKey, e.shiftKey];
    return ["ctrl", "alt", "shift"].filter((_, i) => on[i]).join("+");
  };

  // The file type a link leads to: its `download` name's, else its path's last name's.
  const typeOf = (a) => {
    let name = a.getAttribute("download") || "";
    if (!name) {
      try {
        name = new URL(a.href).pathname;
      } catch {
        return "";
      }
    }
    const base = name.split("/").pop() || "";
    const dot = base.lastIndexOf(".");
    return dot > 0 ? base.slice(dot + 1) : "";
  };

  const hold = (e, a) => {
    e.preventDefault();
    // The `download` name, when the link has one: the app's rules decide by it, as page.js did.
    const name = a.getAttribute("download") || "";
    tell({ type: "link", url: a.href, ...(name ? { name } : {}) }).then((reply) => {
      if (reply && reply.taken) return;
      // Not the app's (Options, an excluded site, DDM not running): the click as it was, its
      // Cmd/Shift/Ctrl too (a new tab or window stays one). This one isn't trusted, so `note`
      // leaves it alone.
      const keys = { ctrlKey: e.ctrlKey, metaKey: e.metaKey, shiftKey: e.shiftKey };
      try {
        a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, button: 0, ...keys }));
      } catch {
        a.click();
      }
    });
  };

  // Forced to DDM: neither the page's own handling of the click (a script that would open
  // something else) nor the browser's (the tab Ctrl opens) happens. The link's `download` name
  // goes with it.
  const force = (e, a) => {
    e.preventDefault();
    e.stopPropagation();
    const name = a.getAttribute("download") || "";
    tell({ type: "force", url: a.href, ...(name ? { name } : {}) });
  };

  const note = (e) => {
    if (!e.isTrusted || !(e.target instanceof Element)) return;
    const a = e.target.closest("a[href], area[href]");
    if (!a) return;
    const k = held(e);
    // The primary button only (a middle click opens a tab, as it always does). FTP too: browsers
    // no longer download it, DDM does.
    if (keys.force && k === keys.force && e.type === "click" && e.button === 0 && /^(https?|ftps?|ftpes):/i.test(a.href)) {
      return force(e, a);
    }
    if (!/^https?:/i.test(a.href)) return;
    if (keys.prevent && k === keys.prevent) {
      tell({ type: "bypass", url: a.href });
      return;
    }
    // Safari: a plain click (or Cmd/Shift: a link opened elsewhere is held the same).
    if (SAFARI && e.type === "click" && e.button === 0 && patterns.length) {
      const type = typeOf(a);
      if (type && patterns.some((p) => p.test(type))) hold(e, a);
    }
  };
  addEventListener("click", note, true);
  addEventListener("auxclick", note, true);
  globalThis.__mdmPage = {
    alive: () => Boolean(ext.runtime?.id),
    stop: () => {
      removeEventListener("click", note, true);
      removeEventListener("auxclick", note, true);
      try {
        ext.storage.onChanged.removeListener(onStorage);
      } catch {
        // Cut off already.
      }
    },
  };
})();
