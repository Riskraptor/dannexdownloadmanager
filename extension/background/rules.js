// The rules (Options in the app), kept in storage between the browser's stops, and the downloads
// left to the browser. A part of the background: background.js loads the parts in order, into one
// scope.

// ---- the rules: Options in the app, kept here between the browser's stops ------------------

// {types, capture, captureOff, excludedSites, excludedAddresses, opened, minKb, keys, menu, store, at};
// null until first asked. `captureOff`: why `capture` is false (a host before #208 doesn't say).
// `keys`: Options → General → Keys... ({prevent, force}, page.js holds a click by them); `menu`:
// the right-click items to show (Customize DDM menu items, MENUS' names).
let rules = null;

async function storedRules() {
  if (!rules) rules = (await ext.storage.local.get({ rules: null })).rules;
  return rules;
}

// The key combinations Options offers (mdm_ipc::capture::KEY_CHOICES), "" none.
const KEY_CHOICES = ["", "alt", "ctrl", "shift", "ctrl+alt", "alt+shift", "ctrl+shift", "ctrl+alt+shift"];
// Options' defaults: Alt leaves a link to the browser, Ctrl+Alt has DDM take any link.
const DEFAULT_KEYS = { prevent: "alt", force: "ctrl+alt" };

// The keys the host said, each one Options offers (the same for both: the browser's). A host
// before Keys... says none: Alt leaves a link to the browser, as it always did, and nothing forces.
function keysOf(said) {
  if (!said || typeof said !== "object") return { prevent: "alt", force: "" };
  const prevent = KEY_CHOICES.includes(said.prevent) ? said.prevent : "";
  const force = KEY_CHOICES.includes(said.force) && said.force !== prevent ? said.force : "";
  return { prevent, force };
}

// One question at a time: callers meanwhile get its answer.
let asking = null;
// When a page (page.js) or the window's focus last had them asked for.
let rulesAskedAt = 0;
function refreshRules() {
  asking = asking || askRules().finally(() => (asking = null));
  return asking;
}

// The rules asked again when older than `ms` (and not asked in that time): a page that loads
// (PAGE_RULES_MS), the browser's window getting the focus back (FOCUS_RULES_MS: from DDM's Options,
// where they may have changed). Once a minute otherwise, at a download (`currentRules`).
const PAGE_RULES_MS = 15000;
const FOCUS_RULES_MS = 5000;
async function freshRules(ms) {
  const r = await storedRules();
  if ((!r || Date.now() - r.at > ms) && Date.now() - rulesAskedAt > ms) {
    rulesAskedAt = Date.now();
    refreshRules().catch(() => {});
  }
}

async function askRules() {
  const reply = await native({ rules: true });
  if (reply.ok && typeof reply.types === "string") {
    rules = {
      types: reply.types,
      capture: reply.capture !== false,
      captureOff: typeof reply.captureOff === "string" ? reply.captureOff : "",
      excludedSites: reply.excludedSites || "",
      excludedAddresses: Array.isArray(reply.excludedAddresses) ? reply.excludedAddresses : [],
      opened: reply.opened !== false,
      minKb: Number(reply.minKb) || 0,
      keys: keysOf(reply.keys),
      // A host before Customize DDM menu items says none: all of them.
      menu: Array.isArray(reply.menu) ? MENU_NAMES.filter((n) => reply.menu.includes(n)) : MENU_NAMES,
      // The Chrome Web Store's page once its listing is public (`storeOffer`); none from an
      // older DDM, or before.
      store: typeof reply.store === "string" ? reply.store : "",
      at: Date.now(),
    };
    await ext.storage.local.set({ rules });
    await syncOpenedRule();
    await syncMenus();
    // The toolbar button follows DDM's Options too (off there: OFF).
    await showStatus();
    return rules;
  }
  // No DDM on this computer, another one chosen: the app's default file types (the browser
  // keeps the files it would open), its default keys and every right-click item.
  if (await remoteTarget()) {
    rules = { types: DEFAULT_TYPES, capture: true, excludedSites: "", excludedAddresses: [], opened: false, minKb: 0, keys: DEFAULT_KEYS, menu: MENU_NAMES, at: Date.now() };
    // Kept as the app's are: page.js holds links by the stored copy.
    await ext.storage.local.set({ rules });
    await syncMenus();
    return rules;
  }
  return null;
}

// The rules to decide by now: the stored ones at once (refreshed meanwhile once a minute old;
// the host checks every offer again), else asked. None while the host can't be reached: then
// the browser keeps everything.
async function currentRules() {
  const r = await storedRules();
  const { why } = await readStatus();
  if (!r || (why && why !== "update-app")) return refreshRules();
  if (Date.now() - r.at > 60000) refreshRules();
  return r;
}

function typePatterns(list) {
  return list
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => new RegExp("^" + t.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$", "i"));
}

function extensionOf(name) {
  const base = name.split(/[\\/]/).pop() || "";
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1) : "";
}

// As the app matches them (mdm_ipc::capture): `*.example.com` takes the domain and its
// subdomains; an address pattern's `*` any run of characters.
function hostMatches(host, pattern) {
  let p = pattern.trim().toLowerCase().replace(/^[a-z]+:\/\//, "").split("/")[0];
  if (p.startsWith("*.")) {
    p = p.slice(2);
    return host === p || host.endsWith("." + p);
  }
  return p !== "" && host === p;
}

function wildcard(pattern, text) {
  const re = new RegExp("^" + pattern.toLowerCase().split("*").map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*") + "$");
  return re.test(text.toLowerCase());
}

// An address pattern with a scheme is the whole address; without one, the rest after it.
function addressMatches(pattern, url) {
  const p = pattern.trim();
  if (!p) return false;
  return wildcard(p, p.includes("://") ? url : url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, ""));
}

// Which of Options' lists leaves `url` to the browser: "site", "address", or "" (neither).
function excludedBy(r, url) {
  let host = "";
  try {
    host = new URL(url).hostname.toLowerCase();
  } catch {
    return "site";
  }
  if (r.excludedSites.split(/\s+/).some((p) => p && hostMatches(host, p))) return "site";
  if (r.excludedAddresses.some((p) => addressMatches(p, url))) return "address";
  return "";
}

// ---- downloads left to the browser: why, for the status page and DDM's log (#208) ----------

// A host's refusal by Options names the rule (`declined`); one before #208 says it only in its
// error text, by the rule's Rust name (mdm_ipc::capture::Declined).
const DECLINED_NAMES = {
  IntegrationOff: "integration-off",
  Browser: "browser",
  FileType: "file-type",
  Site: "site",
  Address: "address",
  Small: "small",
};
function declinedOf(reply) {
  if (typeof reply.declined === "string" && reply.declined) return reply.declined;
  const m = /left to the browser by Options \((\w+)\)/.exec(String(reply.error || ""));
  return (m && DECLINED_NAMES[m[1]]) || "";
}

function siteOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

// The last download left to the browser by Options (or because the extension may not read the
// site): kept for the status page ({why, name, site, at}; a private window's without its name or
// site), and, when the extension decided alone (`told` false), told to the host for DDM's log.
// The host logs its own refusals itself. A download taken later clears it.
async function noteLeft(why, name, url, incognito, told) {
  const declined = incognito ? { why, name: "", site: "", at: Date.now() } : { why, name, site: siteOf(url), at: Date.now() };
  await ext.storage.local.set({ declined });
  if (told) return;
  // Under a key of its own, with no address: an older host refuses it as one it can't
  // download, and hands nothing over.
  await native({ declined: { why, name: declined.name, site: declined.site }, ...(incognito ? { private: true } : {}) });
}
