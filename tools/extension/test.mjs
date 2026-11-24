// Runs extension/background.js against fake Firefox and Chrome APIs (no browser needed):
//   node tools/extension/test.mjs
import { readFileSync, readdirSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const code = readFileSync(new URL("../../extension/background.js", import.meta.url), "utf8");
const DEFAULT_TYPES = "3GP 7Z AAC EXE ISO MP3 MP4 MSI PDF R0* RAR ZIP";

// A fake extension API; `firefox` picks which download event exists. `reply` is the app's
// answer to a download; `types` its Options -> File types when the rules are asked (`host: false`:
// the host isn't registered and every native message fails, as the browser says then).
// `appName`: what runtime.getBrowserInfo says (Firefox's family only; "Zen").
// `session`: storage.session's contents (shared between two loads: the browser stopped the
// script and started it again); `tabs`: tabs.get's answers by id. `proxyPerm`: how the manifest has the "proxy" permission
// ("optional": Firefox's; "required"; none: Chrome's, it can't be optional there), `proxyGranted`
// whether it is granted (a required one always is), `proxyAnswer` what permissions.request says.
function fakeApi({
  firefox,
  // Safari: no downloads API at all (nor proxy and declarativeNetRequest).
  safari,
  reply = { ok: true },
  // What the page's own player says it plays (video.js's button on YouTube); undefined: it can't be read.
  playing,
  types = DEFAULT_TYPES,
  host = true,
  // What the browser says when the host can't be reached (with `host: false`).
  hostError,
  finished,
  panelOn,
  access = true,
  rules = {},
  rulesReply,
  appName,
  proxy,
  session,
  tabs = {},
  proxyPerm = firefox ? "optional" : undefined,
  proxyGranted = false,
  proxyAnswer = true,
  proxyOn = false,
  noticeReply,
  // The host's answer to the video button's lookups (`videoLookup`, `videoPoll`, `videoPick`): a
  // function of the message; none: `reply`.
  lookup,
  // What the page's pictures, videos and sounds are (pageMedia's result in the page).
  pageMediaResult = ["https://x.test/a.jpg"],
  // Download panels' settings with the app's yes to "may it show here" (`panel`); none: an app
  // before them.
  panelLookReply,
  // This copy's ID (the Chrome Web Store's, the one loaded from DDM's folder; "ours": a build of
  // its own), and the other copies of ours in the browser by ID, as each answers another's
  // question ({why}; none: not installed). A function of (id, message): the answer itself.
  id = "ours",
  copies = {},
  // How long the other copy takes to answer (ms): a worker it has to wake.
  copyDelay = 0,
  // What the browser says of this extension: its install's kind (management.getSelf), whether it
  // may run in private windows, whether it may read every site.
  installType = "normal",
  incognitoAllowed = true,
  allSites,
}) {
  const raw = [];
  // Objects made inside the sandbox have its prototypes: compare plain copies.
  const calls = { push: (c) => raw.push(JSON.parse(JSON.stringify(c))) };
  const listeners = {};
  // Every listener of an event is called (background.js has two for some); the answer is the
  // last one given.
  const event = (name) => {
    const fns = [];
    return {
      addListener: (f) => {
        fns.push(f);
        listeners[name] = (...a) => fns.reduce((r, g) => g(...a) ?? r, undefined);
      },
    };
  };
  // `proxyOn`: Chromium's proxy switch (#42), stored by the options page.
  const store = proxyOn ? { proxyOn: true } : {};
  const granted = { proxy: proxyGranted };
  const downloads = {
    pause: async (id) => calls.push(["pause", id]),
    cancel: async (id) => calls.push(["cancel", id]),
    erase: async (q) => calls.push(["erase", q.id]),
    resume: async (id) => calls.push(["resume", id]),
    // `finished`: the download completed before the extension could stop it (a small file).
    search: async ({ id }) => [{ id, state: finished ? "complete" : "interrupted", exists: true }],
    removeFile: async (id) => calls.push(["removeFile", id]),
  };
  if (firefox) downloads.onCreated = event("created");
  else downloads.onDeterminingFilename = event("determining");
  const api = {
    downloads,
    storage: {
      local: {
        get: async (defaults) => ({ ...defaults, ...JSON.parse(JSON.stringify(store)) }),
        set: async (v) => {
          const changes = Object.fromEntries(Object.entries(v).map(([k, n]) => [k, { oldValue: store[k], newValue: n }]));
          Object.assign(store, JSON.parse(JSON.stringify(v)));
          listeners.storageChanged && listeners.storageChanged(changes, "local");
        },
        remove: async (k) => [k].flat().forEach((key) => delete store[key]),
      },
      onChanged: event("storageChanged"),
      ...(session
        ? {
            session: {
              get: async (defaults) => ({ ...defaults, ...JSON.parse(JSON.stringify(session)) }),
              set: async (v) => Object.assign(session, JSON.parse(JSON.stringify(v))),
            },
          }
        : {}),
    },
    cookies: {
      getAll: async (q) => {
        calls.push(["cookies", q]);
        // As Chrome gives one: its other fields aren't sent.
        return [{ name: "sid", value: "1", domain: ".tube.test", hostOnly: false, path: "/", secure: true, httpOnly: true, sameSite: "lax", session: false, expirationDate: 1893456000.5, storeId: "0" }];
      },
    },
    permissions: {
      // The "proxy" permission (#42), every site (the status page), else a site's access.
      contains: async (q) => {
        if ((q.origins || []).includes("<all_urls>")) return allSites ?? access;
        if (!(q.permissions || []).includes("proxy")) return access;
        calls.push(["contains-proxy"]);
        return proxyPerm === "required" || (proxyPerm === "optional" && granted.proxy);
      },
      request: async (q) => {
        calls.push(["request", q.permissions]);
        if (proxyPerm !== "optional" || !proxyAnswer) return false;
        granted.proxy = true;
        listeners.permAdded && listeners.permAdded(q);
        return true;
      },
      remove: async (q) => {
        calls.push(["remove", q.permissions]);
        granted.proxy = false;
        listeners.permRemoved && listeners.permRemoved(q);
        return true;
      },
      onAdded: event("permAdded"),
      onRemoved: event("permRemoved"),
    },
    runtime: {
      sendNativeMessage: async (name, m) => {
        if (!host) throw new Error(hostError || "Specified native messaging host not found.");
        if (m.rules) {
          calls.push(["rules", m]);
          // `rulesReply`: the whole answer, as an older app's host gives it.
          if (rulesReply) return rulesReply;
          return { ok: true, version: "0.2.5", types, capture: true, excludedSites: "*.update.microsoft.com", excludedAddresses: [], opened: true, ...rules };
        }
        calls.push(["native", name, m]);
        // The video button: "may it show" follows `panelOn` (a function: the app's answer now).
        if (m.panel) return { ok: typeof panelOn === "function" ? panelOn(m) : panelOn !== false, ...(panelLookReply ? { panel: panelLookReply } : {}) };
        // A notice of a download left alone (#208): an older host, which knows none, takes it
        // for an address it can't download (`noticeReply`).
        if (m.declined) return noticeReply || { ok: true };
        if (lookup && (m.videoLookup || m.videoPoll || m.videoPick)) return lookup(m);
        return reply;
      },
      getManifest: () => ({
        version: "0.2.5",
        content_scripts: [{ js: ["video.js"] }, { js: ["page.js"] }],
        permissions: [...(safari ? [] : ["downloads"]), ...(proxyPerm === "required" ? ["proxy"] : [])],
        ...(safari ? { browser_specific_settings: { safari: {} } } : {}),
        ...(proxyPerm === "optional" ? { optional_permissions: ["proxy"] } : {}),
      }),
      onInstalled: event("installed"),
      onStartup: event("startup"),
      onMessage: event("message"),
      onMessageExternal: event("external"),
      // Another extension asked by ID: a copy of ours answers as it would (`copies`).
      sendMessage: async (to, m) => {
        if (copyDelay) await new Promise((r) => setTimeout(r, copyDelay));
        calls.push(["ask-copy", to, m.type]);
        const there = typeof copies === "function" ? copies(to, m) : copies[to];
        if (there === undefined) throw new Error("Could not establish connection. Receiving end does not exist.");
        return m.type === "ddm-copy" ? { ddmCopy: true, version: "0.2.5", why: null, reached: true, paused: false, ...there } : undefined;
      },
      openOptionsPage: async () => calls.push(["options-page"]),
      id,
      getURL: (p) => `chrome-extension://${id}/${p}`,
    },
    management: { getSelf: async () => ({ id, installType }) },
    extension: { isAllowedIncognitoAccess: async () => incognitoAllowed },
    tabs: {
      onCreated: event("tabCreated"),
      onRemoved: event("tabRemoved"),
      onUpdated: event("tabUpdated"),
      get: async (id) => tabs[id] || { id, url: "about:blank", status: "complete" },
      remove: async (id) => calls.push(["closeTab", id]),
      query: async () => [],
      create: async (t) => calls.push(["openTab", t.url]),
      update: async (tabId, u) => calls.push(["tab-update", tabId, u]),
    },
    webRequest: {
      onBeforeRequest: event("beforeRequest"),
      onBeforeSendHeaders: event("beforeSendHeaders"),
      onHeadersReceived: event("headersReceived"),
      onResponseStarted: event("responseStarted"),
      onAuthRequired: event("authRequired"),
    },
    scripting: {
      executeScript: async ({ func, args }) => {
        if (func && func.name === "playingAt") {
          if (playing === undefined) throw new Error("Cannot access contents of the page");
          return [{ result: playing }];
        }
        if (func && func.name === "pageMedia") {
          calls.push(["pageMedia", args]);
          return [{ result: pageMediaResult }];
        }
        return func ? [{ result: ["https://x/1.zip", "https://x/2.zip"] }] : [];
      },
    },
    action: {
      setBadgeText: async (b) => calls.push(["badge", b.text]),
      setBadgeBackgroundColor: async (b) => calls.push(["badge-color", b.color]),
      setTitle: async (b) => calls.push(["title", b.title]),
      setIcon: async (b) => calls.push(["icon", b.path[16]]),
      onClicked: event("toolbar"),
    },
    contextMenus: {
      create: (item, done) => {
        calls.push(["menu-item", item]);
        if (done) done();
      },
      removeAll: (done) => {
        calls.push(["menu-clear"]);
        done();
      },
      onClicked: event("menu"),
    },
    // The browser's windows: one getting the focus (-1: none has it), one made, one brought up.
    windows: {
      WINDOW_ID_NONE: -1,
      onFocusChanged: event("focus"),
      create: async (w) => calls.push(["window", w.url, w.type]),
      update: async (windowId, u) => calls.push(["window-update", windowId, u]),
    },
    // The browser's language: German here.
    i18n: { getMessage: (key) => ({ menu: "Mit Dannex Download Manager herunterladen" })[key] ?? "" },
  };
  if (safari) delete api.downloads;
  if (!firefox && !safari) api.declarativeNetRequest = { getSessionRules: async () => [], updateSessionRules: async (r) => calls.push(["dnr", r]) };
  if (appName) api.runtime.getBrowserInfo = async () => ({ name: appName, vendor: "Mozilla", version: "1.0" });
  // `proxy`: what proxy.settings.get says (#42; Chromium {mode, rules, pacScript}, Firefox {proxyType, …}).
  if (proxy) api.proxy = { settings: { get: async () => ({ value: proxy, levelOfControl: "controlled_by_other_extensions" }) } };
  return { api, calls: raw, listeners, store };
}

// The servers the extension reads manifests from (#38): address -> {status, body}.
let served = {};
const fetched = [];
// The requests to another DDM's remote control (#157): {url, method, body, auth}.
const remoteAsked = [];
async function fakeFetch(url, opts) {
  fetched.push([String(url), opts && opts.credentials]);
  if (opts && opts.headers && opts.headers.Authorization) {
    remoteAsked.push({ url: String(url), method: opts.method, body: opts.body ? JSON.parse(opts.body) : null, auth: opts.headers.Authorization });
  }
  const r = served[url];
  if (!r) throw new TypeError("Failed to fetch");
  // Its body as a stream too, 1 KB a chunk: `r.read` counts what was pulled (#171 reads 4 KB).
  const bytes = new TextEncoder().encode(r.body);
  r.read = 0;
  const body = new ReadableStream({
    pull(c) {
      if (r.read >= bytes.length) return c.close();
      c.enqueue(bytes.slice(r.read, r.read + 1024));
      r.read += 1024;
    },
  });
  return { ok: (r.status || 200) < 300, status: r.status || 200, headers: { get: () => null }, body, text: async () => r.body, json: async () => JSON.parse(r.body) };
}

// What background.js wrote to the console as an error or a warning (#42: none, granted or not).
const complaints = [];
const quietConsole = { ...console, error: (...a) => complaints.push(["error", ...a]), warn: (...a) => complaints.push(["warn", ...a]) };

// `extra`: more of the script's globals (a `Date` whose clock the test moves, a `setTimeout`).
// Its timers of a second or more (a host's time to answer, a copy's question) don't keep these
// tests running once they are done. Returns the script's globals.
function load(api, firefox, extra = {}) {
  const ctx = {
    fetch: fakeFetch,
    navigator: { userAgent: "UA", userAgentData: { brands: [{ brand: "Chromium" }, { brand: "Brave" }] } },
    console: quietConsole,
    URL,
    URLSearchParams,
    TextDecoder,
    setTimeout: (f, ms, ...a) => {
      const timer = setTimeout(f, ms, ...a);
      if (ms >= 1000) timer.unref();
      return timer;
    },
    clearTimeout,
    Promise,
  };
  if (firefox) ctx.browser = api;
  else ctx.chrome = api;
  Object.assign(ctx, extra);
  vm.runInNewContext(code, ctx);
  return ctx;
}

const settle = () => new Promise((r) => setTimeout(r, 30));
// The messages to the host about downloads, and apart, the notices of downloads the extension
// left to the browser by Options (#208).
const natives = (calls) => calls.filter((c) => c[0] === "native" && !c[2].declined).map((c) => c[2]);
const notices = (calls) => calls.filter((c) => c[0] === "native" && c[2].declined).map((c) => c[2].declined);
const steps = (calls) => calls.filter((c) => ["pause", "cancel", "erase", "resume", "removeFile"].includes(c[0])).map((c) => c[0]);
let passed = 0;
const ok = (what) => {
  passed++;
  console.log(`ok - ${what}`);
};

// Firefox: a .zip in a container is taken over with that container's cookies and the browser's
// name, size and type; every message says which browser and version it comes from.
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  listeners.created({
    id: 7,
    url: "https://x/a.zip",
    filename: "C:\\dl\\a.zip",
    referrer: "https://page/",
    cookieStoreId: "firefox-container-2",
    totalBytes: 2048,
    mime: "application/zip",
  });
  await settle();
  const cookies = calls.find((c) => c[0] === "cookies");
  assert.deepEqual(cookies[1], { url: "https://x/a.zip", storeId: "firefox-container-2" });
  const native = calls.find((c) => c[0] === "native");
  assert.equal(native[1], "com.dannexdownloadmanager.host");
  const m = native[2];
  assert.deepEqual(
    [m.url, m.referrer, m.cookies, m.filename, m.size, m.mime, m.userAgent, m.v],
    ["https://x/a.zip", "https://page/", "sid=1", "a.zip", 2048, "application/zip", "UA", "0.2.5"],
  );
  assert.deepEqual(m.brands, ["Chromium", "Brave"]);
  assert.equal(m.cookieJar, undefined, "a file's download: the header the app sends");
  assert.deepEqual(steps(calls), ["pause", "cancel", "erase"]);
  ok("Firefox: taken with its container's cookies, size and type");
}

// Zen (Firefox's family) says its own name through runtime.getBrowserInfo: it goes with the
// brands, so the app tells Zen from Firefox (#41).
{
  const { api, calls, listeners } = fakeApi({ firefox: true, appName: "Zen" });
  load(api, true);
  listeners.created({ id: 8, url: "https://x/b.zip", filename: "C:\\dl\\b.zip", totalBytes: 10, mime: "application/zip" });
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual(m.brands, ["Chromium", "Brave", "Zen"]);
  ok("Zen: its own name goes with the brands");
}

// Firefox: the app doesn't take it → the browser's download resumes; nothing is lost.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, reply: { ok: false } });
  load(api, true);
  listeners.created({ id: 8, url: "https://x/b.iso", filename: "/home/u/b.iso" });
  await settle();
  assert.deepEqual(steps(calls), ["pause", "resume"]);
  ok("Firefox: declined, resumed");
}

// A type not on the list (a web page) stays with the browser.
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  listeners.created({ id: 9, url: "https://x/page.html", filename: "C:\\dl\\page.html" });
  await settle();
  assert.deepEqual(natives(calls), []);
  assert.deepEqual(steps(calls), []);
  // Said, for DDM's log, under its own key and without an address.
  assert.deepEqual(notices(calls), [{ why: "file-type", name: "page.html", site: "x" }]);
  assert.ok(!calls.some((c) => c[0] === "native" && c[2].declined && "url" in c[2]));
  ok("a type not listed stays with the browser, and DDM's log hears why");
}

// Chrome: incognito downloads use the incognito store; the address asked for goes as the
// download's (an expired signed link is renewed from it), the redirect's end beside it.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  let suggested = false;
  listeners.determining(
    { id: 3, url: "https://x/redirect", finalUrl: "https://cdn/x.msi", filename: "x.msi", incognito: true },
    () => (suggested = true),
  );
  await settle();
  assert.ok(!suggested, "a download the app took is never named: Chrome wrote none of it");
  const cookies = calls.find((c) => c[0] === "cookies");
  assert.deepEqual(cookies[1], { url: "https://x/redirect", storeId: "1" });
  const [m] = natives(calls);
  assert.deepEqual([m.url, m.finalUrl, m.private], ["https://x/redirect", "https://cdn/x.msi", true]);
  assert.deepEqual(steps(calls), ["cancel", "erase"], "held at its name, not paused and resumed");
  ok("Chrome: taken at its name, incognito cookies, both addresses");
}

// Chrome: a page that left for the file by script (Chromium counts it as the download's start,
// #374) isn't the address of the file.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const page = "https://site.test/welcome";
  listeners.responseStarted({ type: "main_frame", tabId: 1, frameId: 0, statusCode: 200, url: page, responseHeaders: [{ name: "Content-Type", value: "text/html; charset=UTF-8" }] });
  listeners.determining({ id: 7, url: page, finalUrl: "https://cdn.test/x.exe", filename: "x.exe" }, () => {});
  await settle();
  const [m] = natives(calls);
  assert.deepEqual([m.url, m.finalUrl], ["https://cdn.test/x.exe", ""], "the file's address, not the page's");
  ok("Chrome: a page that went to the file by script isn't the file's address");
}

// Chrome: one the app doesn't take goes on as the browser's own (its name confirmed).
{
  const { api, calls, listeners } = fakeApi({ firefox: false, reply: { ok: false } });
  load(api, false);
  let suggested = false;
  listeners.determining({ id: 4, url: "https://x/y.zip", filename: "y.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested);
  assert.deepEqual(steps(calls), []);
  ok("Chrome: declined, the browser's own");
}

// Chrome, #578: a download left to the browser keeps another extension's name. What Chromium
// does with each listener's answer (downloads_custom_bindings.js, then downloads_api.cc
// DetermineFilename and DeterminerCallback): no valid suggestion goes as an empty name with no
// conflict action, which counts as a name; a name the browser refuses is emptied after the
// answer is counted, and only an empty name with "uniquify" is no name at all. The newest
// extension's name wins (here DDM's, updated after Save In was installed).
{
  const answer = (s) => {
    const valid = s && typeof s === "object" && typeof s.filename === "string" && s.filename.length > 0;
    const [name, action] = valid ? [s.filename, s.conflictAction || ""] : ["", ""];
    const safe = name && !name.split(/[\/]/).some((p) => p === "" || p === "." || p === "..") && !/^[a-z]:|^[\/]/i.test(name);
    const filename = safe ? name : "";
    return filename || action !== "uniquify" ? { filename } : null; // null: no suggestion
  };
  // The determination of one download: `answers` in install order, oldest first.
  const determined = (creator, answers) => {
    let name = creator;
    let by = -1;
    answers.forEach((a, i) => {
      const s = answer(a);
      if (s && i >= by) [name, by] = [s.filename, i];
    });
    return name;
  };
  for (const [what, item, creator] of [
    ["its own listener's folder (Save In)", { id: 70, url: "https://img.test/cat.jpg", filename: "cat.jpg", byExtensionId: "savein" }, ""],
    ["a downloads.download filename", { id: 71, url: "https://img.test/dog.jpg", filename: "dog.jpg", byExtensionId: "savein" }, "Pic/OthersW/dog.jpg"],
    ["a page's download", { id: 72, url: "https://x/page.html", filename: "page.html" }, ""],
  ]) {
    const { api, listeners } = fakeApi({ firefox: false });
    load(api, false);
    const other = item.byExtensionId && !creator ? { filename: `Pic/OthersW/${item.filename}` } : undefined;
    let mdm = "none";
    listeners.determining(item, (s) => (mdm = s));
    await settle();
    assert.notEqual(mdm, "none", `${what}: answered, the download doesn't wait`);
    const want = other ? other.filename : creator;
    assert.equal(determined(creator, other ? [other, mdm] : [mdm]), want, `${what}: DDM's answer gives no name of its own`);
    // A name the browser refuses is logged as an error: only for another extension's download.
    const refused = Boolean(mdm && typeof mdm === "object" && mdm.filename);
    assert.equal(refused, Boolean(item.byExtensionId), `${what}: an invalid name only when another extension started it`);
  }
  // The model, checked: Chromium's own cases.
  assert.equal(determined("Pic/a.jpg", [undefined]), "", "suggest() empties the creator's name");
  assert.equal(determined("", [{ filename: "Pic/b.jpg" }, undefined]), "", "and the older extension's");
  assert.equal(determined("", [undefined, { filename: "Pic/b.jpg" }]), "Pic/b.jpg", "an older suggest() loses");
  ok("Chrome: a download left alone keeps another extension's name and folder (#578)");
}

// Firefox: a small file finished before it could be stopped: the browser's copy is removed.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, finished: true });
  load(api, true);
  listeners.created({ id: 5, url: "https://x/s.zip", filename: "C:\\dl\\s.zip" });
  await settle();
  assert.ok(calls.some((c) => c[0] === "removeFile" && c[1] === 5));
  assert.ok(calls.some((c) => c[0] === "erase" && c[1] === 5));
  ok("Firefox: a finished small file is removed");
}

// Options -> File types in the app decides what is offered: its list, not a built-in one.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, types: "PNG ISO" });
  load(api, true);
  listeners.created({ id: 20, url: "https://x/pic.png", filename: "C:\\dl\\pic.png" });
  await settle();
  listeners.created({ id: 21, url: "https://x/a.zip", filename: "C:\\dl\\a.zip" });
  await settle();
  assert.deepEqual(natives(calls).map((m) => m.filename), ["pic.png"], "PNG is the app's, ZIP is not listed there");
  assert.equal(calls.filter((c) => c[0] === "rules").length, 1, "the rules kept, not asked per download");
  ok("the app's file types decide, asked once");
}

// Dannex Download Manager not installed (no host): everything stays with the browser, and the
// toolbar button says something is wrong.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false, host: false });
  load(api, false);
  let suggested = false;
  listeners.determining({ id: 30, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested);
  assert.deepEqual(steps(calls), []);
  assert.equal(store.status.why, "not-installed");
  assert.ok(calls.some((c) => c[0] === "badge" && c[1] === "!"));
  // Firefox isn't paused for nothing either.
  const ff = fakeApi({ firefox: true, host: false });
  load(ff.api, true);
  ff.listeners.created({ id: 31, url: "https://x/a.zip", filename: "a.zip" });
  await settle();
  assert.deepEqual(steps(ff.calls), []);
  ok("no host: nothing taken, nothing paused, the badge says so");
}

// ---- the toolbar button: a click turns taking downloads off and on ---------------------------

// The toolbar button's right-click menu: only what a person needs beside its click, on the button
// only (the browser adds its own Options item: none of ours).
const TOOLBAR_ITEMS = ["mdm-toolbar-exclude", "mdm-toolbar-help"];
const toolbarItems = (calls) => {
  const at = calls.map((c) => c[0]).lastIndexOf("menu-clear");
  return calls.slice(at + 1).filter((c) => c[0] === "menu-item" && c[1].contexts.includes("action")).map((c) => c[1].id);
};
// The button as it was last drawn: {badge, color, title, icon}.
const lastOf = (calls, kind) => (calls.filter((c) => c[0] === kind).at(-1) || [])[1];
const button = (calls) => ({ badge: lastOf(calls, "badge"), color: lastOf(calls, "badge-color"), title: lastOf(calls, "title"), icon: lastOf(calls, "icon") });
// The copies' IDs (crates/ipc/src/extension.rs; a test below keeps background.js's the same).
const STORE = "plmckdlgphkcfkifahbkonbibfedckih";
const UNPACKED = "nphkobgebehllokfnmfdchmcbolllkif";
const ownAsk = (listeners, m, page = "install.html", id = "ours") =>
  new Promise((r) => listeners.message(m, { id, url: `chrome-extension://${id}/${page}` }, (v) => r(JSON.parse(JSON.stringify(v)))));

// The extension installed without the app: the status page opens, once, by itself (it says what
// is missing, offers DDM's download and turns green once DDM answers).
{
  const { api, calls, listeners } = fakeApi({ firefox: false, host: false });
  load(api, false);
  await listeners.installed({ reason: "install" });
  await settle();
  assert.deepEqual(calls.filter((c) => c[0] === "openTab").map((c) => c[1]), ["chrome-extension://ours/install.html"]);
  const items = calls.filter((c) => c[0] === "menu-item").map((c) => c[1].id);
  assert.deepEqual(items, ["mdm-link", "mdm-selected", "mdm-all", "mdm-images", "mdm-media", ...TOOLBAR_ITEMS]);
  assert.deepEqual(
    calls.filter((c) => c[0] === "menu-item").slice(2, 5).map((c) => c[1].contexts),
    [["page"], ["page"], ["page"]],
    "a page's links, pictures, videos and sounds: on the page",
  );
  // An update, or an install where DDM answers: nothing opens.
  const later = fakeApi({ firefox: false, host: false });
  load(later.api, false);
  await later.listeners.installed({ reason: "update" });
  const there = fakeApi({ firefox: false });
  load(there.api, false);
  await there.listeners.installed({ reason: "install" });
  await settle();
  assert.ok(!later.calls.some((c) => c[0] === "openTab") && !there.calls.some((c) => c[0] === "openTab"));
  ok("installed without the app: the status page, once; the menus");
}

// Connected: the click turns it off (grey with an X, "OFF", the tooltip says so and that a click
// turns it on) and on again (in colour, no badge, so it looks as any button does); nothing is
// taken while off; the switch is the browser's, not a tab's (no tab named). The status page's
// switch is the same.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false });
  load(api, false);
  listeners.startup();
  await settle();
  assert.deepEqual(button(calls), { badge: "", color: "#6B6B6B", title: "tip_on", icon: "icons/icon-16.png" });
  await listeners.toolbar({ id: 5, url: "https://x.test/" });
  await settle();
  assert.equal(store.paused, true);
  assert.deepEqual(button(calls), { badge: "badge_off", color: "#6B6B6B", title: "tip_off", icon: "icons/icon-off-16.png" });
  assert.ok(!calls.some((c) => (c[0] === "badge" || c[0] === "icon" || c[0] === "title") && c.length > 2), "for every tab");
  let suggested = false;
  listeners.determining({ id: 61, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested && natives(calls).length === 0, "off: the browser keeps it");
  const off = await ownAsk(listeners, { type: "status" });
  assert.deepEqual([off.connected, off.paused, off.look], [true, true, "off"]);
  await listeners.toolbar({ id: 6, url: "https://y.test/" });
  await settle();
  assert.equal(store.paused, false);
  assert.deepEqual(button(calls).icon, "icons/icon-16.png");
  assert.equal(button(calls).badge, "");
  // The status page's switch is the same.
  assert.deepEqual(await ownAsk(listeners, { type: "pause", paused: true }), { ok: true });
  assert.equal(button(calls).badge, "badge_off");
  assert.deepEqual(await ownAsk(listeners, { type: "toggle" }), { ok: false }, "no other way to flip it");
  assert.ok(!calls.some((c) => c[0] === "openTab" || c[0] === "window"), "a click opens nothing while connected");
  ok("the toolbar button: a click turns it off (grey, X, OFF) and on, for the whole browser");
}

// The toolbar button's panel (popup.html): how things stand, the switch, the tab's video and
// links for DDM (only a web page's), the status page.
{
  const tabs = { 9: { id: 9, url: "https://x.test/p", title: "Page", incognito: false } };
  const { api, calls, listeners } = fakeApi({ firefox: false, tabs });
  load(api, false);
  listeners.startup();
  await settle();
  const s = await ownAsk(listeners, { type: "popup", tab: 9 }, "popup.html");
  assert.deepEqual([s.ok, s.state, s.title, s.toggle, s.paused, s.web, s.videos], [true, "on", "tip_on", true, false, true, 0]);
  assert.equal((await ownAsk(listeners, { type: "popup", tab: 3 }, "popup.html")).web, false, "about:blank: no page to take from");
  await ownAsk(listeners, { type: "pause", paused: true }, "popup.html");
  const off = await ownAsk(listeners, { type: "popup", tab: 9 }, "popup.html");
  assert.deepEqual([off.state, off.paused], ["off", true]);
  await ownAsk(listeners, { type: "pause", paused: false }, "popup.html");
  assert.deepEqual(await ownAsk(listeners, { type: "popup-all", tab: 9 }, "popup.html"), { ok: true });
  const all = natives(calls).at(-1);
  assert.deepEqual([all.links, all.referrer, all.private], [["https://x/1.zip", "https://x/2.zip"], "https://x.test/p", false]);
  assert.deepEqual(await ownAsk(listeners, { type: "popup-video", tab: 9 }, "popup.html"), { ok: true });
  const video = natives(calls).at(-1);
  assert.deepEqual([video.video, video.explicit, video.url, video.title], [true, true, "https://x.test/p", "Page"]);
  const sent = natives(calls).length;
  assert.deepEqual(await ownAsk(listeners, { type: "popup-all", tab: 3 }, "popup.html"), { ok: false }, "not a web page");
  assert.equal(natives(calls).length, sent);
  await ownAsk(listeners, { type: "popup-status" }, "popup.html");
  assert.ok(calls.some((c) => c[0] === "openTab" && /install\.html/.test(c[1])), "Status and help: the status page");
  ok("the toolbar button's panel: the state, the switch, the tab's video and links, the status page");
}

// DDM not reached: grey with a red "!", the tooltip says why and that a click shows how to fix
// it; the click opens the status page (never toggles). The status page's answer says why.
for (const [why, options] of [
  ["not-installed", { host: false }],
  // An older DDM that doesn't know this copy's ID (the store's, before 0.3).
  ["forbidden", { host: false, hostError: "Access to the specified native messaging host is forbidden." }],
  ["host-failed", { host: false, hostError: "Native host has exited." }],
  // DDM didn't start in time for a download (the host waited 20 s).
  ["not-running", { reply: { ok: false, why: "not-running", error: "Dannex Download Manager did not start" } }],
]) {
  const { api, calls, listeners, store } = fakeApi({ firefox: false, ...options });
  load(api, false);
  listeners.startup();
  await settle();
  if (why === "not-running") {
    listeners.determining({ id: 70, url: "https://x/a.zip", filename: "a.zip" }, () => {});
    await settle();
  }
  assert.equal(store.status.why, why);
  assert.deepEqual(button(calls), { badge: "!", color: "#C42B1C", title: `status_${why.replace(/-/g, "_")}\nclick_fix`, icon: "icons/icon-off-16.png" });
  await listeners.toolbar({ id: 5 });
  await settle();
  assert.equal(store.paused, undefined, "never toggled");
  assert.deepEqual(calls.filter((c) => c[0] === "openTab").map((c) => c[1]), ["chrome-extension://ours/install.html"]);
  const s = await ownAsk(listeners, { type: "status" });
  assert.deepEqual([s.connected, s.why, s.look], [false, why, "problem"]);
  // The status page asks again every few seconds: DDM installed meanwhile (here: answering its
  // rules again), it turns green by itself, and so does the button.
  if (why === "not-running") {
    const again = await ownAsk(listeners, { type: "probe" });
    assert.deepEqual([again.connected, again.why, again.app, again.look], [true, null, "0.2.5", "on"]);
    assert.equal(button(calls).badge, "");
  } else {
    api.runtime.sendNativeMessage = async () => ({ ok: true, version: "0.2.5", types: "ZIP", capture: true });
    const again = await ownAsk(listeners, { type: "probe" });
    assert.deepEqual([again.connected, again.why, again.app], [true, null, "0.2.5"]);
    assert.equal(button(calls).icon, "icons/icon-16.png");
  }
  ok(`the toolbar button: ${why} is a red "!", a click opens the status page, which turns green by itself`);
}

// DDM's Options take nothing from this browser (integration off, the browser unticked): grey,
// "OFF" in amber, the tooltip says it's DDM's Options and why; the click opens the status page,
// which says the same (the switch here can't turn DDM's on).
for (const captureOff of ["integration-off", "browser"]) {
  const { api, calls, listeners, store } = fakeApi({ firefox: false, rules: { capture: false, captureOff } });
  load(api, false);
  listeners.startup();
  await settle();
  const b = button(calls);
  assert.deepEqual([b.badge, b.color, b.icon], ["badge_off", "#9D5D00", "icons/icon-off-16.png"]);
  assert.equal(b.title, `status_off_in_app\ndeclined_${captureOff.replace(/-/g, "_")}\nclick_details`);
  await listeners.toolbar({ id: 5 });
  await settle();
  assert.equal(store.paused, undefined);
  assert.ok(calls.some((c) => c[0] === "openTab" && c[1].endsWith("/install.html")));
  const s = await ownAsk(listeners, { type: "status" });
  assert.deepEqual([s.connected, s.captureOff, s.look], [true, captureOff, "off-in-app"]);
  ok(`the toolbar button: off in DDM's Options (${captureOff}) is an amber OFF, and a click explains`);
}

// An app older than the extension: still on, and the click still toggles; the tooltip adds the
// nudge to update it. The status page hears it (`why` update-app).
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false, rules: { version: "0.1.0" } });
  load(api, false);
  listeners.startup();
  await settle();
  assert.deepEqual(button(calls), { badge: "", color: "#6B6B6B", title: "tip_on\nstatus_update_app", icon: "icons/icon-16.png" });
  await listeners.toolbar({ id: 5 });
  await settle();
  assert.equal(store.paused, true);
  const s = await ownAsk(listeners, { type: "status" });
  assert.deepEqual([s.connected, s.why, s.app, s.behind], [true, "update-app", "0.1.0", false]);
  ok("the toolbar button: an older app still toggles, the tooltip says to update it");
}

// DDM newer than the extension (the store's review lagging, a copy not reloaded): connected, and
// the status page says this extension is behind; nothing on the button.
{
  const { api, calls, listeners } = fakeApi({ firefox: false, rules: { version: "0.4.0" }, id: UNPACKED });
  load(api, false);
  listeners.startup();
  await settle();
  assert.equal(button(calls).badge, "");
  const s = await ownAsk(listeners, { type: "probe" }, "install.html", UNPACKED);
  assert.deepEqual([s.connected, s.why, s.behind, s.copy], [true, null, true, "unpacked"]);
  ok("DDM newer than the extension: connected, the status page says it's behind");
}

// On, the button counts nothing: no badge after a hand-off, no question to DDM about its list,
// nothing kept awake (no alarm); the Downloads and links window's questions are gone (answered
// nothing, asking DDM nothing).
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  listeners.startup();
  await settle();
  listeners.determining({ id: 71, url: "https://x/a.zip", filename: "a.zip" }, () => {});
  await new Promise((r) => setTimeout(r, 120));
  assert.equal(natives(calls).length, 1, "handed over");
  assert.ok(calls.filter((c) => c[0] === "badge").every((c) => c[1] === ""), "no number, ever");
  assert.ok(!natives(calls).some((m) => m.downloads || m.downloadsAct || m.open), "DDM's list never asked, DDM never brought up");
  assert.equal(api.alarms, undefined);
  for (const type of ["downloads", "downloads-act", "add-url", "media", "media-download", "open-app", "open-panel", "open-status", "exclude-site"]) {
    assert.deepEqual(await ownAsk(listeners, { type, url: "https://x.test/a.zip", tab: 1, ids: [1], action: "pause", host: "x.test" }), { ok: false }, type);
  }
  assert.equal(natives(calls).length, 1, "nothing more sent for those");
  ok("the toolbar button: no count of DDM's downloads; the old window's questions are gone");
}

// Never reached yet (just installed, storage cleared) and no reason known: the click asks DDM
// first, then toggles (or opens the status page when DDM isn't there).
{
  const { api, store, listeners } = fakeApi({ firefox: false });
  load(api, false);
  store.rules = undefined;
  await listeners.toolbar({ id: 1 });
  await settle();
  assert.equal(typeof store.paused, "boolean");
  ok("the toolbar button: a first click asks DDM, then toggles");
}

// The toolbar button's right-click menu: Don't take downloads from this site (a "✓" a moment on
// the badge) and Help (the status page). Nothing opens DDM, no panel, no Options item of ours
// (the browser has its own); the old items, clicked from a menu made before an update, do
// nothing.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await listeners.installed({ reason: "update" });
  await settle();
  assert.deepEqual(toolbarItems(calls), TOOLBAR_ITEMS);
  const titles = calls.filter((c) => c[0] === "menu-item" && c[1].contexts.includes("action")).map((c) => c[1].title);
  assert.deepEqual(titles, ["toolbar_exclude", "toolbar_help"]);
  const tab = { id: 9, url: "https://files.test/page", incognito: false };
  await listeners.menu({ menuItemId: "mdm-toolbar-exclude" }, tab);
  await settle();
  assert.ok(natives(calls).some((m) => m.excludeSite === "files.test"));
  assert.ok(calls.some((c) => c[0] === "badge" && c[1] === "✓"));
  // On the browser's own page: no site to leave, nothing sent.
  const sent = natives(calls).length;
  await listeners.menu({ menuItemId: "mdm-toolbar-exclude" }, { id: 2, url: "chrome://newtab/" });
  assert.equal(natives(calls).length, sent);
  await listeners.menu({ menuItemId: "mdm-toolbar-help" }, tab);
  assert.deepEqual(calls.filter((c) => c[0] === "openTab").map((c) => c[1]), ["chrome-extension://ours/install.html"]);
  for (const old of ["open", "panel", "options", "status"]) await listeners.menu({ menuItemId: `mdm-toolbar-${old}` }, tab);
  await settle();
  assert.equal(natives(calls).length, sent, "Open DDM is gone");
  assert.ok(!calls.some((c) => c[0] === "window" || c[0] === "options-page"), "no panel, no Options of ours");
  assert.equal(calls.filter((c) => c[0] === "openTab").length, 1);
  // Firefox has the same menu (its toolbar button's "action" menu).
  const ff = fakeApi({ firefox: true });
  load(ff.api, true);
  await ff.listeners.installed({ reason: "update" });
  await settle();
  assert.deepEqual(toolbarItems(ff.calls), TOOLBAR_ITEMS);
  ok("the toolbar button's right-click menu: don't take this site's downloads, help");
}

// What the status page hears of the browser: Firefox's temporary add-on, private windows not
// allowed, site access limited; none of it changes the button.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, installType: "development", incognitoAllowed: false, allSites: false });
  load(api, true);
  listeners.startup();
  await settle();
  const s = await ownAsk(listeners, { type: "status" });
  assert.deepEqual([s.browser, s.temporary, s.incognito, s.allSites, s.connected], ["firefox", true, false, false, true]);
  assert.equal(button(calls).badge, "");
  // Told at its install too (Firefox's onInstalled says `temporary`).
  const told = fakeApi({ firefox: true });
  load(told.api, true);
  await told.listeners.installed({ reason: "install", temporary: true });
  assert.equal((await ownAsk(told.listeners, { type: "status" })).temporary, true);
  // Chrome's unpacked copy is "development" too, and no temporary add-on.
  const chrome = fakeApi({ firefox: false, installType: "development" });
  load(chrome.api, false);
  const c = await ownAsk(chrome.listeners, { type: "status" });
  assert.deepEqual([c.browser, c.temporary, c.incognito, c.allSites], ["chromium", false, true, true]);
  ok("the status page hears of Firefox's temporary add-on, private windows, site access");
}

// Safari: the click toggles as elsewhere (no right-click menu on its button: its creation is
// refused quietly, the options page has the same).
{
  const { api, calls, listeners, store } = fakeApi({ safari: true, firefox: true });
  api.contextMenus.create = (item, done) => {
    if (item.contexts.includes("action")) throw new TypeError("Invalid context: action");
    calls.push(["menu-item", item]);
    if (done) done();
  };
  load(api, true);
  await listeners.installed({ reason: "update" });
  await listeners.toolbar({ id: 1 });
  await settle();
  assert.equal(store.paused, true);
  assert.equal(button(calls).icon, "icons/icon-off-16.png");
  assert.ok(calls.some((c) => c[0] === "menu-item" && c[1].id === "mdm-link"), "the page's items still made");
  assert.deepEqual(complaints, []);
  ok("Safari: the click toggles; no menu on its button, quietly");
}

// Turned off, then the browser stops the script and starts it again (no browser start, no install):
// the button is drawn off straight after loading, by the script's own start.
for (const firefox of [false, true]) {
  const { api, calls, store } = fakeApi({ firefox });
  store.paused = true;
  load(api, firefox);
  await settle();
  const b = button(calls);
  assert.deepEqual([b.badge, b.icon, b.color], ["badge_off", "icons/icon-off-16.png", "#6B6B6B"], firefox ? "Firefox" : "Chrome");
  assert.equal(b.title, "tip_off");
}
ok("the toolbar button: off as it was, straight after the script starts again");

// ---- two copies in one browser: the Chrome Web Store's acts, the unpacked one stands aside ------

// background.js names the copies as crates/ipc/src/extension.rs does (the store's first).
{
  const ipc = readFileSync(new URL("../../crates/ipc/src/extension.rs", import.meta.url), "utf8");
  assert.equal(ipc.match(/pub const STORE_ID: &str = "([a-p]{32})"/)[1], STORE);
  assert.equal(ipc.match(/pub const UNPACKED_ID: &str = "([a-p]{32})"/)[1], UNPACKED);
  assert.ok(code.includes(`const STORE_COPY = "${STORE}";`) && code.includes(`const UNPACKED_COPY = "${UNPACKED}";`));
  assert.ok(/const COPIES = \[STORE_COPY, UNPACKED_COPY\];/.test(code), "the store's preferred");
  const manifest = JSON.parse(readFileSync(new URL("../../extension/manifest.json", import.meta.url), "utf8"));
  assert.equal(manifest.externally_connectable, undefined, "other extensions may ask by ID, no web page may");
  ok("two copies: the same IDs as the native host, the store's first");
}

// The unpacked copy with the store's copy installed (and working) in the same browser: it stands
// aside. It takes no download, shows none of the page's right-click items (the store's are there)
// and only Help on its button, answers its pages nothing (one Download video button,
// one hand-off per Ctrl+Alt click), drops its rule for PDFs; its button is grey with an amber "!"
// and "Remove this copy", and a click opens the status page, which offers to remove it.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false, id: UNPACKED, copies: { [STORE]: { why: null } } });
  load(api, false);
  await listeners.installed({ reason: "update" });
  await settle();
  assert.ok(calls.some((c) => c[0] === "ask-copy" && c[1] === STORE && c[2] === "ddm-copy"));
  assert.ok(!calls.some((c) => c[0] === "ask-copy" && c[1] === UNPACKED), "never asks itself");
  let suggested = false;
  listeners.determining({ id: 80, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested && natives(calls).length === 0, "the browser's: the store's copy takes it");
  const at = calls.map((c) => c[0]).lastIndexOf("menu-clear");
  assert.deepEqual(calls.slice(at + 1).filter((c) => c[0] === "menu-item").map((c) => c[1].id), ["mdm-toolbar-help"]);
  const page = { id: UNPACKED, frameId: 0, url: "https://tube.test/v", tab: { id: 3, url: "https://tube.test/v" } };
  const fromPage = (m) => new Promise((r) => listeners.message(m, page, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(await fromPage({ type: "panel" }), { on: false });
  assert.deepEqual(await fromPage({ type: "force", url: "https://x.test/a.html" }), { ok: false });
  assert.equal(natives(calls).filter((m) => m.panel || m.url).length, 0, "nothing asked or handed over");
  assert.deepEqual(button(calls), { badge: "!", color: "#9D5D00", title: "status_duplicate\nclick_fix", icon: "icons/icon-off-16.png" });
  const dnr = calls.filter((c) => c[0] === "dnr").at(-1);
  assert.ok(dnr && dnr[1].addRules.length === 0, "its rule for PDFs and music dropped");
  await listeners.toolbar({ id: 1 });
  await settle();
  assert.equal(store.paused, undefined, "never toggled");
  assert.ok(calls.some((c) => c[0] === "openTab" && c[1] === `chrome-extension://${UNPACKED}/install.html`));
  const s = await ownAsk(listeners, { type: "status" }, "install.html", UNPACKED);
  assert.deepEqual([s.duplicate, s.copy, s.look], ["store", "unpacked", "duplicate"]);
  ok("two copies: the unpacked one stands aside, says to remove it, takes nothing");
}

// The store's copy acts as ever: it asks no one, and answers the unpacked copy (only ours: a
// stranger asking hears nothing), with its connection's state.
{
  const { api, calls, listeners } = fakeApi({ firefox: false, id: STORE });
  load(api, false);
  listeners.startup();
  await settle();
  assert.ok(!calls.some((c) => c[0] === "ask-copy" && c[2] === "ddm-copy"), "the preferred copy asks no one");
  const external = (m, id) =>
    new Promise((r) => {
      const async = listeners.external(m, { id }, (v) => r(JSON.parse(JSON.stringify(v))));
      if (!async) r("no answer");
    });
  assert.deepEqual(await external({ type: "ddm-copy" }, UNPACKED), { ddmCopy: true, version: "0.2.5", why: null, reached: true, paused: false });
  assert.equal(await external({ type: "ddm-copy" }, "abcdefghijklmnopabcdefghijklmnop"), "no answer");
  assert.equal(await external({ type: "pause" }, UNPACKED), "no answer", "nothing else, even from ours");
  let suggested = false;
  listeners.determining({ id: 81, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(!suggested && natives(calls).length === 1, "it takes the download");
  // Installed or updated, it tells the copy after it to ask again, before asking DDM anything
  // (that may take half a minute).
  const told = (c) => c.filter((x) => x[0] === "ask-copy" && x[1] === UNPACKED && x[2] === "ddm-copy-changed").length;
  const slow = fakeApi({ firefox: false, id: STORE });
  let answer;
  slow.api.runtime.sendNativeMessage = () => new Promise((r) => (answer = r));
  load(slow.api, false);
  const before = told(slow.calls);
  slow.listeners.installed({ reason: "install" });
  await settle();
  assert.ok(told(slow.calls) > before, "told before DDM answered");
  answer({ ok: true, version: "0.2.5", types: "ZIP", capture: true });
  // And when the browser starts.
  const startup = told(calls);
  listeners.startup();
  await settle();
  assert.ok(told(calls) > startup, "told at the browser's start");
  ok("two copies: the store's acts, and answers ours only; it tells the other at once");
}

// The store's copy turned on again in the browser's extensions page (no install, no browser
// start: only its script starting) tells the other copy too, once per time it is turned on (the
// browser's session storage is emptied then), not at each wake of its script.
{
  const session = {};
  const first = fakeApi({ firefox: false, id: STORE, session });
  load(first.api, false);
  await settle();
  const told = (c) => c.filter((x) => x[0] === "ask-copy" && x[1] === UNPACKED && x[2] === "ddm-copy-changed").length;
  assert.equal(told(first.calls), 1, "turned on: told");
  const woken = fakeApi({ firefox: false, id: STORE, session });
  load(woken.api, false);
  await settle();
  assert.equal(told(woken.calls), 0, "woken again: not told again");
  ok("two copies: the store's copy turned on tells the other at once");
}

// What the store's copy answers is what is true now: with no rules yet, or a reason from before
// (DDM didn't start in time once), it asks DDM first (briefly), so the other copy stands aside only
// when the store's copy really reaches DDM, and never takes a download the store's copy takes too.
{
  const external = (listeners, m, id) => new Promise((r) => listeners.external(m, { id }, (v) => r(JSON.parse(JSON.stringify(v)))));
  // Never asked DDM yet, DDM there: reached.
  const fresh = fakeApi({ firefox: false, id: STORE });
  load(fresh.api, false);
  fresh.store.rules = undefined;
  const a = await external(fresh.listeners, { type: "ddm-copy" }, UNPACKED);
  assert.deepEqual([a.reached, a.why, a.paused], [true, null, false]);
  // DDM not installed (no host): not reached, and why.
  const none = fakeApi({ firefox: false, id: STORE, host: false });
  load(none.api, false);
  const b = await external(none.listeners, { type: "ddm-copy" }, UNPACKED);
  assert.deepEqual([b.reached, b.why], [false, "not-installed"]);
  // A download that DDM didn't start in time a moment ago: asked again, it answers.
  const stale = fakeApi({ firefox: false, id: STORE });
  load(stale.api, false);
  await settle();
  stale.store.status = { why: "not-running" };
  const c = await external(stale.listeners, { type: "ddm-copy" }, UNPACKED);
  assert.deepEqual([c.reached, c.why], [true, null]);
  // Turned off by the user: said.
  const off = fakeApi({ firefox: false, id: STORE });
  load(off.api, false);
  off.store.paused = true;
  assert.equal((await external(off.listeners, { type: "ddm-copy" }, UNPACKED)).paused, true);
  ok("two copies: the store's copy answers what is true now (reached, why, off)");
}

// A worker the browser starts again (its memory empty) for the unpacked copy, with the store's
// copy there: a click asks first, so it never flips this copy's switch (which would come back
// silently OFF once the store's copy is gone); its button is drawn standing aside from the start,
// with no "on" look, and its rule for PDFs never installed. Woken again with the browser's
// session storage kept, it knows at once without asking.
{
  const session = {};
  const rules = { types: "PDF ZIP", capture: true, captureOff: "", excludedSites: "", excludedAddresses: [], opened: true, minKb: 0, keys: { prevent: "alt", force: "ctrl+alt" }, menu: ["link"], store: "", at: Date.now() };
  const cold = fakeApi({ firefox: false, id: UNPACKED, copies: { [STORE]: {} }, copyDelay: 15, session });
  cold.store.rules = rules;
  load(cold.api, false);
  await cold.listeners.toolbar({ id: 1 });
  await settle();
  assert.equal(cold.store.paused, undefined, "a cold click never toggles a copy standing aside");
  assert.ok(!cold.calls.some((c) => c[0] === "icon" && c[1] === "icons/icon-16.png"), "never drawn on");
  assert.ok(!cold.calls.some((c) => c[0] === "badge" && c[1] === ""), "never a moment without its !");
  assert.ok(!cold.calls.some((c) => c[0] === "dnr" && c[1].addRules.length), "its rule for PDFs never installed");
  assert.ok(cold.calls.some((c) => c[0] === "openTab" && c[1].endsWith("/install.html")));
  const woken = fakeApi({ firefox: false, id: UNPACKED, copies: { [STORE]: {} }, copyDelay: 15, session });
  woken.store.rules = rules;
  load(woken.api, false);
  await woken.listeners.toolbar({ id: 1 });
  await settle();
  assert.equal(woken.store.paused, undefined);
  assert.ok(!woken.calls.some((c) => c[0] === "ask-copy" && c[2] === "ddm-copy"), "known from the session: not asked again");
  assert.ok(!woken.calls.some((c) => c[0] === "icon" && c[1] === "icons/icon-16.png"));
  ok("two copies: a cold worker asks first: no toggle, no flicker, no rule");
}

// The unpacked copy stands aside only for a store copy that reaches DDM: one that can't (DDM not
// found, the helper failing, never reached yet) takes nothing, so this one goes on. A store copy
// the user turned off keeps this one off too: off means off, never a silent takeover.
{
  for (const [answer, acts, why] of [
    [{ reached: false, why: "not-installed" }, true, "the store's copy doesn't find DDM"],
    [{ reached: true, why: "host-failed" }, true, "the store's copy's helper fails"],
    [{ reached: false, why: null }, true, "the store's copy never reached DDM"],
    [{ reached: true, why: "update-app" }, false, "the store's copy works (with an older DDM)"],
    [{ reached: true, why: null, paused: true }, false, "the store's copy turned off by the user"],
    [{ reached: false, why: "not-installed", paused: true }, false, "turned off, whatever else"],
  ]) {
    const { api, calls, listeners } = fakeApi({ firefox: false, id: UNPACKED, copies: { [STORE]: answer } });
    load(api, false);
    await settle();
    listeners.determining({ id: 84, url: "https://x/a.zip", filename: "a.zip" }, () => {});
    await settle();
    assert.equal(natives(calls).length, acts ? 1 : 0, why);
  }
  ok("two copies: the unpacked copy stands aside only for a store copy that reaches DDM, or is off");
}

// The unpacked copy goes on taking downloads when the store's copy isn't there, is turned off
// (no answer), or is one this computer's DDM refuses (an app from before the store's ID: only the
// unpacked copy reaches it); and takes them back once the store's copy is gone.
{
  for (const [copies, why] of [
    [{}, "not installed"],
    [{ [STORE]: { why: "forbidden" } }, "refused by an old DDM"],
  ]) {
    const { api, calls, listeners } = fakeApi({ firefox: false, id: UNPACKED, copies });
    load(api, false);
    await settle();
    listeners.determining({ id: 82, url: "https://x/a.zip", filename: "a.zip" }, () => {});
    await settle();
    assert.equal(natives(calls).length, 1, why);
    assert.equal(button(calls).badge, "", why);
  }
  const clock = { t: Date.now() };
  class FakeDate extends Date {
    static now() {
      return clock.t;
    }
  }
  const there = { [STORE]: { why: null } };
  const { api, calls, listeners } = fakeApi({ firefox: false, id: UNPACKED, copies: (id) => there[id] });
  load(api, false, { Date: FakeDate });
  await settle();
  assert.equal(button(calls).badge, "!");
  // The store's copy removed: at the next download (a few seconds on), this one takes it.
  delete there[STORE];
  clock.t += 4000;
  listeners.determining({ id: 83, url: "https://x/b.zip", filename: "b.zip" }, () => {});
  await settle();
  assert.equal(natives(calls).length, 1, "taken at the next download");
  assert.equal(button(calls).icon, "icons/icon-16.png");
  // Installed again: it tells this one, which stands aside at once.
  there[STORE] = { why: null };
  listeners.external({ type: "ddm-copy-changed" }, { id: STORE }, () => {});
  await settle();
  assert.equal(button(calls).badge, "!");
  // Turned off and on again without a word (nothing told): within one download, again.
  delete there[STORE];
  clock.t += 4000;
  listeners.determining({ id: 85, url: "https://x/c.zip", filename: "c.zip" }, () => {});
  await settle();
  assert.equal(natives(calls).length, 2);
  there[STORE] = { why: null };
  clock.t += 4000;
  listeners.determining({ id: 86, url: "https://x/d.zip", filename: "d.zip" }, () => {});
  await settle();
  assert.equal(natives(calls).length, 2, "the store's copy back: this one stands aside within one download");
  // Firefox has one add-on ID wherever it comes from: it never asks.
  const ff = fakeApi({ firefox: true, id: UNPACKED, copies: there });
  load(ff.api, true);
  await settle();
  assert.ok(!ff.calls.some((c) => c[0] === "ask-copy"));
  ok("two copies: the unpacked one acts alone, or with a copy DDM refuses, and takes over when the store's goes");
}

// The Chrome Web Store's listing, once public (DDM's rules carry its page: `store`): the copy
// loaded from DDM's folder, which never updates itself, offers it once and quietly (an item in
// its button's menu and a note on its status and options pages; no badge, nothing opened). Not
// the store's copy, not Firefox, nothing from an older DDM (no `store`) or another address.
{
  const page = "https://chromewebstore.google.com/detail/dannex-download-manager/plmckdlgphkcfkifahbkonbibfedckih";
  const offer = async (options, firefox = false) => {
    const { api, calls, listeners } = fakeApi({ firefox, ...options });
    load(api, firefox);
    await listeners.installed({ reason: "update" });
    await settle();
    const s = await ownAsk(listeners, { type: "status" }, "install.html", options.id || "ours");
    return { items: toolbarItems(calls), store: s.store, badge: button(calls).badge, opened: calls.filter((c) => c[0] === "openTab"), listeners, calls };
  };
  const unpacked = await offer({ id: UNPACKED, rules: { store: page } });
  assert.deepEqual(unpacked.items, [...TOOLBAR_ITEMS, "mdm-toolbar-store"]);
  assert.equal(unpacked.store, page);
  assert.deepEqual([unpacked.badge, unpacked.opened], ["", []], "never a badge, nothing opened");
  await unpacked.listeners.menu({ menuItemId: "mdm-toolbar-store" }, { id: 1 });
  assert.ok(unpacked.calls.some((c) => c[0] === "openTab" && c[1] === page));
  for (const [options, firefox, why] of [
    [{ id: STORE, rules: { store: page } }, false, "the store's copy"],
    [{ id: UNPACKED, rules: { store: page } }, true, "Firefox"],
    [{ id: UNPACKED }, false, "an older DDM, or not listed"],
    [{ id: UNPACKED, rules: { store: "https://evil.test/detail/x/plmckdlgphkcfkifahbkonbibfedckih" } }, false, "another address"],
  ]) {
    const none = await offer(options, firefox);
    assert.deepEqual([none.items.includes("mdm-toolbar-store"), none.store], [false, ""], why);
  }
  ok("the store's listing: offered once, quietly, by the unpacked copy only");
}

// A form's download: posted again by the app with the same body (it can't be a GET).
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  listeners.beforeRequest({ method: "POST", url: "https://x/form", requestId: "9", requestBody: { formData: { token: ["abc"], note: ["hello world"] } } });
  listeners.beforeSendHeaders({ method: "POST", url: "https://x/form", requestId: "9", requestHeaders: [{ name: "Content-Type", value: "application/x-www-form-urlencoded" }] });
  listeners.determining({ id: 40, url: "https://x/form", filename: "invoice.pdf" }, () => {});
  await settle();
  const [m] = natives(calls);
  assert.deepEqual([m.post, m.postType], ["token=abc&note=hello+world", "application/x-www-form-urlencoded"]);
  // A form with a file in it can't be posted again: the browser keeps its download.
  listeners.beforeRequest({ method: "POST", url: "https://x/upload", requestId: "10", requestBody: { raw: [{ file: "C:\\a.txt" }] } });
  let suggested = false;
  listeners.determining({ id: 41, url: "https://x/upload", filename: "out.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested);
  assert.equal(natives(calls).length, 1);
  // A multipart form: the browser gives its fields without their files, so it stays there too.
  listeners.beforeRequest({ method: "POST", url: "https://x/multi", requestId: "11", requestBody: { formData: { a: ["1"] } } });
  listeners.beforeSendHeaders({ method: "POST", url: "https://x/multi", requestId: "11", requestHeaders: [{ name: "Content-Type", value: "multipart/form-data; boundary=xyz" }] });
  let kept = false;
  listeners.determining({ id: 42, url: "https://x/multi", filename: "out.zip" }, () => (kept = true));
  await settle();
  assert.ok(kept);
  assert.equal(natives(calls).length, 1);
  ok("forms: posted again with their body, or left to the browser");
}

// Alt held on a link, a site the extension may not read, an excluded site, a blob: address:
// each stays with the browser without asking the app.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const send = (m, sender) => new Promise((r) => listeners.message(m, sender, r));
  await send({ type: "bypass", url: "https://x/alt.zip" }, { id: "ours", url: "https://x/", tab: { url: "https://x/" } });
  const keep = async (item) => {
    let suggested = false;
    listeners.determining(item, () => (suggested = true));
    await settle();
    return suggested;
  };
  assert.ok(await keep({ id: 50, url: "https://x/alt.zip", filename: "alt.zip" }), "Alt");
  assert.ok(await keep({ id: 51, url: "https://dl.update.microsoft.com/kb.exe", filename: "kb.exe" }), "excluded site");
  assert.ok(await keep({ id: 52, url: "blob:https://x/1234", filename: "a.zip" }), "blob:");
  assert.deepEqual(natives(calls), []);
  // The Alt-click was that download's only: the same link clicked plainly afterwards is DDM's.
  listeners.determining({ id: 53, url: "https://x/alt.zip", filename: "alt.zip" }, () => {});
  await settle();
  assert.ok(natives(calls).some((m) => m.url === "https://x/alt.zip"), "the next click asks DDM");
  // Alt and blob: are the user's and the browser's own: nothing to say. An excluded site is.
  assert.deepEqual(notices(calls), [{ why: "site", name: "kb.exe", site: "dl.update.microsoft.com" }]);
  const limited = fakeApi({ firefox: false, access: false });
  load(limited.api, false);
  let suggested = false;
  limited.listeners.determining({ id: 53, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested && natives(limited.calls).length === 0, "site access withdrawn");
  assert.deepEqual(notices(limited.calls).map((n) => n.why), ["site-access"]);
  ok("Alt, excluded sites, blob: and withdrawn site access stay with the browser");
}

// Turned off from the options page (the same switch as a click on the toolbar button): nothing
// is taken until it is on again.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false });
  load(api, false);
  const send = (m) => new Promise((r) => listeners.message(m, { id: "ours" }, (v) => r(JSON.parse(JSON.stringify(v)))));
  const state = await send({ type: "state" });
  assert.deepEqual([state.connected, state.app, state.paused], [true, "0.2.5", false]);
  await send({ type: "pause", paused: true });
  assert.equal(store.paused, true);
  let suggested = false;
  listeners.determining({ id: 60, url: "https://x/a.zip", filename: "a.zip" }, () => (suggested = true));
  await settle();
  assert.ok(suggested && natives(calls).length === 0);
  assert.ok(calls.some((c) => c[0] === "badge" && c[1] === "badge_off"));
  ok("the options page: state, off");
}

// An app older than the extension: still connected, with a nudge to update it.
{
  const { api, store, listeners } = fakeApi({ firefox: false, rules: { version: "0.1.0" } });
  load(api, false);
  listeners.startup();
  await settle();
  assert.equal(store.status && store.status.why, "update-app");
  assert.equal(store.status.app, "0.1.0");
  ok("an older app: update-app");
}

// Only a bug-fix version apart (the store publishing the add-on first): no nudge, no update exists.
{
  const { api, store, listeners } = fakeApi({ firefox: false, rules: { version: "0.2.4" } });
  load(api, false);
  listeners.startup();
  await settle();
  assert.notEqual(store.status && store.status.why, "update-app");
  ok("a bug-fix version behind: no update-app");
}

// Files the browser would open (a PDF): a session rule makes them downloads, for the listed types
// only; none when the app says not to.
{
  const { api, calls } = fakeApi({ firefox: false, types: "PDF MP3 ZIP" });
  load(api, false);
  await settle();
  const rule = calls.filter((c) => c[0] === "dnr").pop()[1];
  const values = rule.addRules[0].condition.responseHeaders[0].values;
  assert.ok(values.includes("application/pdf*") && values.includes("audio/mpeg*"));
  assert.ok(!values.some((v) => v.startsWith("video/mp4")), "MP4 not listed");
  // Options' excluded sites keep opening in the browser: *. with its subdomains, a plain host
  // by itself (an allow rule).
  const ex = fakeApi({ firefox: false, types: "PDF", rules: { excludedSites: "*.docs.test files.test:8080" } });
  load(ex.api, false);
  await settle();
  const added = ex.calls.filter((c) => c[0] === "dnr").pop()[1].addRules;
  assert.deepEqual(added[0].condition.excludedRequestDomains, ["docs.test"]);
  assert.equal(added[1].action.type, "allow");
  assert.equal(added[1].condition.regexFilter, "^[a-z]+://files\\.test(:[0-9]+)?/", "the host alone, any port");
  // Options' excluded addresses too, as addressMatches reads them (left to the browser, a PDF there
  // was saved instead of shown).
  const addr = fakeApi({ firefox: false, types: "PDF", rules: { excludedAddresses: ["example.test/private/*", "https://a.test/x.pdf"] } });
  load(addr.api, false);
  await settle();
  const allowed = addr.calls
    .filter((c) => c[0] === "dnr")
    .pop()[1]
    .addRules.filter((r) => r.action.type === "allow")
    .map((r) => r.condition.regexFilter);
  assert.deepEqual(allowed, ["^[a-z][a-z0-9+.-]*://example\\.test/private/.*$", "^https://a\\.test/x\\.pdf$"]);
  for (const [url, re] of [
    ["https://example.test/private/doc.pdf", allowed[0]],
    ["https://a.test/x.pdf", allowed[1]],
  ]) {
    assert.ok(new RegExp(re, "i").test(url), url);
  }
  assert.ok(!new RegExp(allowed[0], "i").test("https://example.test/public/doc.pdf"));
  const off = fakeApi({ firefox: false, rules: { opened: false } });
  load(off.api, false);
  await settle();
  assert.deepEqual(off.calls.filter((c) => c[0] === "dnr").pop()[1].addRules, []);
  ok("opened types: the session rule follows the app's list");
}

// Right-click → Download with Dannex Download Manager: handed over as an explicit request, so
// the host takes any link (an .html page here); DDM not reached opens the status page (live),
// anything else the help page's section.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await listeners.menu({ menuItemId: "mdm-link", linkUrl: "https://x/page.html", pageUrl: "https://x/" }, { incognito: false });
  await settle();
  const [m] = natives(calls);
  assert.deepEqual([m.url, m.explicit, m.referrer], ["https://x/page.html", true, "https://x/"]);
  await listeners.menu({ menuItemId: "mdm-all", pageUrl: "https://x/" }, { id: 1, incognito: false });
  await settle();
  assert.deepEqual(natives(calls)[1].links, ["https://x/1.zip", "https://x/2.zip"]);
  // A magnet link: asked for, it goes to DDM (its Add torrent window); a javascript: link never.
  const magnet = "magnet:?xt=urn:btih:08ada5a7a6183aae1e09d831df6748d566095a10&dn=Sintel";
  await listeners.menu({ menuItemId: "mdm-link", linkUrl: magnet, pageUrl: "https://x/" }, { incognito: false });
  await listeners.menu({ menuItemId: "mdm-link", linkUrl: "javascript:void(0)", pageUrl: "https://x/" }, { incognito: false });
  await settle();
  assert.equal(natives(calls).length, 3, "the magnet went, the javascript: link didn't");
  assert.deepEqual([natives(calls)[2].url, natives(calls)[2].explicit], [magnet, true]);
  const failing = fakeApi({ firefox: false, reply: { ok: false, why: "not-running" } });
  load(failing.api, false);
  await failing.listeners.menu({ menuItemId: "mdm-link", linkUrl: "https://x/a.zip", pageUrl: "https://x/" }, {});
  await settle();
  assert.ok(failing.calls.some((c) => c[0] === "openTab" && c[1] === "chrome-extension://ours/install.html"));
  const large = fakeApi({ firefox: false, reply: { ok: false, why: "too-large" } });
  load(large.api, false);
  await large.listeners.menu({ menuItemId: "mdm-link", linkUrl: "https://x/a.zip", pageUrl: "https://x/" }, {});
  await settle();
  assert.ok(large.calls.some((c) => c[0] === "openTab" && c[1].endsWith("browser-help.html#too-large")));
  ok("context menus: explicit links, a page's links, failures explained");
}

// What the background answers "may it show here" with when the app says yes and Download panels
// is as installed (over videos and sound players, any length, top right first).
const ON = { on: true, video: true, audio: true, minSeconds: 0, corner: "top-right" };

// Another DDM chosen (#157): the button's question about a page, and a video, are never added
// there as a file (its remote control adds files by address: the page itself was added each time a
// page with a video opened); they go to this computer's DDM.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  store.remote = { url: "https://server.test:8463/", token: "k" };
  remoteAsked.length = 0;
  const ask = (m, page = "https://tube.test/embed/9") =>
    new Promise((resolve) => {
      const async_ = listeners.message(m, { id: "ours", url: page, tab: { url: "https://blog.test/post", incognito: false } }, (r) =>
        resolve(JSON.parse(JSON.stringify(r))),
      );
      if (!async_) resolve(undefined);
    });
  await ask({ type: "panel" });
  await ask({ type: "video" });
  assert.deepEqual(
    remoteAsked.filter((r) => r.url.endsWith("api/add")),
    [],
    "nothing added on the other DDM",
  );
  assert.ok(natives(calls).some((m) => m.panel), "the question went to this computer's DDM");
  ok("another DDM: the video button's question and a video stay this computer's");
}

// The download button over videos: the page asks once a minute per site, a click opens the
// app's video window for the page the browser says sent it (with the tab as referrer and its
// cookies), "Never on this site" goes to the app and is remembered at once; nothing from
// another extension is answered.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const ask = (m, page = "https://tube.test/embed/1", sender) =>
    new Promise((resolve) => {
      // Plain copies: objects from the sandbox have its prototypes.
      const async_ = listeners.message(m, sender || { id: "ours", url: page, tab: { url: "https://blog.test/post", incognito: false } }, (r) =>
        resolve(JSON.parse(JSON.stringify(r))),
      );
      if (!async_) resolve(undefined);
    });
  assert.deepEqual(await ask({ type: "panel" }), ON);
  assert.deepEqual(await ask({ type: "panel" }, "https://tube.test/embed/2"), ON);
  assert.equal(natives(calls).filter((m) => m.panel).length, 1, "asked once for the site");
  assert.equal((await ask({ type: "video", page: "https://evil.test/" })).ok, true);
  const video = natives(calls).find((m) => m.video);
  assert.equal(video.url, "https://tube.test/embed/1", "the page the browser names, not the message's");
  assert.equal(video.referrer, "https://blog.test/post");
  assert.equal(video.cookies, "sid=1");
  // Each cookie with its domain and flags, for yt-dlp asking another host of the site.
  assert.deepEqual(video.cookieJar, [{ name: "sid", value: "1", domain: ".tube.test", hostOnly: false, path: "/", secure: true, expirationDate: 1893456000.5 }]);
  assert.deepEqual(await ask({ type: "hide-site", host: "elsewhere.test" }), { ok: true });
  assert.ok(natives(calls).some((m) => m.hideSite === "tube.test"));
  assert.deepEqual(await ask({ type: "panel" }, "https://tube.test/embed/3"), { on: false }, "off at once");
  assert.deepEqual(await ask({ type: "panel" }, "https://tube.test:8443/embed/4"), { on: false }, "whatever the port");
  assert.deepEqual(await ask({ type: "video" }, "javascript:alert(1)"), { ok: false });
  // The video's own address, when the content script names one of its site's: a watch page opened
  // from a mix or a playlist is the list in the address bar, but the video the button is on is one
  // video. The tab stays the referrer, and its cookies are read for that address.
  assert.equal((await ask({ type: "video", url: "https://tube.test/watch?v=abc123" })).ok, true);
  const own = natives(calls).filter((m) => m.video).at(-1);
  assert.equal(own.url, "https://tube.test/watch?v=abc123", "the video's own page");
  assert.equal(own.referrer, "https://blog.test/post");
  assert.equal(own.cookies, "sid=1");
  // Another site's address is not the page's: ignored, as any message content was before.
  assert.equal((await ask({ type: "video", url: "https://evil.test/watch?v=abc123" })).ok, true);
  assert.equal(
    natives(calls).filter((m) => m.video).at(-1).url,
    "https://tube.test/embed/1",
    "another site's address is ignored",
  );
  assert.equal(await ask({ type: "panel" }, "https://x.test/", { id: "someone-else", url: "https://x.test/", tab: {} }), undefined);
}
// A player playing a stream (HLS) from a blob: the button sends the manifest its frame loaded
// first; the tab going elsewhere forgets it.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const sender = { id: "ours", url: "https://news.test/story", frameId: 0, tab: { id: 5, url: "https://news.test/story" } };
  listeners.responseStarted({ tabId: 5, frameId: 0, url: "https://cdn.test/v/master.m3u8?t=1", responseHeaders: [] });
  listeners.responseStarted({ tabId: 5, frameId: 0, url: "https://cdn.test/v/720p.m3u8", responseHeaders: [] });
  listeners.responseStarted({ tabId: 5, frameId: 0, url: "https://cdn.test/v/seg1.ts", responseHeaders: [{ name: "Content-Type", value: "video/mp2t" }] });
  const send = (m) => new Promise((r) => listeners.message(m, sender, r));
  await send({ type: "video", src: "", title: "A story's film" });
  const first = natives(calls).find((m) => m.video);
  assert.deepEqual([first.src, first.title], ["https://cdn.test/v/master.m3u8?t=1", "A story's film"], "the page's title goes with its stream");
  listeners.tabUpdated(5, { url: "https://news.test/other" });
  await send({ type: "video", src: "" });
  assert.equal(natives(calls).filter((m) => m.video)[1].src, undefined, "forgotten on another page");
  ok("streams: the frame's manifest goes with the button");
}
// #155: the right-click menu on a player playing from a blob: sends the manifest its frame loaded
// (it sent the page alone, which yt-dlp answered "Unsupported URL"), with the manifest's cookies;
// a manifest's own link goes with its cookies too; the button asks for the stream's cookies.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  listeners.responseStarted({ tabId: 7, frameId: 2, url: "https://cdn.test/s/master.m3u8", responseHeaders: [] });
  listeners.responseStarted({ tabId: 7, frameId: 0, url: "https://other.test/x/main.mpd", responseHeaders: [] });
  await listeners.menu(
    { menuItemId: "mdm-link", srcUrl: "blob:https://player.test/1", frameId: 2, frameUrl: "https://player.test/embed", pageUrl: "https://blog.test/post" },
    { id: 7, incognito: false, title: "A post with a film" },
  );
  await settle();
  const m = natives(calls).find((x) => x.video);
  assert.deepEqual(
    [m.url, m.src, m.referrer, m.explicit, m.title],
    ["https://player.test/embed", "https://cdn.test/s/master.m3u8", "https://blog.test/post", true, "A post with a film"],
    "the frame's stream, not the page alone; the tab's title names its file",
  );
  assert.ok(calls.some((c) => c[0] === "cookies" && c[1].url === "https://cdn.test/s/master.m3u8"), "the stream's cookies");
  await listeners.menu({ menuItemId: "mdm-link", linkUrl: "https://cdn.test/s/other.m3u8", pageUrl: "https://blog.test/post" }, { id: 7, incognito: false, title: "A post with a film" });
  await settle();
  const linked = natives(calls).filter((x) => x.video)[1];
  assert.deepEqual([linked.src, linked.title], ["https://cdn.test/s/other.m3u8", "A post with a film"], "a manifest's link: named after its tab");
  assert.ok(calls.some((c) => c[0] === "cookies" && c[1].url === "https://cdn.test/s/other.m3u8"), "the manifest's cookies");
  // Firefox names no address for a player fed by a MediaSource: the same stream goes.
  await listeners.menu({ menuItemId: "mdm-link", mediaType: "video", frameId: 2, frameUrl: "https://player.test/embed", pageUrl: "https://blog.test/post" }, { id: 7, incognito: false });
  await settle();
  assert.equal(natives(calls).filter((x) => x.video).pop().src, "https://cdn.test/s/master.m3u8", "Firefox: no srcUrl");
  // Nothing loaded in that tab: the page, as before (a page yt-dlp may know).
  await listeners.menu({ menuItemId: "mdm-link", srcUrl: "blob:https://tube.test/2", frameId: 0, pageUrl: "https://tube.test/watch" }, { id: 9, incognito: false });
  await settle();
  const page = natives(calls).filter((x) => x.video).pop();
  assert.deepEqual([page.url, page.src], ["https://tube.test/watch", undefined]);
  // The button: the stream's cookies, not the page's.
  const sender = { id: "ours", url: "https://blog.test/post", frameId: 0, tab: { id: 7, url: "https://blog.test/post" } };
  calls.length = 0;
  await new Promise((r) => listeners.message({ type: "video", src: "" }, sender, r));
  assert.ok(calls.some((c) => c[0] === "cookies" && c[1].url === "https://other.test/x/main.mpd"), "the button: the stream's cookies");
  ok("#155: the right-click menu on a stream's player sends its manifest; cookies of the stream");
}
// #532: Options changed while a page with a video is open: the app's answer is kept a few
// seconds, not a minute, so the button follows (the page asks again: video.js ASK_MS). An
// embedded player asks with its tab's address, so the site in the address bar counts too.
{
  const clock = { t: Date.now() };
  class FakeDate extends Date {
    static now() {
      return clock.t;
    }
  }
  let appSays = true;
  const { api, calls, listeners } = fakeApi({ firefox: false, panelOn: () => appSays });
  load(api, false, { Date: FakeDate });
  const embed = { id: "ours", url: "https://tube.test/embed/1", frameId: 3, tab: { url: "https://blog.test/post" } };
  const top = { id: "ours", url: "https://tube.test/watch", frameId: 0, tab: { url: "https://tube.test/watch" } };
  const ask = (sender) =>
    new Promise((r) => listeners.message({ type: "panel" }, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(await ask(embed), ON);
  const asked = () => natives(calls).filter((m) => m.panel);
  assert.equal(asked()[0].referrer, "https://blog.test/post", "the tab's address with an embedded player's question");
  assert.deepEqual(await ask(top), ON);
  assert.equal(asked()[1].referrer, undefined, "none for the tab's own page");
  appSays = false;
  clock.t += 1000;
  assert.deepEqual(await ask(top), ON, "kept a moment (the page's frames ask together)");
  assert.equal(asked().length, 2);
  clock.t += 5000;
  assert.deepEqual(await ask(top), { on: false }, "switched off in Options: the next question has it");
  assert.deepEqual(await ask(embed), { on: false });
  appSays = true;
  clock.t += 6000;
  assert.deepEqual(await ask(embed), ON, "and on again");
  // "Never on this site" from the embed: off at once, in its tab and in any other.
  await new Promise((r) => listeners.message({ type: "hide-site" }, embed, r));
  assert.deepEqual(await ask(embed), { on: false });
  assert.deepEqual(await ask({ ...embed, tab: { url: "https://news.test/" } }), { on: false }, "whatever the tab");
  ok("#532: the video button follows Options within seconds; an embed asks with its tab's address");
}
{
  const { api, listeners } = fakeApi({ firefox: true, panelOn: false });
  load(api, true);
  const on = await new Promise((r) =>
    listeners.message({ type: "panel" }, { id: "ours", url: "https://x.test/", tab: {} }, (v) => r(JSON.parse(JSON.stringify(v)))),
  );
  assert.deepEqual(on, { on: false }, "where the app says no");
  ok("video button messages");
}

// The button's qualities: an app whose rules say it looks videos up (`videoLookup`, kept with the
// status) gets the click as a lookup, never with `video` or a top-level `url` (an older host
// refuses such a message and does nothing), with the cookies of what yt-dlp is given; its answer
// goes back to the page as `lookup`. Without that (an older app, another DDM chosen, an answer
// that isn't a lookup's) the page gets {fallback: true}: the click as it always was.
{
  const ask = (listeners, m, sender) => new Promise((r) => listeners.message(m, sender, (x) => r(JSON.parse(JSON.stringify(x)))));
  const sender = { id: "ours", url: "https://news.test/story", frameId: 0, tab: { id: 5, url: "https://news.test/story", incognito: false, cookieStoreId: "firefox-container-2" } };
  const lookups = (calls) => natives(calls).filter((m) => m.videoLookup || m.videoPoll || m.videoPick);
  // The lookup: the page names the video, the background what the browser knows.
  {
    const { api, calls, listeners, store } = fakeApi({ firefox: true, rules: { videoLookup: true }, lookup: () => ({ ok: true, lookup: { state: "pending", token: "t1" } }) });
    load(api, true);
    await settle();
    assert.equal(store.status.videoLookup, true, "kept from the answer to rules");
    const r = await ask(listeners, { type: "video-lookup", src: "https://cdn.test/v/clip.mp4", title: "A story's film", url: "https://news.test/story", at: { x: 1, y: 1 } }, sender);
    assert.deepEqual(r, { lookup: { state: "pending", token: "t1" } });
    const m = natives(calls).find((x) => x.videoLookup);
    assert.deepEqual([m.video, m.url], [undefined, undefined], "no `video`, no top-level `url`");
    assert.deepEqual(m.videoLookup, { page: "https://news.test/story", src: "https://cdn.test/v/clip.mp4", title: "A story's film", storeId: "firefox-container-2" });
    assert.deepEqual([m.cookies, m.private, m.userAgent, m.v], ["sid=1", false, "UA", "0.2.5"]);
    assert.equal(m.cookieJar.length, 1, "each cookie with its domain");
    assert.deepEqual(calls.filter((c) => c[0] === "cookies").at(-1)[1], { url: "https://cdn.test/v/clip.mp4", storeId: "firefox-container-2" }, "the file's cookies, from the tab's container");
    // A player's page in a frame, in a private window: that page's cookies, from the private
    // store, and the tab as its referrer.
    const framed = { id: "ours", url: "https://player.test/embed/1", frameId: 3, tab: { id: 6, url: "https://blog.test/post", incognito: true } };
    await ask(listeners, { type: "video-lookup", src: "blob:https://player.test/x", title: "" }, framed);
    const p = natives(calls).filter((x) => x.videoLookup).at(-1);
    assert.deepEqual([p.videoLookup, p.private], [{ page: "https://player.test/embed/1", referrer: "https://blog.test/post" }, true]);
    assert.deepEqual(calls.filter((c) => c[0] === "cookies").at(-1)[1], { url: "https://player.test/embed/1", storeId: "1" }, "the page's cookies, from the private store");
    assert.ok(!natives(calls).some((x) => x.video), "never the window's message");
  }
  // An app whose rules say nothing of it (an older one): the click as it was, no lookup sent.
  {
    const { api, calls, listeners, store } = fakeApi({ firefox: false });
    load(api, false);
    await settle();
    assert.ok(!store.status.videoLookup, "an older app's rules");
    assert.deepEqual(await ask(listeners, { type: "video-lookup", src: "https://cdn.test/v/clip.mp4" }, sender), { fallback: true });
    assert.deepEqual(lookups(calls), []);
  }
  // Another DDM chosen: none of the three goes to the host, nor to the other DDM.
  {
    const { api, calls, listeners, store } = fakeApi({ firefox: false, rules: { videoLookup: true }, lookup: () => ({ ok: true, lookup: { state: "pending", token: "t1" } }) });
    load(api, false);
    await settle();
    store.remote = { url: "https://server.test:8463/", token: "k" };
    fetched.length = 0;
    for (const m of [{ type: "video-lookup", src: "https://cdn.test/v/clip.mp4" }, { type: "video-poll", token: "t1" }, { type: "video-pick", token: "t1", choice: 0 }]) {
      assert.deepEqual(await ask(listeners, m, sender), { fallback: true }, m.type);
    }
    assert.deepEqual(lookups(calls), []);
    assert.deepEqual(fetched, [], "nothing to the other DDM");
  }
  // The ability kept from before, an older host or app now (DDM went back a version): its answer
  // has no `lookup`. Not kept as able, its rules asked again, the click as it was; the next click
  // sends no lookup at all. A refused message is no broken connection.
  {
    const { api, calls, listeners, store } = fakeApi({ firefox: false, reply: { ok: false, why: "refused", error: "not an http(s) or ftp(s) URL" } });
    store.status = { why: null, app: "0.2.5", videoLookup: true };
    store.rules = { types: DEFAULT_TYPES, capture: true, captureOff: "", excludedSites: "", excludedAddresses: [], opened: true, minKb: 0, at: Date.now() };
    load(api, false);
    await settle();
    const asked = () => calls.filter((c) => c[0] === "rules").length;
    const before = asked();
    assert.deepEqual(await ask(listeners, { type: "video-lookup", src: "https://cdn.test/v/clip.mp4" }, sender), { fallback: true });
    await settle();
    assert.equal(store.status.videoLookup, false, "not kept as able");
    assert.ok(asked() > before, "its rules asked again");
    assert.deepEqual(await ask(listeners, { type: "video-lookup", src: "https://cdn.test/v/clip.mp4" }, sender), { fallback: true });
    assert.equal(natives(calls).filter((x) => x.videoLookup).length, 1, "one lookup, then none");
    assert.ok(!calls.some((c) => c[0] === "badge" && c[1] === "!"), "no \"!\"");
  }
  // How the lookup is going, and the quality picked: the token and the row's number from the
  // page; private or not and the container from the tab, whatever the page says.
  {
    const ready = { state: "ready", token: "t1", title: "Film", choices: [{ label: "720p MP4", ext: "mp4", size: 187500000 }] };
    const answers = { videoPoll: { ok: true, lookup: ready }, videoPick: { ok: true, lookup: { state: "started" } } };
    const { api, calls, listeners } = fakeApi({ firefox: true, rules: { videoLookup: true }, lookup: (m) => answers[m.videoPoll ? "videoPoll" : "videoPick"] });
    load(api, true);
    await settle();
    assert.deepEqual(await ask(listeners, { type: "video-poll", token: "t1" }, sender), { lookup: ready });
    assert.deepEqual(natives(calls).find((x) => x.videoPoll).videoPoll, { token: "t1" });
    const privateTab = { id: 8, url: "https://news.test/story", incognito: true, cookieStoreId: "firefox-private" };
    const r = await ask(listeners, { type: "video-pick", token: "t1", choice: 0, private: false, storeId: "firefox-default" }, { id: "ours", url: "https://news.test/story", frameId: 0, tab: privateTab });
    assert.deepEqual(r, { lookup: { state: "started" } });
    const picked = natives(calls).find((x) => x.videoPick);
    assert.deepEqual(
      [picked.videoPick, picked.private, picked.url, picked.video],
      [{ token: "t1", choice: 0, storeId: "firefox-private" }, true, undefined, undefined],
      "the tab's window and container, not the page's say",
    );
    // An answer without `lookup`: the lookup is gone; a pick gets the window.
    answers.videoPoll = { ok: true };
    answers.videoPick = { ok: true };
    assert.deepEqual(await ask(listeners, { type: "video-poll", token: "t1" }, sender), { lookup: { state: "gone" } });
    assert.deepEqual(await ask(listeners, { type: "video-pick", token: "t1", choice: 0 }, sender), { fallback: true });
    // Nothing but a token and a row's number is sent on.
    const sent = natives(calls).length;
    assert.deepEqual(await ask(listeners, { type: "video-poll", token: { x: 1 } }, sender), { lookup: { state: "gone" } });
    assert.deepEqual(await ask(listeners, { type: "video-pick", token: "t1", choice: "0" }, sender), { fallback: true });
    assert.deepEqual(await ask(listeners, { type: "video-pick", token: "t1", choice: -1 }, sender), { fallback: true });
    assert.equal(natives(calls).length, sent);
  }
  // What a lookup comes to is never the connection's status: no "!" on the toolbar button, the
  // rule for PDFs and music untouched, still able.
  {
    const states = [
      { state: "failed", reason: "error", message: "ERROR: Unsupported URL" },
      { state: "failed", reason: "no-ytdlp" },
      { state: "busy" },
      { state: "off" },
      { state: "list" },
      { state: "use-window" },
      { state: "gone" },
    ];
    let next = 0;
    const { api, calls, listeners, store } = fakeApi({ firefox: false, rules: { videoLookup: true }, lookup: () => ({ ok: true, lookup: states[next++] }) });
    load(api, false);
    await settle();
    const rules = calls.filter((c) => c[0] === "dnr").length;
    for (let i = 0; i < states.length; i++) {
      const m = i % 2 ? { type: "video-poll", token: "t1" } : { type: "video-lookup", src: "https://cdn.test/v/clip.mp4" };
      assert.deepEqual((await ask(listeners, m, sender)).lookup, states[i]);
    }
    await settle();
    assert.equal(store.status.why, null);
    assert.ok(!calls.some((c) => c[0] === "badge" && c[1] === "!"), "no \"!\"");
    assert.equal(calls.filter((c) => c[0] === "dnr").length, rules, "the rule for PDFs and music untouched");
    assert.equal(store.status.videoLookup, true, "still able");
  }
  // The page's own word on the qualities goes back with the lookup's first answer (`page`): on
  // YouTube what its player says (playingAt, asked for the formats too, in the same call that
  // names the video), else the stream the tab loaded; nothing when it says nothing usable.
  {
    const player = JSON.parse(readFileSync(new URL("fixtures/youtube-player.json", import.meta.url), "utf8"));
    const pending = { state: "pending", token: "t1", merge: true };
    const yt = { id: "ours", url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ", frameId: 0, tab: { id: 7, url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ", incognito: false } };
    const { api, calls, listeners } = fakeApi({ firefox: false, rules: { videoLookup: true }, playing: { id: "aqz-KE-bpKQ", page: player }, lookup: () => ({ ok: true, lookup: pending }) });
    const asked = [];
    const exec = api.scripting.executeScript;
    api.scripting.executeScript = async (q) => {
      if (q.func && q.func.name === "playingAt") asked.push(JSON.parse(JSON.stringify(q.args)));
      return exec(q);
    };
    load(api, false);
    await settle();
    const r = await ask(listeners, { type: "video-lookup", url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ", title: "Big Buck Bunny", at: { x: 10, y: 10 } }, yt);
    assert.deepEqual(r.lookup, pending);
    assert.equal(r.page.duration, 635);
    assert.equal(r.page.formats.length, 41);
    assert.deepEqual(asked, [[10, 10, true]], "one call: the video and its formats");
    assert.equal(natives(calls).find((x) => x.videoLookup).videoLookup.page, "https://www.youtube.com/watch?v=aqz-KE-bpKQ");
    // The window's click asks the video alone.
    await ask(listeners, { type: "video", url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ", at: { x: 1, y: 2 } }, yt);
    assert.deepEqual(asked.at(-1), [1, 2, false]);
  }
  {
    // Another video's word (the player moved on), or none: no `page`.
    const { api, listeners } = fakeApi({ firefox: false, rules: { videoLookup: true }, playing: { id: "aqz-KE-bpKQ", page: { id: "other", status: "OK", formats: [] } }, lookup: () => ({ ok: true, lookup: { state: "pending", token: "t1" } }) });
    load(api, false);
    await settle();
    const yt = { id: "ours", url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ", frameId: 0, tab: { id: 7, url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ" } };
    const r = await ask(listeners, { type: "video-lookup", at: { x: 1, y: 1 } }, yt);
    assert.deepEqual(r, { lookup: { state: "pending", token: "t1" } });
  }
  {
    // A stream the tab loaded: its master playlist read again (as the tab's player loaded it), its
    // variants sized from the player's length.
    const src = "https://cdn.test/v/stream/master.m3u8";
    served[src] = { body: "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720\n720.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n360.m3u8\n" };
    const { api, listeners } = fakeApi({ firefox: false, rules: { videoLookup: true }, lookup: () => ({ ok: true, lookup: { state: "pending", token: "t1", merge: false } }) });
    load(api, false);
    await settle();
    const r = await ask(listeners, { type: "video-lookup", src, title: "A film", url: "https://news.test/story", duration: 600, at: { x: 1, y: 1 } }, sender);
    assert.deepEqual(r.page.formats.map((f) => [f.height, f.size]), [[720, 187500000], [360, 60000000]]);
    // A file, not a stream: nothing read.
    const before = fetched.length;
    const f = await ask(listeners, { type: "video-lookup", src: "https://cdn.test/v/clip.mp4", url: "https://news.test/story", at: { x: 1, y: 1 } }, sender);
    assert.equal(f.page, undefined);
    assert.equal(fetched.length, before);
    delete served[src];
  }
  // Asked ahead (`prefetch`): said so to the app, never from a private window; one at a time per
  // tab, the one before called off for another video; called off when the tab goes to another page
  // or closes, and when video.js says so; a lookup a click follows (asked how it goes) is the
  // click's and never called off for the tab.
  {
    let n = 0;
    const { api, calls, listeners } = fakeApi({ firefox: false, rules: { videoLookup: true }, lookup: (m) => (m.videoLookup ? { ok: true, lookup: { state: "pending", token: `t${++n}` } } : { ok: true, lookup: { state: "gone" } }) });
    load(api, false);
    await settle();
    const tab = { id: 5, url: "https://news.test/story", incognito: false };
    const from = { id: "ours", url: tab.url, frameId: 0, tab };
    const cancels = () => natives(calls).filter((x) => x.videoCancel).map((x) => x.videoCancel.token);
    const r = await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/a.mp4", url: tab.url, at: { x: 1, y: 1 } }, from);
    assert.equal(r.lookup.token, "t1");
    assert.equal(natives(calls).filter((x) => x.videoLookup).at(-1).videoLookup.prefetch, true);
    // The same video again: no call off. Another: the first called off.
    await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/a.mp4", url: tab.url, at: { x: 1, y: 1 } }, from);
    assert.deepEqual(cancels(), []);
    await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/b.mp4", url: tab.url, at: { x: 1, y: 1 } }, from);
    assert.deepEqual(cancels(), ["t2"], "the one before, for another video");
    // A fragment of the same page: kept; another page: called off.
    listeners.tabUpdated(5, { url: "https://news.test/story#comments" }, tab);
    await settle();
    assert.deepEqual(cancels(), ["t2"]);
    listeners.tabUpdated(5, { url: "https://news.test/other" }, tab);
    await settle();
    assert.deepEqual(cancels(), ["t2", "t3"]);
    // The tab closed.
    await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/c.mp4", url: tab.url, at: { x: 1, y: 1 } }, from);
    listeners.tabRemoved(5);
    await settle();
    assert.deepEqual(cancels(), ["t2", "t3", "t4"]);
    // A click follows it: the click's.
    await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/d.mp4", url: tab.url, at: { x: 1, y: 1 } }, from);
    await ask(listeners, { type: "video-poll", token: "t5" }, from);
    listeners.tabUpdated(5, { url: "https://news.test/other" }, tab);
    await settle();
    assert.deepEqual(cancels(), ["t2", "t3", "t4"], "never the click's");
    // video.js calls one off: the token alone goes.
    assert.deepEqual(await ask(listeners, { type: "video-cancel", token: "t9" }, from), { ok: true });
    assert.deepEqual(natives(calls).filter((x) => x.videoCancel).at(-1), { videoCancel: { token: "t9" }, userAgent: "UA", brands: natives(calls).at(-1).brands, v: "0.2.5" });
    const sent = natives(calls).length;
    await ask(listeners, { type: "video-cancel", token: { x: 1 } }, from);
    assert.equal(natives(calls).length, sent, "no token: nothing sent");
    // A private window: never asked ahead.
    const priv = { id: "ours", url: tab.url, frameId: 0, tab: { ...tab, id: 6, incognito: true } };
    assert.deepEqual(await ask(listeners, { type: "video-lookup", prefetch: true, src: "https://cdn.test/v/a.mp4", url: tab.url }, priv), { skipped: true });
    assert.equal(natives(calls).length, sent);
  }
  ok("video button: qualities looked up by the app, picked in the page; else the window, as before");
}

// #38: the media a tab loaded. A response is media by its type or its address; segments, byte
// ranges and small pieces a player fetched are parts of a stream. What is kept goes two ways: the
// stream a frame's player loaded first, with the download button over the video (and the
// right-click menu on it); the files the tab played, with "Download all videos and audio with DDM".
const H = (type, more = {}) => [{ name: "Content-Type", value: type }, ...Object.entries(more).map(([name, value]) => ({ name, value }))];
const res = (url, type, headers = [], more = {}) => ({ tabId: 9, frameId: 0, url, type, statusCode: 200, responseHeaders: headers, ...more });
// The stream the button over the video in frame `frameId` of tab `tabId` (at `page`) sends, or
// undefined (none: the page goes alone).
const streamSent = async (listeners, calls, tabId, frameId, page) => {
  const sender = { id: "ours", url: page, frameId, tab: { id: tabId, url: page } };
  const before = natives(calls).length;
  await new Promise((r) => listeners.message({ type: "video", src: "" }, sender, r));
  const sent = natives(calls).slice(before).find((m) => m.video);
  assert.ok(sent, "the button's click went to DDM");
  return sent.src;
};
// The files the tab played, as "Download all videos and audio with DDM" sends them (the page's own
// elements and links name none here): [] when nothing is sent.
const playedSent = async (listeners, calls, tabId) => {
  const before = natives(calls).length;
  await listeners.menu({ menuItemId: "mdm-media", pageUrl: "https://watch.test/", frameId: 0 }, { id: tabId, incognito: false });
  await settle();
  const sent = natives(calls).slice(before).find((m) => Array.isArray(m.links));
  return sent ? sent.links : [];
};
const MPD = (protect) => `<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT1M30S">
<Period><AdaptationSet mimeType="video/mp4">${protect ? '<ContentProtection schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"/>' : ""}
<Representation id="1080p" bandwidth="4000000" width="1920" height="1080"/>
<Representation id="480p" bandwidth="900000" width="854" height="480"/>
</AdaptationSet></Period></MPD>`;
{
  served = {};
  const { api, calls, listeners } = fakeApi({ firefox: false, pageMediaResult: [] });
  load(api, false);
  listeners.tabUpdated(9, { url: "https://watch.test/v/1" });
  await settle();
  fetched.length = 0;
  const seen = [
    // HLS: the master, then its variants (with a query) and segments.
    res("https://cdn.test/v/master.m3u8", "xmlhttprequest", H("application/vnd.apple.mpegurl")),
    res("https://cdn.test/v/720p/index.m3u8?tok=1", "xmlhttprequest", H("application/vnd.apple.mpegurl")),
    res("https://cdn.test/v/audio/index.m3u8", "xmlhttprequest", H("audio/mpegurl")),
    res("https://cdn.test/v/720p/seg0.ts", "xmlhttprequest", H("video/mp2t")),
    // DASH, its init segment (video/mp4, in its folder) and fragments.
    res("https://cdn.test/d/manifest.mpd", "xmlhttprequest", H("application/dash+xml")),
    res("https://cdn.test/d/1080p/init.mp4", "xmlhttprequest", H("video/mp4", { "Content-Length": "4000000" })),
    res("https://cdn.test/d/1080p/seg1.m4s", "xmlhttprequest", H("video/iso.segment")),
    // An embedded player's stream, in a frame of its own.
    res("https://locked.test/l/master.m3u8", "xmlhttprequest", H("application/x-mpegURL"), { frameId: 3 }),
    res("https://locked.test/l/hd/index.m3u8", "xmlhttprequest", H("application/x-mpegURL"), { frameId: 3 }),
    // A <video>'s own file in ranges (its size from Content-Range), asked twice.
    res("https://files.test/movie.mp4", "media", H("video/mp4", { "Content-Range": "bytes 0-1023/52428800" }), { statusCode: 206 }),
    res("https://files.test/movie.mp4", "media", H("video/mp4", { "Content-Range": "bytes 1024-2047/52428800" }), { statusCode: 206 }),
    // An <audio>'s file sent as octet-stream, known by its extension.
    res("https://files.test/song.mp3?x=1", "media", H("application/octet-stream", { "Content-Length": "3000000" })),
    // A player's fetch: a whole large file is kept; a range, a small piece, a 404 are not.
    res("https://files.test/whole.webm", "xmlhttprequest", H("video/webm", { "Content-Length": "9000000" })),
    res("https://files.test/part.mp4", "xmlhttprequest", H("video/mp4", { "Content-Range": "bytes 0-99/900000000" }), { statusCode: 206 }),
    res("https://yt.test/videoplayback?id=1&range=0-50000", "xmlhttprequest", H("video/mp4", { "Content-Length": "5000000" })),
    res("https://files.test/tiny.mp4", "xmlhttprequest", H("video/mp4", { "Content-Length": "40000" })),
    res("https://files.test/gone.mp4", "media", H("video/mp4"), { statusCode: 404 }),
    // Not media; not the tab's (the extension's own fetch); not a web address.
    res("https://files.test/page.html", "xmlhttprequest", H("text/html", { "Content-Length": "9000000" })),
    res("https://files.test/other.mp4", "media", H("video/mp4"), { tabId: -1 }),
    res("blob:https://files.test/1", "media", H("video/mp4")),
  ];
  for (const d of seen) listeners.responseStarted(d);
  await settle();
  assert.deepEqual(
    await playedSent(listeners, calls, 9),
    ["https://files.test/movie.mp4", "https://files.test/song.mp3?x=1", "https://files.test/whole.webm"],
    "the files the tab played, each once: no stream's manifest, segment or piece",
  );
  assert.equal(await streamSent(listeners, calls, 9, 0, "https://watch.test/v/1"), "https://cdn.test/v/master.m3u8", "the frame's first stream, its master");
  assert.equal(await streamSent(listeners, calls, 9, 3, "https://player.test/embed/7"), "https://locked.test/l/master.m3u8", "an embedded player's own");
  assert.equal(fetched.length, 0, "streams known by their type: nothing read");
  ok("media a tab loaded: the button's stream, the files it played");

  // A move within the page keeps them; another page clears them; so does closing the tab.
  listeners.tabUpdated(9, { url: "https://watch.test/v/1#t=30" });
  await settle();
  assert.equal((await playedSent(listeners, calls, 9)).length, 3);
  listeners.tabUpdated(9, { url: "https://watch.test/v/2" });
  await settle();
  assert.deepEqual(await playedSent(listeners, calls, 9), []);
  assert.equal(await streamSent(listeners, calls, 9, 0, "https://watch.test/v/2"), undefined, "the last page's stream is forgotten");
  listeners.responseStarted(res("https://files.test/movie.mp4", "media", H("video/mp4")));
  await settle();
  listeners.tabRemoved(9);
  await settle();
  assert.deepEqual(await playedSent(listeners, calls, 9), []);
  ok("media a tab loaded: cleared on another page, kept within one");
}
{
  // Bounded: a page loading file after file keeps at most 60 per tab, its first stream (the one
  // the button sends) included; at most 50 tabs.
  served = {};
  const { api, calls, listeners } = fakeApi({ firefox: true, pageMediaResult: [] });
  load(api, true);
  for (let tab = 100; tab < 160; tab++) listeners.responseStarted(res("https://x.test/a.mp4", "media", H("video/mp4"), { tabId: tab }));
  listeners.responseStarted(res("https://live.test/s/master.m3u8", "xmlhttprequest", H("application/vnd.apple.mpegurl")));
  for (let i = 0; i < 500; i++) listeners.responseStarted(res(`https://live.test/clip${i}.mp4`, "media", H("video/mp4")));
  await settle();
  const files = await playedSent(listeners, calls, 9);
  assert.equal(files.length, 59, "60 kept, the stream one of them");
  assert.deepEqual([files[0], files.at(-1)], ["https://live.test/clip441.mp4", "https://live.test/clip499.mp4"]);
  assert.equal(await streamSent(listeners, calls, 9, 0, "https://live.test/"), "https://live.test/s/master.m3u8");
  assert.deepEqual(await playedSent(listeners, calls, 100), [], "the least recent tabs go");
  assert.deepEqual(await playedSent(listeners, calls, 159), ["https://x.test/a.mp4"]);
  ok("media a tab loaded: bounded per tab and in tabs");
}
{
  // The browser stopped the script and started it again: what the tab loaded is read back.
  const session = {};
  const first = fakeApi({ firefox: false, session });
  load(first.api, false);
  first.listeners.responseStarted(res("https://files.test/kept.mp4", "media", H("video/mp4")));
  first.listeners.responseStarted(res("https://cdn.test/k/master.m3u8", "xmlhttprequest", H("application/vnd.apple.mpegurl")));
  await new Promise((r) => setTimeout(r, 300));
  const again = fakeApi({ firefox: false, session, pageMediaResult: [] });
  load(again.api, false);
  assert.deepEqual(await playedSent(again.listeners, again.calls, 9), ["https://files.test/kept.mp4"]);
  assert.equal(await streamSent(again.listeners, again.calls, 9, 0, "https://watch.test/k"), "https://cdn.test/k/master.m3u8");
  ok("media a tab loaded: kept across the script's restarts");
}
{
  // A page's short sound isn't one of its media (a click, a voice-search chime), a real file is.
  const { api, calls, listeners } = fakeApi({ firefox: false, pageMediaResult: [] });
  load(api, false);
  const sound = (url, length) => listeners.responseStarted({ tabId: 3, frameId: 0, type: "media", url, statusCode: 200, responseHeaders: [{ name: "content-type", value: "audio/mpeg" }, { name: "content-length", value: String(length) }] });
  sound("https://x.test/open.mp3", 6000);
  sound("https://x.test/song.mp3", 5 << 20);
  await settle();
  assert.deepEqual(await playedSent(listeners, calls, 3), ["https://x.test/song.mp3"]);
  ok("media a tab loaded: a page's short sound isn't kept");
}

// #171: a manifest whose address and type don't say so (text/plain, octet-stream, no .m3u8 or
// .mpd) is noted, never read on sight, and read (its first 4 KB) only when a stream is asked
// for: the button over a video, the right-click menu on one.
{
  const PL = "https://cdn.test/p/playlist?id=7";
  const PL_720 = "https://cdn.test/p/playlist?id=7&v=720";
  const DASH = "https://cdn.test/q/manifest?id=7";
  const CONFIG = "https://cdn.test/p/config?id=7";
  const FEED = "https://cdn.test/p/feed";
  const LONG = "https://cdn.test/p/log";
  served = {
    [PL]: { body: "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1280000,RESOLUTION=1280x720\nplaylist?id=7&v=720\n" },
    [PL_720]: { body: "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\nseg?i=0\n#EXT-X-ENDLIST\n" },
    // With a byte order mark and a comment before the MPD.
    [DASH]: { body: `﻿<?xml version="1.0"?>\n<!-- made by a packager -->\n${MPD(false).replace(/^<\?xml[^>]*\?>\n/, "")}` },
    [CONFIG]: { body: '{"autoplay":true}' },
    [FEED]: { body: '<?xml version="1.0"?><rss><channel><title>MPD news</title></channel></rss>' },
    // Text going on and on: only its start is read.
    [LONG]: { body: "x".repeat(200_000) },
  };
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  listeners.tabUpdated(9, { url: "https://watch.test/v/3" });
  await settle();
  fetched.length = 0;
  for (const d of [
    res(PL, "xmlhttprequest", H("text/plain; charset=utf-8")),
    res(PL_720, "xmlhttprequest", H("text/plain")),
    res("https://cdn.test/p/seg?i=0", "xmlhttprequest", H("video/mp2t")),
    // A DASH manifest served as a file, in an embedded player's frame.
    res(DASH, "xmlhttprequest", H("application/octet-stream"), { frameId: 1 }),
    res(CONFIG, "xmlhttprequest", H("text/plain")),
    res(FEED, "other", H("application/xml")),
    res(LONG, "xmlhttprequest", []),
    // Not possible manifests: too large, a piece, a media element's own file, a page, JSON.
    res("https://cdn.test/p/huge", "xmlhttprequest", H("text/plain", { "Content-Length": String(3 << 20) })),
    res("https://cdn.test/p/part", "xmlhttprequest", H("application/octet-stream", { "Content-Range": "bytes 0-9/99" }), { statusCode: 206 }),
    res("https://cdn.test/p/el", "media", H("text/plain")),
    res("https://cdn.test/p/page", "xmlhttprequest", H("text/html")),
    res("https://cdn.test/p/api", "xmlhttprequest", H("application/json")),
  ]) {
    listeners.responseStarted(d);
  }
  await settle();
  assert.equal(fetched.length, 0, "nothing read on sight");
  assert.equal(await streamSent(listeners, calls, 9, 0, "https://watch.test/v/3"), PL, "the button: the playlist that is one, its master first");
  assert.equal(await streamSent(listeners, calls, 9, 1, "https://player.test/e/3"), DASH, "the embedded player's: the DASH manifest, read");
  const sniffed = new Set(fetched.map(([u]) => u));
  for (const u of [PL, PL_720, DASH, CONFIG, FEED, LONG]) assert.ok(sniffed.has(u), `read: ${u}`);
  for (const u of ["https://cdn.test/p/huge", "https://cdn.test/p/part", "https://cdn.test/p/el", "https://cdn.test/p/page", "https://cdn.test/p/api"]) {
    assert.ok(!sniffed.has(u), `never read: ${u}`);
  }
  // 4 KB, and the stream's own read-ahead of a chunk or two.
  assert.ok(served[LONG].read <= 6 * 1024, `only the start read (${served[LONG].read} of 200000 bytes)`);
  assert.ok(fetched.every(([, c]) => c === "include"), "read with the site's cookies");
  const reads = fetched.length;
  await streamSent(listeners, calls, 9, 0, "https://watch.test/v/3");
  await playedSent(listeners, calls, 9);
  assert.equal(fetched.length, reads, "each read once");
  ok("#171: manifests without .m3u8/.mpd or their type: noted, read only when asked, sent");
}
{
  // SVT Play (#377): the player first loads .../dash-full.mpd, a JSON answer naming the manifest,
  // then the manifest itself from an address without .mpd. The button must send the manifest.
  const SWITCHER = "https://switcher.test/resolve/1/dash-full.mpd";
  const REAL = "https://api.test/manifest?manifestUrl=x";
  served = { [SWITCHER]: { body: '{"location":"https://cdn.test/dash-full.mpd"}' } };
  const tabs = { 9: { id: 9, url: "https://watch.test/v/5", title: "Clip", incognito: false } };
  const sender = { id: "ours", url: "https://watch.test/v/5", frameId: 0, tab: { id: 9, url: "https://watch.test/v/5" } };
  {
    const { api, calls, listeners } = fakeApi({ firefox: false, tabs });
    load(api, false);
    listeners.tabUpdated(9, { url: "https://watch.test/v/5" });
    await settle();
    fetched.length = 0;
    listeners.responseStarted(res(SWITCHER, "xmlhttprequest", H("application/json")));
    listeners.responseStarted(res(REAL, "xmlhttprequest", H("application/dash+xml")));
    await settle();
    await new Promise((r) => listeners.message({ type: "video", src: "" }, sender, r));
    assert.equal(natives(calls).find((x) => x.video).src, REAL, "the manifest, not the JSON that names it");
    assert.ok(!fetched.some(([u]) => u === SWITCHER), "a real stream known: the other isn't even read");
  }
  {
    // Alone, it is read when a stream is asked for: JSON is no stream, so nothing goes as one.
    const { api, calls, listeners } = fakeApi({ firefox: false, tabs });
    load(api, false);
    listeners.tabUpdated(9, { url: "https://watch.test/v/5" });
    await settle();
    fetched.length = 0;
    listeners.responseStarted(res(SWITCHER, "xmlhttprequest", H("application/json")));
    await settle();
    await new Promise((r) => listeners.message({ type: "video", src: "" }, sender, r));
    const m = natives(calls).find((x) => x.video);
    assert.equal(m.url, "https://watch.test/v/5", "the page goes");
    assert.equal(m.src, undefined, "no stream");
    assert.ok(fetched.some(([u]) => u === SWITCHER), "read before believed");
  }
  {
    // What keeps working: a .m3u8 or .mpd served as its own type, as text or as a file, is a stream
    // on sight; one served as something else is a stream too once read and found to be one.
    const OK_HLS = "https://cdn.test/a/index.m3u8";
    const TXT_MPD = "https://cdn.test/b/stream.mpd";
    const ODD = "https://cdn.test/c/odd.m3u8";
    served = { [ODD]: { body: "#EXTM3U"+String.fromCharCode(10)+"#EXT-X-ENDLIST"+String.fromCharCode(10) } };
    const { api, calls, listeners } = fakeApi({ firefox: false, tabs });
    load(api, false);
    listeners.tabUpdated(9, { url: "https://watch.test/v/5" });
    await settle();
    fetched.length = 0;
    listeners.responseStarted(res(OK_HLS, "xmlhttprequest", H("application/vnd.apple.mpegurl")));
    listeners.responseStarted(res(TXT_MPD, "xmlhttprequest", H("text/plain"), { frameId: 1 }));
    await settle();
    assert.equal(await streamSent(listeners, calls, 9, 0, "https://watch.test/v/5"), OK_HLS);
    assert.equal(await streamSent(listeners, calls, 9, 1, "https://player.test/e/5"), TXT_MPD);
    assert.equal(fetched.length, 0, "not read to be believed");
    listeners.tabUpdated(9, { url: "https://watch.test/v/6" });
    await settle();
    listeners.responseStarted(res(ODD, "xmlhttprequest", H("video/mp2t")));
    await settle();
    await new Promise((r) => listeners.message({ type: "video", src: "" }, { ...sender, url: "https://watch.test/v/6", tab: { id: 9, url: "https://watch.test/v/6" } }, r));
    assert.equal(natives(calls).filter((x) => x.video).pop().src, ODD, "read, and it is one");
  }
  ok("#377: a .mpd or .m3u8 address answering JSON is no stream: read first, the real manifest goes");
}
{
  // The button over a player playing from a blob: and the right-click menu: no stream known, the
  // frame's possible manifests are read and the one that is goes; a frame of its own first.
  const PL = "https://emb.test/e/playlist?id=9";
  served = {
    [PL]: { body: "#EXTM3U\n#EXT-X-STREAM-INF:RESOLUTION=640x360\nplaylist?id=9&v=360\n" },
    "https://emb.test/e/settings": { body: "volume=1" },
    "https://blog.test/ads/list": { body: "#EXTM3U\n#EXTINF:5,\nad.ts\n" },
  };
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  // The page's own frame loaded another playlist (an ad's); the player's frame (2) its stream.
  listeners.responseStarted({ tabId: 7, frameId: 0, url: "https://blog.test/ads/list", type: "xmlhttprequest", statusCode: 200, responseHeaders: H("text/plain") });
  listeners.responseStarted({ tabId: 7, frameId: 2, url: "https://emb.test/e/settings", type: "xmlhttprequest", statusCode: 200, responseHeaders: H("text/plain") });
  listeners.responseStarted({ tabId: 7, frameId: 2, url: PL, type: "xmlhttprequest", statusCode: 200, responseHeaders: H("application/octet-stream") });
  await settle();
  fetched.length = 0;
  await listeners.menu(
    { menuItemId: "mdm-link", srcUrl: "blob:https://emb.test/1", frameId: 2, frameUrl: "https://emb.test/player", pageUrl: "https://blog.test/post" },
    { id: 7, incognito: false },
  );
  await settle();
  const m = natives(calls).find((x) => x.video);
  assert.deepEqual([m.url, m.src], ["https://emb.test/player", PL], "the menu: the frame's sniffed stream");
  assert.ok(!fetched.some(([u]) => u === "https://blog.test/ads/list"), "the frame's own found: the page's frame isn't read");
  assert.ok(calls.some((c) => c[0] === "cookies" && c[1].url === PL), "the stream's cookies");
  // The button in the page's frame (0): none of its own known yet; its frame's read, it goes.
  const sender = { id: "ours", url: "https://blog.test/post", frameId: 0, tab: { id: 7, url: "https://blog.test/post" } };
  await new Promise((r) => listeners.message({ type: "video", src: "" }, sender, r));
  assert.equal(natives(calls).filter((x) => x.video).pop().src, "https://blog.test/ads/list", "the button: its own frame's");
  ok("#171: the button and the menu read the frame's possible manifests when no stream is known");
}

// The button's logo is the extension's own icon, kept in video.js as data.
{
  const video = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  const icon = readFileSync(new URL("../../extension/icons/icon-48.png", import.meta.url)).toString("base64");
  assert.ok(video.includes(`"data:image/png;base64,${icon}"`), "video.js's LOGO is extension/icons/icon-48.png");
  ok("video button logo");
}

// video.js's videoPage, with what it calls (from it to its end mark), run against `location` and
// `document`: the address the button hands the app for a <video>.
const videoPageOn = (() => {
  const source = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  const start = source.indexOf("function videoPage(v) {");
  const end = source.indexOf("// ---- the video's own address: end.");
  assert.ok(start > 0 && end > start, "video.js has videoPage and its end mark");
  const body = `${source.slice(start, end)}; return videoPage;`;
  return (location, document) => new Function("location", "document", body)(location, document);
})();

// The address the button hands the app (video.js's videoPage): the video's own page, not the page
// that plays it. YouTube's home, a search, a channel or a playlist is a list to yt-dlp, and a watch
// page opened from a mix carries the list in its address: the button sent either, and the app
// offered the whole feed (~50 videos) instead of the one playing. #311.
{
  // A page: its address, an optional ytd-watch-flexy video-id, and the cards a player can sit in
  // (each level on the way up from the video element names the video links it contains).
  const at = (href, { flexy = null, cards = [] } = {}) => {
    const element = (links) => ({
      parentElement: null,
      querySelectorAll: () => (links || []).map((href) => ({ href })),
    });
    const chain = cards.map(element);
    for (let i = 0; i < chain.length; i += 1) chain[i].parentElement = chain[i + 1] || null;
    const document = { querySelector: (sel) => (sel === "ytd-watch-flexy" && flexy ? { getAttribute: () => flexy } : null) };
    return videoPageOn({ href }, document)(chain[0] || element([]));
  };
  // A watch page from a mix or a playlist: the video's own address, without the list.
  assert.equal(at("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ&start_radio=1"), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
  assert.equal(at("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL1234567890abcdef&index=12"), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
  // A player that isn't the page's own: the card it sits in links to the video it is playing.
  assert.equal(at("https://www.youtube.com/", { cards: [[], ["https://www.youtube.com/watch?v=abc123XYZ_-&t=5s"]] }), "https://www.youtube.com/watch?v=abc123XYZ_-");
  // The card links to it twice (thumbnail and title): one video, still.
  assert.equal(at("https://www.youtube.com/results?search_query=rick+astley", { cards: [[], ["https://www.youtube.com/watch?v=abc123XYZ_-", "https://www.youtube.com/watch?v=abc123XYZ_-&pp=1"]] }), "https://www.youtube.com/watch?v=abc123XYZ_-");
  assert.equal(at("https://www.youtube.com/@someone", { cards: [[], [], ["https://www.youtube.com/watch?v=abc123XYZ_-"]] }), "https://www.youtube.com/watch?v=abc123XYZ_-");
  // A page that plays a video and names none: no address at all, so no button and nothing sent
  // (the page's own address — a channel's videos — would be a list of ~50 to yt-dlp).
  assert.equal(at("https://www.youtube.com/@someone/videos"), null);
  assert.equal(at("https://www.youtube.com/@someone/videos", { flexy: "feat12345" }), null);
  assert.equal(at("https://www.youtube.com/"), null);
  // A container naming two videos is the page's column, not a card: nothing is sent, rather than
  // some other card's video.
  assert.equal(at("https://www.youtube.com/@someone/videos", { cards: [[], ["https://www.youtube.com/watch?v=aaa", "https://www.youtube.com/watch?v=bbb"]] }), null);
  // Short links, embeds, and a site that isn't YouTube's (untouched).
  assert.equal(at("https://youtu.be/dQw4w9WgXcQ?t=10&list=PL999"), "https://youtu.be/dQw4w9WgXcQ");
  assert.equal(at("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=5"), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
  assert.equal(at("https://vimeo.com/123456789?list=abc"), "https://vimeo.com/123456789?list=abc");
  ok("#311: the button sends the video's own page, not the page playing it");
}

// A page that moves in place (history.pushState): the button sends the address it's at now, not
// the one the document loaded at; the page's own player names the video on YouTube; nothing goes
// when nothing names it (the page would be a list of ~50 to yt-dlp).
{
  const message = (listeners, m, sender) => new Promise((r) => listeners.message(m, sender, (x) => r(JSON.parse(JSON.stringify(x)))));
  const yt = (url, tabUrl = url, frameId = 0) => ({ id: "ours", url, frameId, tab: { id: 9, url: tabUrl, incognito: false } });
  const last = (calls) => natives(calls).filter((m) => m.video).at(-1);
  // Any site: `sender.url` is stale in the top frame, the tab's isn't. A frame keeps its own.
  {
    const { api, calls, listeners } = fakeApi({ firefox: false });
    load(api, false);
    await message(listeners, { type: "video" }, { id: "ours", url: "https://news.test/first", frameId: 0, tab: { id: 2, url: "https://news.test/second" } });
    assert.equal(last(calls).url, "https://news.test/second", "the tab's address, not the stale sender's");
    await message(listeners, { type: "video" }, { id: "ours", url: "https://player.test/embed", frameId: 4, tab: { id: 2, url: "https://news.test/second" } });
    assert.equal(last(calls).url, "https://player.test/embed", "a frame's own address");
    // m. and music. are the same site as www.
    await message(listeners, { type: "video", url: "https://www.mobile.test/watch?v=1" }, { id: "ours", url: "https://m.mobile.test/watch", frameId: 0, tab: { id: 2, url: "https://m.mobile.test/watch" } });
    assert.equal(last(calls).url, "https://www.mobile.test/watch?v=1", "another host of the same site");
  }
  // YouTube: the player's own answer, over the address and whatever the DOM named.
  {
    const { api, calls, listeners } = fakeApi({ firefox: false, playing: "dQw4w9WgXcQ" });
    load(api, false);
    const page = "https://www.youtube.com/@someone";
    assert.equal((await message(listeners, { type: "video", url: null, at: { x: 100, y: 80 } }, yt(page))).ok, true);
    assert.equal(last(calls).url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "the channel's player, named by the player");
    assert.equal(last(calls).referrer, page);
    await message(listeners, { type: "video", url: "https://www.youtube.com/watch?v=stale", at: { x: 1, y: 1 } }, yt("https://www.youtube.com/watch?v=stale"));
    assert.equal(last(calls).url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "the player over a DOM guess");
  }
  // The player can't be read: what the DOM named goes; with nothing named, nothing goes.
  {
    const { api, calls, listeners } = fakeApi({ firefox: false });
    load(api, false);
    await message(listeners, { type: "video", url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ", at: { x: 1, y: 1 } }, yt("https://www.youtube.com/shorts/dQw4w9WgXcQ"));
    assert.equal(last(calls).url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    const r = await message(listeners, { type: "video", url: null, at: { x: 1, y: 1 } }, yt("https://www.youtube.com/@someone/videos"));
    assert.deepEqual([r.ok, r.why], [false, "unnamed"]);
    assert.equal(natives(calls).filter((m) => m.video).length, 1, "the channel's list is never sent");
  }
  ok("in-place navigation and YouTube's player: the video playing now, or nothing");
}

// A small DOM for the sample pages (tools/extension/feeds): elements with their attributes, open
// shadow roots (<template shadowrootmode>), `a.href`, and querySelector(All) with the simple
// selectors video.js uses (a tag, #id, [attr], [attr="v"], [attr*="v"], [attr^="v"], lists).
function samplePage(html) {
  const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);
  const ENTITY = { amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'" };
  const decode = (s) => s.replace(/&(amp|lt|gt|quot|#39);/g, (_, e) => ENTITY[e]);
  let base = "about:blank";
  // Descendants in document order, not into shadow roots (as the DOM's own).
  const descendants = function* (n) {
    for (const c of n.children) {
      yield c;
      yield* descendants(c);
    }
  };
  const compound = (sel) => {
    const m = /^([a-z][\w-]*)?(?:#([\w-]+))?((?:\[[^\]]+\])*)$/i.exec(sel.trim());
    assert.ok(m, `a selector the sample DOM reads: ${sel}`);
    const attrs = [...m[3].matchAll(/\[([\w-]+)(?:([*^]?=)"([^"]*)")?\]/g)].map((a) => ({ name: a[1], op: a[2], value: a[3] }));
    return (el) =>
      (!m[1] || el.localName === m[1].toLowerCase()) &&
      (!m[2] || el.getAttribute("id") === m[2]) &&
      attrs.every(({ name, op, value }) => {
        const v = el.getAttribute(name);
        if (v === null) return false;
        return !op || (op === "=" ? v === value : op === "*=" ? v.includes(value) : v.startsWith(value));
      });
  };
  const finder = {
    querySelectorAll(sel) {
      const any = sel.split(",").map(compound);
      return [...descendants(this)].filter((el) => any.some((f) => f(el)));
    },
    querySelector(sel) {
      return this.querySelectorAll(sel)[0] || null;
    },
  };
  class Element {
    constructor(name, attributes, parent) {
      this.localName = name;
      this.attributes = attributes;
      this.children = [];
      this.parentNode = parent;
      this.shadowRoot = null;
    }
    get parentElement() {
      return this.parentNode instanceof Element ? this.parentNode : null;
    }
    getAttribute(name) {
      const a = this.attributes.find((x) => x.name === name);
      return a ? a.value : null;
    }
    get href() {
      return new URL(this.getAttribute("href"), base).href;
    }
  }
  Object.assign(Element.prototype, finder);
  const doc = { children: [], localName: "#document", ...finder };
  const stack = [doc];
  const tags = /<!--[\s\S]*?-->|<!doctype[^>]*>|<\/([\w-]+)\s*>|<([\w-]+)((?:\s+[^\s=/>]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*\/?>/gi;
  for (const t of html.matchAll(tags)) {
    if (t[1]) {
      const name = t[1].toLowerCase();
      const at = stack.findLastIndex((n) => n.localName === name || (name === "template" && n.isShadow));
      if (at > 0) stack.length = at;
      continue;
    }
    if (!t[2]) continue;
    const name = t[2].toLowerCase();
    const attributes = [...(t[3] || "").matchAll(/([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g)].map((a) => ({
      name: a[1].toLowerCase(),
      value: decode(a[2] ?? a[3] ?? a[4] ?? ""),
    }));
    const parent = stack[stack.length - 1];
    // A declarative shadow root: what follows, up to its </template>, is the host's shadow tree.
    if (name === "template" && attributes.some((a) => a.name === "shadowrootmode" && a.value === "open")) {
      const root = { children: [], localName: "#shadow-root", isShadow: true, host: parent, ...finder };
      parent.shadowRoot = root;
      stack.push(root);
      continue;
    }
    const el = new Element(name, attributes, parent);
    parent.children.push(el);
    if (!VOID.has(name)) stack.push(el);
  }
  // Every element, into shadow roots too (the button finds the videos there).
  const all = function* (n) {
    for (const c of n.children) {
      yield c;
      if (c.shadowRoot) yield* all(c.shadowRoot);
      yield* all(c);
    }
  };
  const page = doc.querySelector('meta[name="mdm-page"]').getAttribute("content");
  base = page;
  return { document: doc, page, videos: [...all(doc)].filter((e) => e.localName === "video") };
}

// #689, #643: a video in a feed names the post it sits in, never the feed. Each sample page (our
// own, reduced to what names a post on each site; tools/extension/feeds) says what each of its
// videos must name: an address ({origin}: the page's own), "page" (the page is the video's) or
// "none" (nothing names it: the feed is not sent; its own plain file is, when it has one).
{
  const dir = new URL("feeds/", import.meta.url);
  const pages = readdirSync(dir).filter((f) => f.endsWith(".html"));
  assert.ok(pages.length >= 12, "the sample pages");
  // Each page at its own address, and at the big sites' other hosts too.
  const OTHER = {
    "x.html": ["https://twitter.com/home", "https://mobile.x.com/home"],
    "reddit.html": ["https://sh.reddit.com/"],
    "linkedin.html": ["https://linkedin.com/feed/foryou/"],
    "facebook.html": ["https://web.facebook.com/", "https://m.facebook.com/"],
  };
  let checked = 0;
  for (const file of pages) {
    const { document, page, videos } = samplePage(readFileSync(new URL(file, dir), "utf8"));
    assert.ok(videos.length, `${file}: its videos`);
    for (const at of [page, ...(OTHER[file] || [])]) {
      const location = new URL(at);
      const videoPage = videoPageOn(location, document);
      for (const [i, v] of videos.entries()) {
        const expect = v.getAttribute("data-expect");
        const want = expect === "page" ? location.href : expect === "none" ? null : expect.replaceAll("{origin}", location.origin);
        const got = videoPage(v);
        assert.equal(got, want, `${file} at ${at}: video ${i + 1}`);
        // Never the page itself, unless it is the video's own.
        if (expect !== "page") assert.notEqual(got, location.href, `${file}: video ${i + 1} named the page`);
        checked += 1;
      }
    }
  }
  ok(`#689: a video in a feed names its post, never the feed (${pages.length} sample pages, ${checked} videos)`);
}

// What the resolvers build is a form yt-dlp's extractors read (their _VALID_URL, yt-dlp
// 2026.08.19), so the app's lookup finds the one video.
{
  const YTDLP = [
    /^https?:\/\/(?:www\.)?linkedin\.com\/feed\/update\/urn:li:activity:(\d+)/,
    /^https?:\/\/(?:[\w-]+\.)?facebook\.com\/(?:(?:permalink\.php|video\.php|watch\/?)\?(?:.*?)(?:v|story_fbid)=|[^/]+\/videos\/(?:[^/]+\/)?|[^/]+\/posts\/|groups\/[^/]+\/(?:permalink|posts)\/|reel\/)(pfbid[A-Za-z0-9]+|\d+)/,
    /^https?:\/\/(?:(?:www|m(?:obile)?)\.)?(?:twitter|x)\.com\/(?:(?:i\/web|[^/]+)\/status|statuses)\/(\d+)/,
    /^https?:\/\/(?:www\.)?instagram\.com(?:\/[^/?#]+)?\/(?:p|tv|reels?)\/([^/?#&]+)/,
    /^https?:\/\/(?:\w+\.)?reddit\.com\/(?:(?:r|user)\/[^/]+\/)?comments\/([^/?#&]+)/,
    /^https?:\/\/www\.tiktok\.com\/@[\w.-]+\/video\/(\d+)/,
    /^https?:\/\/vimeo\.com\/(\d+)(?:\/[\da-f]{10})?$/,
    /^https?:\/\/www\.dailymotion\.com\/video\/([^/?_&#]+)/,
  ];
  const dir = new URL("feeds/", import.meta.url);
  let checked = 0;
  for (const file of readdirSync(dir).filter((f) => f.endsWith(".html") && !/^(social|article)\.html$/.test(f))) {
    const { page, videos } = samplePage(readFileSync(new URL(file, dir), "utf8"));
    for (const v of videos) {
      const expect = v.getAttribute("data-expect").replaceAll("{origin}", new URL(page).origin);
      // fb.watch's short links lead to the video's page; a LinkedIn post known by its ugcPost
      // URN is its public page, which yt-dlp's generic reader reads.
      if (expect === "none" || expect === "page" || expect.startsWith("https://fb.watch/") || /urn:li:ugcPost:/.test(expect)) continue;
      assert.ok(YTDLP.some((r) => r.test(expect)), `${file}: ${expect} is a form yt-dlp reads`);
      checked += 1;
    }
  }
  ok(`#689: the ${checked} addresses built are forms yt-dlp's extractors read`);
}

// #689: what the background does with what video.js named on a feed. A post: it goes alone (no
// stream the tab loaded, which may be another video's, nor the feed's title), the feed as its
// Referer. Nothing named: never the feed; the video's own plain file when it has one (the feed
// as its Referer), else nothing and the button says to open the post.
{
  const message = (listeners, m, sender) => new Promise((r) => listeners.message(m, sender, (x) => r(JSON.parse(JSON.stringify(x)))));
  const at = (url) => ({ id: "ours", url, frameId: 0, tab: { id: 4, url, incognito: false } });
  const videos = (calls) => natives(calls).filter((m) => m.video);
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const feed = "https://www.linkedin.com/feed/";
  // A stream the feed loaded (another video's, as far as anyone knows).
  listeners.responseStarted({ tabId: 4, frameId: 0, url: "https://cdn.test/feed/master.m3u8", responseHeaders: [] });
  const post = "https://www.linkedin.com/feed/update/urn:li:activity:7390000000000000001/";
  assert.equal((await message(listeners, { type: "video", url: post, src: "", title: "Feed | LinkedIn" }, at(feed))).ok, true);
  assert.deepEqual(
    (({ url, src, title, referrer }) => ({ url, src, title, referrer }))(videos(calls).at(-1)),
    { url: post, src: undefined, title: "", referrer: feed },
    "the post alone, the feed as its Referer",
  );
  // Nothing names it, no file of its own: nothing goes, and the button says why.
  const r = await message(listeners, { type: "video", url: null, src: "", title: "Feed | LinkedIn" }, at(feed));
  assert.deepEqual([r.ok, r.why], [false, "feed"]);
  assert.equal(videos(calls).length, 1, "the feed is never sent");
  // Nothing names it, its own plain file: that file, the feed as its Referer, no title.
  await message(listeners, { type: "video", url: null, src: "https://media.test/clip.mp4", title: "Home" }, at("https://social.test/home"));
  const file = videos(calls).at(-1);
  assert.deepEqual([file.src, file.url, file.title], ["https://media.test/clip.mp4", "https://social.test/home", ""]);
  // Another host of the same site is the site's (Facebook's short links, X's old name).
  await message(listeners, { type: "video", url: "https://fb.watch/sAmPlE7x/", src: "" }, at("https://web.facebook.com/"));
  assert.equal(videos(calls).at(-1).url, "https://fb.watch/sAmPlE7x/");
  await message(listeners, { type: "video", url: "https://x.com/someone/status/1", src: "" }, at("https://twitter.com/home"));
  assert.equal(videos(calls).at(-1).url, "https://x.com/someone/status/1");
  // Another site's address named by a page is not taken: the page's own goes, as before.
  await message(listeners, { type: "video", url: "https://elsewhere.test/v/1", src: "" }, at("https://news.test/story"));
  assert.equal(videos(calls).at(-1).url, "https://news.test/story");
  ok("#689: a post alone, the video's own file, or nothing; never the feed");
}

// Shorts and live streams name their video in the path (video.js videoPage); the page is the
// video's, not a list. Their addresses as yt-dlp reads them (the same video as watch?v=).
{
  const named = (href) => videoPageOn({ href }, { querySelector: () => null })({ parentElement: null, querySelectorAll: () => [] });
  assert.equal(named("https://www.youtube.com/shorts/cxkLg22mWco"), "https://www.youtube.com/watch?v=cxkLg22mWco");
  assert.equal(named("https://www.youtube.com/shorts/cxkLg22mWco?feature=share"), "https://www.youtube.com/watch?v=cxkLg22mWco");
  assert.equal(named("https://m.youtube.com/shorts/cxkLg22mWco"), "https://www.youtube.com/watch?v=cxkLg22mWco");
  assert.equal(named("https://www.youtube.com/live/abc123XYZ_-"), "https://www.youtube.com/watch?v=abc123XYZ_-");
  // The Shorts landing page, before it moves to the first one, names none.
  assert.equal(named("https://www.youtube.com/shorts/"), null);
  ok("video.js: Shorts and live pages name their video");
}

// The qualities' menu (video.js; its plain functions run here): the questions' pace and their
// minute, a row's text, and what a page can never make it do.
{
  const source = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  // A function's own source (async with its keyword), by its braces.
  const fn = (name) => {
    let start = source.indexOf(`function ${name}(`);
    assert.ok(start > 0, `video.js has ${name}`);
    if (source.slice(start - 6, start) === "async ") start -= 6;
    let depth = 0;
    // The body's brace: after the parameters (which may destructure).
    for (let i = source.indexOf(") {", start) + 2; i < source.length; i += 1) {
      if (source[i] === "{") depth += 1;
      if (source[i] === "}" && --depth === 0) return source.slice(start, i + 1);
    }
    throw new Error(`${name}: no end`);
  };
  const make = (names, value) => new Function(`${names.map(fn).join("\n")}\nreturn ${value};`)();
  // 100 ms before the first question, then the old backoff's target (1 s x 1.25^n, 2 s at most)
  // less what the last answer took (50 ms at least): an older app (60 ms) is asked as seldom as
  // before, a newer one (holding the answer ~1.5 s) is asked again almost at once early on.
  const pollDelay = make(["pollDelay"], "pollDelay");
  assert.equal(pollDelay(0, 0), 100);
  assert.equal(pollDelay(0, 5000), 100);
  assert.equal(pollDelay(1, 60), 1190);
  assert.equal(pollDelay(1, 1500), 50);
  assert.equal(pollDelay(2, 1500), 50);
  assert.equal(pollDelay(2, 999), 564);
  assert.equal(pollDelay(9, 0), 2000);
  assert.equal(pollDelay(9, 1500), 50);
  assert.equal(pollDelay(9, 99999), 50);
  assert.equal(pollDelay(3, undefined), 1953);
  assert.equal(pollDelay(3, NaN), 1953);
  assert.equal(pollDelay(3, -20), 1953);
  // The qualities in tabs: sound-only ones apart when the video has both kinds; else one list. Every
  // number is the place in the app's list.
  const groupChoices = make(["groupChoices"], "groupChoices");
  const grp = (list, max) => JSON.parse(JSON.stringify(groupChoices(list, max)));
  const mixed = [{ audio: false }, { audio: false }, { audio: true }, { audio: false }, { audio: true }];
  assert.deepEqual(grp(mixed), { tabbed: true, video: [0, 1, 3], audio: [2, 4], all: [0, 1, 2, 3, 4], start: "video" });
  assert.deepEqual(grp([{ audio: true }, { audio: false }]), { tabbed: true, video: [1], audio: [0], all: [0, 1], start: "video" });
  assert.deepEqual(grp([{ label: "720p" }, { label: "360p" }]), { tabbed: false, video: [0, 1], audio: [], all: [0, 1], start: "video" }, "an older app: no audio flag, one list");
  assert.equal(grp([{ audio: "yes" }, { audio: 1 }, null, "x"]).tabbed, false, "only true counts as audio");
  assert.deepEqual(grp([{ audio: true }, { audio: true }]), { tabbed: false, video: [], audio: [0, 1], all: [0, 1], start: "audio" }, "only audio: one list");
  assert.deepEqual(grp([{ audio: false }, { audio: false }]), { tabbed: false, video: [0, 1], audio: [], all: [0, 1], start: "video" }, "only video: one list");
  assert.deepEqual(grp([]), { tabbed: false, video: [], audio: [], all: [], start: "video" });
  assert.deepEqual(grp(undefined), { tabbed: false, video: [], audio: [], all: [], start: "video" });
  assert.deepEqual(grp(mixed, 3), { tabbed: true, video: [0, 1], audio: [2], all: [0, 1, 2], start: "video" }, "the cap keeps the app's order");
  // A row: the app's name for the quality as it is (set as text), its size in the browser's language.
  const { sizeText, rowText } = make(["sizeText", "rowText"], "{ sizeText, rowText }");
  assert.equal(sizeText(187500000, "en"), "178.8 MB");
  assert.equal(sizeText(3 * 2 ** 30, "en"), "3 GB");
  assert.equal(sizeText(null, "en"), "");
  assert.equal(sizeText(undefined, "en"), "");
  assert.deepEqual({ ...rowText({ label: "<img src=x onerror=alert(1)> 720p", ext: "mp4", size: null }, "en") }, { name: "<img src=x onerror=alert(1)> 720p", size: "" });
  assert.deepEqual({ ...rowText(null, "en") }, { name: "", size: "" });
  // Under the Audio tab, a sound row says what it is without "Audio only" again on every row.
  const audioName = make(["audioName"], "audioName");
  const original = (type) => `Original ${type}`;
  const bare = (s) => s && s.replace(/[\u2066\u2069]/g, "");
  assert.equal(bare(audioName({ key: "audio", ext: "m4a" }, original)), "Original M4A");
  assert.equal(bare(audioName({ key: "audio", ext: "<b>webm" }, original)), "Original BWEBM");
  assert.equal(audioName({ key: "audio" }, original), null, "no type: the app's own words");
  assert.equal(bare(audioName({ key: "mp3-0" }, original)), "MP3 VBR");
  assert.equal(bare(audioName({ key: "mp3-320" }, original)), "MP3 320 kbps");
  assert.equal(bare(audioName({ key: "opus" }, original)), "OPUS");
  assert.equal(bare(audioName({ key: "flac" }, original)), "FLAC");
  assert.equal(audioName({ key: "v1080" }, original), null);
  assert.equal(audioName({ key: "mp3-x" }, original), null);
  assert.equal(audioName(null, original), null);
  const names = ["audio", "mp3-0", "mp3-320", "mp3-128", "opus", "flac"].map((key) => audioName({ key, ext: "m4a" }, original));
  assert.equal(new Set(names).size, names.length, "every row its own name");
  // Following a lookup: the next question only after the last answer, on a clock of its own here.
  const followLookup = make(["pollDelay", "followLookup"], "followLookup");
  const follow = async (first, answers, { alive = () => true, took = 0 } = {}) => {
    let clock = 0;
    let open = 0;
    const asked = [];
    const result = await followLookup(first, {
      ask: async (token, ms) => {
        assert.equal(open++, 0, "one question at a time");
        asked.push({ token, at: clock, ms });
        await Promise.resolve();
        clock += took;
        open -= 1;
        return answers.length ? answers.shift() : { lookup: { state: "pending" } };
      },
      alive: () => alive(asked),
      sleep: async (ms) => {
        clock += ms;
      },
      now: () => clock,
      end: 60000,
    });
    return { result: result && { ...result }, asked, clock };
  };
  const pending = { state: "pending", token: "t1" };
  const ready = { state: "ready", token: "t1", choices: [{ label: "720p MP4", ext: "mp4", size: 1 }] };
  let f = await follow(pending, [{ lookup: pending }, { lookup: ready }]);
  assert.deepEqual(f.result, ready);
  assert.deepEqual(f.asked.map((a) => [a.token, a.at]), [["t1", 100], ["t1", 1350]], "100 ms, then the old backoff (an app that answers at once)");
  // Still looking after a minute: given up then, never asked after it.
  f = await follow(pending, []);
  assert.deepEqual(f.result, { state: "late" });
  assert.equal(f.clock, 60000);
  assert.ok(f.asked.every((a) => a.at < 60000 && a.ms > 0 && a.token === "t1"), "the token from the first answer");
  // An older app (answers in ~60 ms): no more questions than the old schedule (1 s x 1.25^n, 2 s at
  // most: asks at 1 s, 2.25 s, ...) made in a minute.
  let old = 0;
  for (let at = 0, n = 0; (at += Math.min(2000, 1000 * 1.25 ** n)) < 60000; n += 1) old += 1;
  f = await follow(pending, [], { took: 60 });
  assert.deepEqual(f.result, { state: "late" });
  assert.equal(f.asked[0].at, 100);
  assert.ok(f.asked.length <= old, `old host: ${f.asked.length} questions in a minute, the old schedule's ${old}`);
  // A newer app (holds the answer ~1.5 s while it looks): one at a time, asked again almost at once
  // early on, and the answer reaches the page soon after it is ready. A host that is ready at `r`
  // answers its question in flight 100 ms later (else after 1.5 s).
  const latency = async (r) => {
    let clock = 0;
    let done = null;
    let ready = { state: "ready", token: "t1", choices: [] };
    const res = await followLookup(pending, {
      ask: async () => {
        const start = clock;
        const end = Math.min(start + 1500, Math.max(start, r) + 100);
        clock = end;
        return { lookup: end >= r ? ((done = end), ready) : pending };
      },
      alive: () => true,
      sleep: async (ms) => {
        clock += ms;
      },
      now: () => clock,
      end: 60000,
    });
    assert.equal(res.state, "ready");
    return done - r;
  };
  for (const r of [0, 300, 1000, 1700, 2500, 3000]) assert.ok((await latency(r)) <= (r ? 150 : 250), `new host, ready at ${r} ms: answered ${await latency(r)} ms later`);
  for (const r of [5000, 12000, 30000]) assert.ok((await latency(r)) <= 150, `new host, ready at ${r} ms: answered ${await latency(r)} ms later`);
  f = await follow(pending, [], { took: 1500 });
  assert.deepEqual(f.result, { state: "late" });
  assert.ok(f.asked.length > 20 && f.asked.length <= 40, `new host: ${f.asked.length} questions in a minute`);
  // In between (an answer after 600 ms): the rest of the second.
  f = await follow(pending, [{ lookup: pending }, { lookup: pending }, { lookup: ready }], { took: 600 });
  assert.deepEqual(f.asked.map((a) => a.at), [100, 1350, 2913]);
  assert.deepEqual(f.result, ready);
  // Dropped (the menu closed, the video or the page changed): nothing more, no answer shown.
  f = await follow(pending, [{ lookup: pending }, { lookup: ready }], { alive: (asked) => asked.length < 1 });
  assert.deepEqual([f.result, f.asked.length], [null, 1]);
  // The background's other answers.
  assert.deepEqual((await follow(pending, [{ late: true }])).result, { state: "late" });
  assert.deepEqual((await follow(pending, [{ fallback: true }])).result, { state: "use-window", token: "t1" }, "another DDM chosen meanwhile");
  assert.deepEqual((await follow(pending, [{ ok: false }])).result, { state: "gone" });
  assert.equal((await follow(pending, [{ ok: false, gone: true }])).result, null, "the extension gone");
  f = await follow({ state: "pending" }, []);
  assert.deepEqual([f.result, f.asked.length], [{ state: "gone" }, 0], "no token, no questions");
  assert.deepEqual((await follow({ state: "use-window" }, [])).result, { state: "use-window", token: "" });
  // What each answer comes to (replyState, then lookupAction): the background's answer to the
  // click, the lookup's end, and the answer to a choice.
  const { replyState, lookupAction, firstWindow, pointerTarget } = make(
    ["replyState", "lookupAction", "firstWindow", "pointerTarget"],
    "{ replyState, lookupAction, firstWindow, pointerTarget }",
  );
  const key = (k) => k;
  const act = (r, picked) => {
    const s = replyState(r);
    return s && { ...lookupAction(s, key, picked) };
  };
  const look = (lookup) => act({ lookup });
  assert.deepEqual(look(pending), { act: "follow" });
  assert.deepEqual(look(ready), { act: "offer" });
  assert.deepEqual(look({ state: "ready", token: "t1", choices: [] }), { act: "say", text: "video_no_choices" });
  assert.deepEqual(look({ state: "failed", reason: "no-choices" }), { act: "say", text: "video_no_choices" });
  assert.deepEqual(look({ state: "failed", reason: "no-ytdlp" }), { act: "say", text: "video_no_ytdlp" });
  assert.deepEqual(look({ state: "failed", reason: "error", message: "HTTP 403" }), { act: "say", text: "video_failed_lookup", detail: "HTTP 403" });
  assert.deepEqual(look({ state: "failed", reason: "error", message: { html: "<b>" } }), { act: "say", text: "video_failed_lookup", detail: "" });
  assert.deepEqual(look({ state: "list" }), { act: "say", text: "video_playlist" });
  assert.deepEqual(look({ state: "busy" }), { act: "say", text: "video_busy" });
  assert.deepEqual(look({ state: "late" }), { act: "say", text: "video_too_long" });
  assert.deepEqual(act({ late: true }), { act: "say", text: "video_too_long" }, "no answer within the minute");
  assert.deepEqual(look({ state: "off" }), { act: "flash", kind: "err", text: "video_off" });
  assert.deepEqual(act({ ok: false, why: "unnamed" }), { act: "flash", kind: "err", text: "video_unnamed" });
  // The click as it always was (the Download video window): no lookups here, the app forgot the
  // lookup, an older or a newer answer, a host that failed.
  const windows = [{ fallback: true }, { ok: false }, { ok: false, why: "not_running" }, { lookup: null }, { lookup: "ready" }, {}, null];
  for (const s of ["use-window", "gone", "a-newer-state", "", 7]) windows.push({ lookup: { state: s } });
  for (const r of windows) assert.deepEqual(act(r), { act: "window" }, JSON.stringify(r));
  assert.equal(act({ ok: false, gone: true }), null, "the extension gone: nothing");
  // A choice: downloading, or handed to the app ("sent", which didn't say in time what became of
  // it: it will download it, so no window), or queued; anything else, the window.
  assert.deepEqual(act({ lookup: { state: "started" } }, true), { act: "flash", kind: "ok", text: "video_sent" });
  assert.deepEqual(act({ lookup: { state: "sent" } }, true), { act: "flash", kind: "ok", text: "video_sent" });
  assert.deepEqual(act({ lookup: { state: "queued" } }, true), { act: "flash", kind: "ok", text: "video_queued" });
  for (const s of ["use-window", "gone", "pending", "ready", "busy", "off", "failed", "list", "late"]) {
    assert.deepEqual(act({ lookup: { state: s, token: "t1", choices: ready.choices } }, true), { act: "window" }, `a choice answered ${s}`);
  }
  assert.deepEqual(act({ fallback: true }, true), { act: "window" });
  assert.equal(act({ ok: false, gone: true }, true), null);
  // One window per click, whichever way it comes to it: a lookup's end, then the choice's answer,
  // then "Open in…" (and a second click gets its own).
  const click = {};
  assert.deepEqual([firstWindow(click), firstWindow(click), firstWindow(click)], [true, false, false]);
  assert.equal(firstWindow({}), true, "the next click");
  assert.equal(firstWindow(null), false);
  // …and every way there goes through it: the window's message is sent from useWindow alone,
  // after firstWindow, and each answer is read by lookupAction.
  assert.equal(source.split('send({ type: "video",').length, 2, "one place sends the window's message");
  assert.ok(/if \(!firstWindow\(j\)\) return;[^]*send\(\{ type: "video",/.test(fn("useWindow")), "useWindow: once per click");
  for (const name of ["firstAnswer", "settle", "sendPick"]) assert.ok(fn(name).includes("lookupAction(s, t"), `${name} reads answers with lookupAction`);
  assert.ok(fn("sendPick").includes("lookupAction(s, t, true)"), "sendPick: a choice's answer");
  assert.ok(fn("download").includes("firstAnswer(j, r,"), "the click's first answer, through firstAnswer");
  // While the qualities' menu is open, the pointer over another video (a feed's next one, under
  // the menu) leaves the button on its own; over its own, or with no menu, it follows.
  const mine = { id: "mine" };
  const next = { id: "next" };
  assert.equal(pointerTarget(next, mine, true), null);
  assert.equal(pointerTarget(mine, mine, true), mine);
  assert.equal(pointerTarget(next, mine, false), next);
  assert.equal(pointerTarget(next, null, false), next);
  assert.equal(pointerTarget(null, mine, false), null);
  assert.ok(fn("look").includes("pointerTarget(videoAt(pointer.x, pointer.y), video, menuOpen && panel === picks)"), "look: kept on its video while picking");
  // What a page can never make it do: the menu's rows and keys take only the user's own clicks
  // and keys; rows are text (markup only for the fixed parts); a pick sends the token and a place
  // in the app's own list alone (a row of the page's own list waits for the app's); nothing is
  // asked on a timer of its own, and before a click only as askAhead allows.
  assert.ok(/shadow\.addEventListener\("keydown", \(e\) => \{\s*if \(!e\.isTrusted\) return;/.test(source), "keys: the user's own");
  assert.ok(/picks\.addEventListener\("click", \(e\) => \{\s*if \(!e\.isTrusted\) return;/.test(source), "rows: the user's own clicks");
  // The page's own controls: a key the open menu used doesn't reach the page, and the button (and
  // so its menu) stays where it is while the menu is open.
  const keys = source.slice(source.indexOf('shadow.addEventListener("keydown"'), source.indexOf("// The main part: a click downloads"));
  assert.equal(keys.match(/e\.stopPropagation\(\)/g).length, 5, "Escape, Enter and Space, the tabs' arrows, between tabs and rows, the rows' arrows: not the page's too");
  assert.ok(fn("spot").includes("if (menuOpen && auto.v === video && auto.at) return auto.at;"), "spot: still while its menu is open");
  assert.equal(source.match(/innerHTML/g).length, 1, "markup only for the fixed parts");
  assert.ok(!/insertAdjacentHTML|outerHTML|setInterval/.test(source));
  assert.ok(/send\(\{ type: "video-pick", token: j\.token, choice: i \}\)/.test(fn("sendPick")), "a pick: the token and the app's place");
  assert.ok(/if \(row\.choice < 0\) return wantRow\(j, row\.key\);/.test(fn("pick")), "a row of the page's own list waits for the app's");
  assert.ok(/if \(row\) return void sendPick\(j, row\.choice\);/.test(fn("offer")), "…and goes as the app's place for that quality");
  assert.equal(source.split('"video-lookup"').length, 2, "one lookup message");
  assert.equal(source.split("askLookup(").length - 1, 3, "asked by the click, again when the app forgot what was asked ahead, and ahead");
  assert.ok(fn("download").includes("askLookup(j.asked)") && fn("relook").includes("askLookup(j.asked)"));
  assert.ok(fn("askAhead").includes("askLookup(asked, true)"), "ahead: said so");
  assert.ok(/if \(aheadOff \|\| privateWindow \|\| never \|\| allowed !== true/.test(fn("askAhead")), "never from a private window, nor where the button is off");
  assert.ok(fn("askAhead").includes('why === "rest" && !asksAhead('), "resting over a video: on its own page only");
  assert.ok(/if \(aheadJob && !aheadJob\.reply\) return;/.test(fn("askAhead")), "one at a time");
  assert.equal(source.split('askAhead(video, "rest")').length, 2, "resting: from the button's frame loop alone");
  assert.equal(source.split('askAhead(video, "hover")').length, 3, "the pointer or the keyboard reaching the button");
  assert.ok(/send\(\{ type: "video-cancel", token: a\.token \}\)/.test(fn("dropAhead")), "called off");
  // The keyboard's focus moves into the menu, and back to the button as it closes, only for
  // someone using the keyboard: after a mouse click the button would look keyboard-focused and
  // stay once the pointer left.
  assert.ok(/const focusHere = \(\) => \{\s*const a = shadow\.activeElement;\s*return lastInput === "key" &&/.test(source), "focusHere: the keyboard only");
  assert.ok(/if \(back && lastInput === "key"\) go\.focus\(\);\s*else if \(back\) shadow\.activeElement\.blur\(\);/.test(fn("closeMenu")), "closeMenu: back to the button for the keyboard only");
  assert.ok(/if \(e\.isTrusted\) lastInput = "pointer";/.test(source) && /if \(e\.isTrusted\) lastInput = "key";/.test(source), "what was used last: the person's own input");
  // Its texts: English's, kept as the fallback too.
  const english = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
  for (const key of ["video_looking", "video_open_mdm", "video_no_choices", "video_playlist", "video_no_ytdlp", "video_too_long", "video_busy", "video_queued", "video_failed_lookup", "video_pick_waiting", "video_pick_gone"]) {
    assert.ok(english[key], key);
    assert.ok(source.includes(`${key}: ${JSON.stringify(english[key].message)}`), `video.js FALLBACK: ${key}`);
    assert.ok(source.includes(`t("${key}")`), `video.js uses ${key}`);
  }
  // The sound's rows of the page's own list: named as the app names them, their type put in.
  for (const key of ["video_audio_original", "video_audio_only"]) {
    assert.equal(english[key].placeholders.type.content, "$1", key);
    assert.ok(source.includes(`${key}: ${JSON.stringify(english[key].message.replace("$type$", "$1"))}`), `video.js FALLBACK: ${key}`);
    assert.ok(source.includes(`"${key}"`), `video.js uses ${key}`);
  }
  ok("video.js: the qualities' menu follows a lookup one question at a time for a minute; rows as text, the user's own clicks and keys");
}

// The qualities the page itself names, at once while the app looks: read by the background
// (YouTube's player response; an HLS or DASH manifest the tab loaded) into yt-dlp's terms, listed
// by video.js as the app lists them (mdm_tools::resolve::choices), then the app's list in their
// place, row for row (its `key`s). Anything unusual: nothing, the menu waits for the app.
{
  const source = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  const fn = (name) => {
    const start = source.indexOf(`function ${name}(`);
    assert.ok(start > 0, `video.js has ${name}`);
    let depth = 0;
    for (let i = source.indexOf(") {", start) + 2; i < source.length; i += 1) {
      if (source[i] === "{") depth += 1;
      if (source[i] === "}" && --depth === 0) return source.slice(start, i + 1);
    }
    throw new Error(`${name}: no end`);
  };
  const { pageChoices, appRows, asksAhead, groupChoices } = new Function(
    `${["pageChoices", "appRows", "asksAhead", "groupChoices"].map(fn).join("\n")}\nreturn { pageChoices, appRows, asksAhead, groupChoices };`,
  )();
  const audioLabel = (kind, type) => (kind === "original" ? `Audio only, original ${type}` : `Audio only ${type}`);
  // The names without their left-to-right isolates (checked once below).
  const bare = (s) => String(s).replace(/[\u2066\u2069]/g, "");
  const rowsOf = (page, merge) => JSON.parse(JSON.stringify(pageChoices(page, merge, audioLabel))).map((r) => ({ ...r, label: bare(r.label) }));
  {
    const raw = pageChoices({ duration: 10, formats: [{ ext: "mp4", height: 720, video: true, audio: true, size: 1 }] }, true, audioLabel);
    assert.equal(raw[0].label, "\u2066720p MP4\u2069", "a resolution's name reads left to right, as the app writes it");
    assert.equal(raw[1].label, "Audio only \u2066MP3 VBR\u2069", "a sound's type too");
  }
  // resolve.rs's SAMPLE (choices_with_and_without_merging), in yt-dlp's terms: the same rows,
  // names, keys and sizes as the app's.
  const SAMPLE = {
    duration: 61.5,
    formats: [
      { ext: "m4a", video: false, audio: true, size: 1000000 },
      { ext: "mp4", height: 360, video: true, audio: true, size: 5000000 },
      { ext: "mp4", height: 720, fps: 30, video: true, audio: false, size: 20000000 },
      { ext: "mp4", height: 1080, fps: 60, video: true, audio: false, size: 60000000 },
    ],
  };
  const merged = rowsOf(SAMPLE, true);
  assert.deepEqual(merged.map((r) => r.label), [
    "1080p 60fps MP4", "720p MP4", "360p MP4", "Audio only, original M4A", "Audio only MP3 VBR", "Audio only MP3 320 kbps",
    "Audio only MP3 256 kbps", "Audio only MP3 192 kbps", "Audio only MP3 128 kbps", "Audio only OPUS", "Audio only FLAC",
  ]);
  assert.deepEqual(merged.map((r) => r.key), ["v1080", "v720", "v360", "audio", "mp3-0", "mp3-320", "mp3-256", "mp3-192", "mp3-128", "opus", "flac"]);
  assert.deepEqual([merged[0].size, merged[5].size, merged.at(-2).size, merged.at(-1).size], [61000000, 2460000, 1230000, 6918750], "video + best sound; MP3, Opus, FLAC from the length");
  assert.ok(merged.every((r) => r.choice === -1), "none is a place in the app's list");
  assert.deepEqual(merged.map((r) => r.audio), [false, false, false, true, true, true, true, true, true, true, true]);
  assert.equal(groupChoices(merged, merged.length).tabbed, true, "tabs, as the app's list will have");
  const plain = rowsOf(SAMPLE, false);
  assert.deepEqual(plain.map((r) => [r.label, r.key, r.size]), [["360p MP4", "v360", 5000000], ["Audio only, original M4A", "audio", 1000000]], "without ffmpeg: what carries its sound");
  // Nothing usable, or a list the app makes otherwise: nothing (the menu waits for the app).
  assert.deepEqual(rowsOf(SAMPLE, undefined), [], "an app that doesn't say whether it merges");
  assert.deepEqual(rowsOf({ formats: [{ ext: "m4a", video: false, audio: true, size: 9 }] }, true), [], "no video");
  assert.deepEqual(rowsOf({ formats: [{ ext: "mp4", video: true, audio: true, size: 9 }] }, true), [], "a video the app lists by its format alone");
  assert.deepEqual(rowsOf(null, true), []);
  assert.deepEqual(rowsOf({ formats: "x" }, true), []);
  // The page's numbers are only shown: junk is dropped, never passed on.
  const odd = rowsOf({ duration: "60", formats: [{ ext: "mp4<b>", height: 720.4, video: true, audio: true, size: -1, fps: "60" }] }, false);
  assert.deepEqual(odd.map((r) => [r.label, r.size, r.ext]), [["720p MP4B", null, "mp4b"]]);

  // The app's list as rows: each its place in the list, and its key (an older app's, or one that
  // isn't one: its place, matching nothing of the page's).
  const app = JSON.parse(JSON.stringify(appRows([{ label: "720p MP4", key: "v720", size: 1 }, { label: "Audio", audio: true }, { label: "x", key: "<img>" }, null], 100)));
  assert.deepEqual(app.map((r) => [r.key, r.choice, r.audio]), [["v720", 0, false], ["#1", 1, true], ["#2", 2, false], ["#3", 3, false]]);
  assert.equal(appRows([{}, {}, {}], 2).length, 2, "the cap");

  // Asked ahead, resting over the video, only on that video's own page.
  const own = { many: false, main: true, feed: false };
  assert.equal(asksAhead("https://www.youtube.com/watch?v=aqz-KE-bpKQ", "https://www.youtube.com/watch?v=aqz-KE-bpKQ", own), true);
  assert.equal(asksAhead("https://www.youtube.com/watch?v=x&list=y", "https://www.youtube.com/watch?v=x", own), true, "a watch page in a list: its video");
  assert.equal(asksAhead("https://www.youtube.com/embed/x", "https://www.youtube.com/watch?v=x", own), true);
  assert.equal(asksAhead("https://www.youtube.com/watch?v=x", "https://www.youtube.com/watch?v=y", { ...own, main: false }), false, "not the main player (a preview)");
  assert.equal(asksAhead("https://www.youtube.com/shorts/x", "https://www.youtube.com/watch?v=x", own), false, "Shorts: an endless feed");
  assert.equal(asksAhead("https://www.youtube.com/", "https://www.youtube.com/watch?v=x", own), false, "the home page's previews");
  assert.equal(asksAhead("https://vimeo.com/123", "https://vimeo.com/123", own), true, "another site's video page");
  assert.equal(asksAhead("https://vimeo.com/123", "https://vimeo.com/123", { ...own, many: true }), false, "a page of several videos");
  assert.equal(asksAhead("https://x.com/home", "https://x.com/a/status/1", own), false, "a post in a feed");
  assert.equal(asksAhead("https://news.test/", "https://news.test/", { ...own, feed: true }), false, "a feed");
  assert.equal(asksAhead("https://news.test/a", null, own), false, "nothing named");
  assert.equal(asksAhead("not an address", "x", own), false);

  // The background's reading of YouTube's player response (a real one, Big Buck Bunny's, as
  // playingAt takes it from the page: tools/extension/fixtures/youtube-player.json), listed: the
  // rows the app's look-up of the same video lists (`video-info`, yt-dlp 2026.10.26: the same
  // heights, frame rates and kinds; yt-dlp's sizes may differ, its own refine them).
  const { api } = fakeApi({ firefox: true });
  const g = load(api, true);
  const player = JSON.parse(readFileSync(new URL("fixtures/youtube-player.json", import.meta.url), "utf8"));
  const yt = g.ytFormats(player, "aqz-KE-bpKQ");
  assert.equal(yt.duration, 635);
  assert.equal(yt.formats.length, 41);
  const ytRows = rowsOf(JSON.parse(JSON.stringify(yt)), true);
  assert.deepEqual(ytRows.map((r) => r.label), [
    "2160p 60fps MP4", "1440p 60fps MP4", "1080p 60fps MP4", "720p 60fps MP4", "480p MP4", "360p MP4", "240p MP4", "144p MP4",
    "Audio only, original M4A", "Audio only MP3 VBR", "Audio only MP3 320 kbps", "Audio only MP3 256 kbps", "Audio only MP3 192 kbps",
    "Audio only MP3 128 kbps", "Audio only OPUS", "Audio only FLAC",
  ], "as DDM lists this video");
  assert.equal(ytRows[0].size, 1362269481 + 30767611, "the largest 2160p (VP9) and the best sound (itag 258)");
  assert.equal(ytRows[5].size, Math.floor((634624 / 1000) * (359607 / 1000) * 125), "360p: the format with its sound, sized from its bitrate");
  assert.equal(ytRows[8].size, 30767611);
  assert.equal(ytRows.at(-2).size, 635 * 160 * 125, "Opus from M4A: encoded");
  const ytPlain = rowsOf(JSON.parse(JSON.stringify(yt)), false);
  assert.deepEqual(ytPlain.map((r) => r.label), ["360p MP4", "Audio only, original M4A"], "without ffmpeg");
  // Another video's, one that doesn't play, live, protected, a video without its height: none.
  assert.equal(g.ytFormats(player, "other"), null);
  assert.equal(g.ytFormats({ ...player, status: "LOGIN_REQUIRED" }, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats({ ...player, live: true }, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats({ ...player, formats: [...player.formats, { ...player.formats[1], drm: true }] }, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats({ ...player, formats: [...player.formats, { mimeType: 'video/mp4; codecs="avc1"' }] }, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats({ ...player, formats: player.formats.filter((f) => f.mimeType.startsWith("audio")) }, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats(null, "aqz-KE-bpKQ"), null);
  assert.equal(g.ytFormats("junk", "aqz-KE-bpKQ"), null);
  // A type yt-dlp doesn't list is left out, the rest kept.
  assert.equal(g.ytFormats({ ...player, formats: [...player.formats, { mimeType: "text/plain" }] }, "aqz-KE-bpKQ").formats.length, 41);

  // An HLS master playlist (resolve.rs's hls_variants_are_sized_from_their_bitrate): its variants'
  // heights and bitrates, sized from the player's length as the app sizes them.
  const hls = (text, duration) => JSON.parse(JSON.stringify(g.hlsFormats(text, duration)));
  const MASTER_HLS = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\n360/i.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720,FRAME-RATE=30.000\n720/i.m3u8\n";
  const fromHls = rowsOf(hls(MASTER_HLS, 600), true).filter((r) => r.audio === false);
  assert.deepEqual(fromHls.map((r) => [r.label, r.size]), [["720p MP4", 187500000], ["360p MP4", 60000000]], "as resolve.rs's test");
  assert.equal(hls(MASTER_HLS).formats[0].size, null, "no length: no size");
  // The sound as a rendition of its own, the variants naming their codecs: picture alone.
  const SPLIT = '#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="a",NAME="en",URI="a.m3u8"\n#EXT-X-STREAM-INF:BANDWIDTH=1280000,AVERAGE-BANDWIDTH=1000000,RESOLUTION=1280x720,CODECS="avc1.4d401f,mp4a.40.2",AUDIO="a"\nv.m3u8\n';
  const split = hls(SPLIT, 10).formats;
  assert.deepEqual(split.map((f) => [f.video, f.audio, f.height, f.tbr]), [[false, true, null, null], [true, false, 720, 1000]]);
  assert.deepEqual(rowsOf({ duration: 10, formats: split }, false).map((r) => r.label), ["Audio only, original MP4"], "without ffmpeg: the sound alone, as the app lists it");
  // A rendition group without codecs on the variant: a whole format (as yt-dlp reads it).
  assert.equal(hls(SPLIT.replace(',CODECS="avc1.4d401f,mp4a.40.2"', ""), 10).formats[1].audio, true);
  assert.equal(g.hlsFormats("#EXTM3U\n#EXTINF:4,\na.ts\n#EXT-X-ENDLIST\n", 4), null, "a media playlist");
  assert.equal(g.hlsFormats("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n", 4), null, "a variant without its height");
  assert.equal(g.hlsFormats('#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=SAMPLE-AES,URI="skd://k"\n' + MASTER_HLS.slice(8), 4), null, "protected");
  assert.equal(g.hlsFormats("<html>", 4), null);

  // A DASH manifest: each representation, its adaptation set's type where it says none.
  const MPD = `<?xml version="1.0"?><MPD type="static" mediaPresentationDuration="PT1M0S"><Period>
    <AdaptationSet mimeType="video/mp4" frameRate="60"><Representation id="1" height="1080" bandwidth="4000000"/><Representation id="2" height="720" bandwidth="2000000" frameRate="30000/1001"/></AdaptationSet>
    <AdaptationSet contentType="audio" mimeType="audio/mp4"><Representation id="3" bandwidth="128000"/></AdaptationSet>
    <AdaptationSet mimeType="text/vtt"><Representation id="4" bandwidth="1"/></AdaptationSet></Period></MPD>`;
  const dash = JSON.parse(JSON.stringify(g.dashFormats(MPD)));
  assert.equal(dash.duration, 60);
  assert.deepEqual(dash.formats.map((f) => [f.video, f.audio, f.height, Math.round(f.fps || 0), f.ext, f.size]), [
    [true, false, 1080, 60, "mp4", 30000000],
    [true, false, 720, 30, "mp4", 15000000],
    [false, true, null, 0, "m4a", 960000],
  ]);
  assert.deepEqual(rowsOf(dash, true).slice(0, 3).map((r) => [r.label, r.size]), [["1080p 60fps MP4", 30960000], ["720p MP4", 15960000], ["Audio only, original M4A", 960000]]);
  assert.equal(g.dashFormats(MPD.replace('type="static"', 'type="dynamic"')), null, "live");
  assert.equal(g.dashFormats(MPD.replace("<Period>", '<Period><ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011"/>')), null, "protected");
  assert.equal(g.dashFormats(MPD.replace(' height="1080"', "")), null, "a video without its height");
  ok("the qualities the page names: read as yt-dlp would, listed as DDM lists them, else nothing");
}

// No other download manager's name in what ships (the clean-room rule, CLAUDE.md).
{
  for (const file of readdirSync(new URL("../../extension/", import.meta.url))) {
    if (!/\.(js|html|css|json)$/.test(file)) continue;
    const text = readFileSync(new URL(`../../extension/${file}`, import.meta.url), "utf8");
    assert.ok(!/\bIDM\b|Internet Download Manager|notes\//.test(text), `${file} names IDM or the private notes`);
  }
  ok("nothing of IDM's in the extension");
}

// The right-click command in the browser's language, and every locale English's: no key English
// lacks, each with English's placeholders (the browser shows English's text for a key a locale
// doesn't have yet, until tools/extension/locales.py brings its translation), and the manifests'
// __MSG_…__ names in English.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await listeners.installed({ reason: "update" });
  const item = calls.find((c) => c[0] === "menu-item")[1];
  assert.equal(item.title, "Mit Dannex Download Manager herunterladen");
  const dir = new URL("../../extension/_locales/", import.meta.url);
  const english = JSON.parse(readFileSync(new URL("en/messages.json", dir), "utf8"));
  for (const locale of readdirSync(dir)) {
    const m = JSON.parse(readFileSync(new URL(`${locale}/messages.json`, dir), "utf8"));
    for (const key of Object.keys(m)) {
      assert.ok(english[key], `${locale}: ${key} is no longer English's`);
      assert.deepEqual(m[key].placeholders, english[key].placeholders, `${locale}: ${key}'s placeholders`);
    }
    // The texts every locale has had since the first: still all there (English unchanged).
    for (const key of ["name", "description", "menu", "menu_all", "popup_help"]) {
      assert.ok(m[key], `${locale} has ${key}`);
    }
  }
  for (const name of ["manifest.json", "manifest.firefox.json"]) {
    const text = readFileSync(new URL(`../../extension/${name}`, import.meta.url), "utf8");
    for (const [, key] of text.matchAll(/__MSG_(\w+)__/g)) assert.ok(english[key], `${name}: ${key}`);
  }
  // Every key the scripts and pages use exists in English (a key in a list or built from a
  // reason, too: each prefix's names).
  const PREFIXED = /["'`]((?:menu|status|popup|video|page|toolbar|click|options|badge|declined|tip)_\w+)["'`]/g;
  for (const file of ["background.js", "pages.js", "options.js", "options.html", "install.js", "install.html", "video.js"]) {
    const text = readFileSync(new URL(`../../extension/${file}`, import.meta.url), "utf8");
    for (const [, key] of text.matchAll(PREFIXED)) assert.ok(english[key], `${file}: ${key}`);
  }
  for (const why of ["not_installed", "forbidden", "not_running", "host_failed", "too_large", "update_app"]) {
    assert.ok(english[`status_${why}`], `status_${why}`);
  }
  // The status page's words for each state, its steps for each system and the files offered.
  for (const state of ["not_installed", "not_running", "host_failed", "forbidden", "too_large"]) assert.ok(english[`page_lead_${state}`], `page_lead_${state}`);
  for (const system of ["windows", "macos", "linux", "other"]) assert.ok(english[`page_step_${system}`] && english[`page_files_${system}`], system);
  // The badge OFF fits a toolbar button (4 characters).
  assert.ok(english.badge_off.message.length <= 4);
  ok("translations complete");
}

// #42: the browser's proxy goes with the download, so the app uses the same one: an opt-in, the
// options page's switch. Chromium can't make the "proxy" permission optional, so its extension has it
// from the install ("required") and the switch is its own setting (`proxyOn`, off as installed).
// What Chromium's proxy.settings.get says, handed over for each address: {url: proxy or PAC}.
async function chromeProxy(proxy, urls, { on = true, levelOfControl } = {}) {
  const { api, calls, listeners } = fakeApi({ firefox: false, proxy, proxyPerm: "required", proxyOn: on });
  if (levelOfControl) api.proxy.settings.get = async () => ({ value: proxy, levelOfControl });
  load(api, false);
  let id = 600;
  for (const url of urls) listeners.determining({ id: id++, url, filename: "f.zip" }, () => {});
  await settle();
  const sent = natives(calls);
  assert.equal(sent.length, urls.length, "every download handed over");
  return Object.fromEntries(sent.map((m) => [m.url, m.proxy || (m.proxyPac ? `PAC ${m.proxyPac}` : null)]));
}
{
  // A VPN extension's fixed SOCKS5 proxy: remote DNS (socks5h), as Chromium does.
  const proxy = { mode: "fixed_servers", rules: { singleProxy: { scheme: "socks5", host: "vpn.example", port: 1080 }, bypassList: ["<local>"] } };
  const { api, calls, listeners } = fakeApi({ firefox: false, proxy, proxyPerm: "required", proxyOn: true });
  load(api, false);
  listeners.determining({ id: 31, url: "https://files.example/p.zip", filename: "p.zip" }, () => {});
  await settle();
  const m = natives(calls)[0];
  assert.equal(m.proxy, "socks5h://vpn.example:1080");
  assert.equal(m.proxyLogin, undefined);
  assert.ok(!calls.some((c) => c[0] === "contains-proxy" || c[0] === "request"), "no permission call: Chrome's is required");
  ok("Chrome, switched on: the extension's SOCKS5 proxy goes with the download");
}
{
  // One server per scheme: http and https each their own, the fallback for the one not given;
  // a scheme and a port left out are Chromium's defaults (http; 80, 443, 1080).
  const per = { mode: "fixed_servers", rules: { proxyForHttp: { scheme: "http", host: "h.example", port: 3128 }, proxyForHttps: { scheme: "https", host: "s.example" } } };
  assert.deepEqual(await chromeProxy(per, ["http://x/a.zip", "https://x/b.zip"]), { "http://x/a.zip": "http://h.example:3128", "https://x/b.zip": "https://s.example:443" });
  const fallback = { mode: "fixed_servers", rules: { proxyForHttp: { host: "h.example" }, fallbackProxy: { scheme: "socks4", host: "f.example" } } };
  assert.deepEqual(await chromeProxy(fallback, ["http://x/c.zip", "https://x/d.zip"]), { "http://x/c.zip": "http://h.example:80", "https://x/d.zip": "socks4://f.example:1080" });
  const onlyHttp = { mode: "fixed_servers", rules: { proxyForHttp: { scheme: "http", host: "h.example", port: 8080 } } };
  assert.deepEqual(await chromeProxy(onlyHttp, ["http://x/e.zip", "https://x/f.zip"]), { "http://x/e.zip": "http://h.example:8080", "https://x/f.zip": null });
  const socks = { mode: "fixed_servers", rules: { singleProxy: { scheme: "socks5", host: "10.8.0.1" } } };
  assert.deepEqual(await chromeProxy(socks, ["https://x/g.zip"]), { "https://x/g.zip": "socks5h://10.8.0.1:1080" });
  const quic = { mode: "fixed_servers", rules: { singleProxy: { scheme: "quic", host: "q.example", port: 443 } } };
  assert.deepEqual(await chromeProxy(quic, ["https://x/h.zip"]), { "https://x/h.zip": null }, "a QUIC proxy, which the app can't use: nothing");
  ok("Chrome: a fixed server for each scheme, the fallback, Chromium's default ports");
}
{
  // The bypass list, read as Chromium reads it: those addresses go without the proxy.
  const P = "http://p.example:8080";
  const rules = (bypassList) => ({ mode: "fixed_servers", rules: { singleProxy: { scheme: "http", host: "p.example", port: 8080 }, bypassList } });
  const list = ["*.intranet.example", ".corp.example", "<local>", "10.0.0.0/8", "files.example:8080", "https://secure.example", "*mirror.example"];
  const urls = [
    "https://files.intranet.example/a.zip", // *.intranet.example
    "https://intranet.example/b.zip", // not *.intranet.example's
    "https://a.corp.example/c.zip", // .corp.example is *.corp.example
    "https://corp.example/d.zip", // not .corp.example's
    "http://nas/e.zip", // <local>: no dot
    "http://10.1.2.3/f.zip", // 10.0.0.0/8
    "http://11.1.2.3/g.zip", // not in it
    "http://files.example:8080/h.zip", // its port
    "http://files.example/i.zip", // another port
    "https://secure.example/j.zip", // its scheme
    "http://secure.example/k.zip", // another scheme
    "https://dl.mirror.example/l.zip", // *mirror.example
    "https://other.example/m.zip",
  ];
  const got = await chromeProxy(rules(list), urls);
  assert.deepEqual(
    urls.map((u) => got[u]),
    [null, P, null, P, null, null, P, null, P, null, P, null, P],
  );
  // Chromium never sends loopback to a proxy, unless the list has "<-loopback>".
  const local = ["http://localhost:8000/n.zip", "http://127.0.0.1/o.zip", "http://[::1]/p.zip", "http://app.localhost/q.zip"];
  const plain = await chromeProxy(rules([]), [...local, "http://[fe80::1]/r.zip", "https://feature.example/s.zip", "https://127.example/t.zip"]);
  assert.deepEqual(local.map((u) => plain[u]), [null, null, null, null], "loopback: direct");
  assert.equal(plain["http://[fe80::1]/r.zip"], null, "link-local: direct");
  assert.deepEqual([plain["https://feature.example/s.zip"], plain["https://127.example/t.zip"]], [P, P], "names that only look local");
  const forced = await chromeProxy(rules(["<-loopback>"]), local);
  assert.deepEqual(local.map((u) => forced[u]), [P, P, P, P], "<-loopback>: through the proxy");
  ok("Chrome: the bypass list (patterns, ranges, ports, schemes, <local>, loopback and <-loopback>)");
}
{
  // A proxy that asked for a login (the VPN extension answers it): flagged for the app.
  const proxy = { mode: "fixed_servers", rules: { proxyForHttps: { scheme: "https", host: "gw.vpn.example" } } };
  const { api, calls, listeners } = fakeApi({ firefox: false, proxy, proxyPerm: "required", proxyOn: true });
  load(api, false);
  await settle();
  listeners.authRequired({ isProxy: true, challenger: { host: "gw.vpn.example", port: 443 } });
  listeners.determining({ id: 33, url: "https://x/q.zip", filename: "q.zip" }, () => {});
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyLogin], ["https://gw.vpn.example:443", true]);
  ok("Chrome: a proxy that asks for a login is flagged");
}
{
  // A PAC script: inline (its data) or by its address (fetched without cookies); one that can't
  // be fetched, or isn't a PAC script, sends nothing.
  const data = 'function FindProxyForURL(u, h) { return "PROXY p:3128"; }';
  assert.deepEqual(await chromeProxy({ mode: "pac_script", pacScript: { data } }, ["https://x/r.zip"]), { "https://x/r.zip": `PAC ${data}` });
  served = { "http://wpad.example/vpn.pac": { body: data }, "http://wpad.example/page.html": { body: "<html>" } };
  const before = fetched.length;
  assert.deepEqual(await chromeProxy({ mode: "pac_script", pacScript: { url: "http://wpad.example/vpn.pac", mandatory: false } }, ["https://x/s.zip"]), { "https://x/s.zip": `PAC ${data}` });
  assert.deepEqual(fetched.slice(before).filter(([u]) => u.includes("wpad")), [["http://wpad.example/vpn.pac", "omit"]]);
  assert.deepEqual(await chromeProxy({ mode: "pac_script", pacScript: { url: "http://wpad.example/gone.pac" } }, ["https://x/t.zip"]), { "https://x/t.zip": null });
  assert.deepEqual(await chromeProxy({ mode: "pac_script", pacScript: { url: "http://wpad.example/page.html" } }, ["https://x/u.zip"]), { "https://x/u.zip": null });
  served = {};
  ok("Chrome: a PAC script, inline or by its address");
}
{
  // Direct, the system's, auto-detect: nothing (the app goes by its own Options). A policy's or
  // the command line's proxy (not_controllable) is the browser's too.
  for (const mode of ["direct", "system", "auto_detect"]) {
    assert.deepEqual(await chromeProxy({ mode }, ["https://x/v.zip"]), { "https://x/v.zip": null }, mode);
  }
  const policy = { mode: "fixed_servers", rules: { singleProxy: { scheme: "http", host: "corp-proxy", port: 8080 } } };
  assert.deepEqual(await chromeProxy(policy, ["https://x/w.zip"], { levelOfControl: "not_controllable" }), { "https://x/w.zip": "http://corp-proxy:8080" });
  ok("Chrome: direct, system and auto-detect send nothing; a policy's proxy goes");
}
{
  // As installed (the switch off): nothing read or sent, a proxy login seen included; the options page
  // switches it on (storage), then off: it follows at once. No permission call, nothing in the
  // console.
  const before = complaints.length;
  const proxy = { mode: "fixed_servers", rules: { singleProxy: { scheme: "http", host: "10.0.0.9", port: 3128 } } };
  const { api, calls, listeners, store } = fakeApi({ firefox: false, proxy, proxyPerm: "required" });
  let read = 0;
  api.proxy.settings.get = async () => (read++, { value: proxy, levelOfControl: "controlled_by_other_extensions" });
  load(api, false);
  await settle();
  listeners.authRequired({ isProxy: true, challenger: { host: "10.0.0.9", port: 3128 } });
  listeners.determining({ id: 42, url: "https://x/off.zip", filename: "off.zip" }, () => {});
  await settle();
  let m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyPac, m.proxyLogin], [undefined, undefined, undefined]);
  assert.equal(read, 0, "the browser's proxy settings never read while off");
  await api.storage.local.set({ proxyOn: true });
  await settle();
  listeners.determining({ id: 43, url: "https://x/on.zip", filename: "on.zip" }, () => {});
  await settle();
  m = natives(calls)[1];
  assert.deepEqual([m.proxy, m.proxyLogin], ["http://10.0.0.9:3128", undefined], "on: the proxy goes; the login seen while off was not kept");
  listeners.authRequired({ isProxy: true, challenger: { host: "10.0.0.9", port: 3128 } });
  await api.storage.local.set({ proxyOn: false });
  await settle();
  assert.equal(store.proxyOn, false);
  listeners.determining({ id: 44, url: "https://x/off2.zip", filename: "off2.zip" }, () => {});
  await settle();
  m = natives(calls)[2];
  assert.deepEqual([m.proxy, m.proxyPac, m.proxyLogin], [undefined, undefined, undefined], "off again: nothing");
  await api.storage.local.set({ proxyOn: true });
  listeners.determining({ id: 45, url: "https://x/on2.zip", filename: "on2.zip" }, () => {});
  await settle();
  m = natives(calls)[3];
  assert.deepEqual([m.proxy, m.proxyLogin], ["http://10.0.0.9:3128", undefined], "on again: the login seen before was forgotten when off");
  assert.equal(read, 2, "read only while on");
  assert.ok(!calls.some((c) => c[0] === "contains-proxy" || c[0] === "request" || c[0] === "remove"), "no permission call");
  assert.equal(complaints.length, before, `the console: ${JSON.stringify(complaints.slice(before))}`);
  ok("Chrome: off as installed (nothing read or sent), on and off from the options page's switch, nothing in the console");
}
{
  // Firefox, switched on (the optional permission granted): the proxy its own request for the
  // file took (proxy.onRequest extensions too).
  const { api, calls, listeners } = fakeApi({ firefox: true, proxyGranted: true });
  load(api, true);
  await settle();
  listeners.responseStarted({ url: "https://x/s.zip", proxyInfo: { type: "socks", host: "10.8.0.1", port: 1080, proxyDNS: true, username: "u" } });
  listeners.created({ id: 35, url: "https://x/s.zip", filename: "C:\\dl\\s.zip" });
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyLogin], ["socks5h://10.8.0.1:1080", true]);
  ok("Firefox: the request's own proxy goes, its login flagged");
}
{
  // Firefox, not switched on (as installed, or the request refused): as before #42, nothing read
  // and nothing sent, the browser's own proxy settings included; no complaint in the console.
  const before = complaints.length;
  const settings = { proxyType: "manual", http: "10.0.0.9:3128", httpProxyAll: true };
  const { api, calls, listeners } = fakeApi({ firefox: true, proxy: settings, proxyAnswer: false });
  let read = 0;
  api.proxy.settings.get = async () => (read++, { value: settings });
  load(api, true);
  await settle();
  listeners.responseStarted({ url: "https://x/u.zip", proxyInfo: { type: "http", host: "10.0.0.9", port: 3128 } });
  listeners.authRequired({ isProxy: true, challenger: { host: "10.0.0.9", port: 3128 } });
  listeners.created({ id: 37, url: "https://x/u.zip", filename: "C:\\dl\\u.zip" });
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyPac, m.proxyLogin], [undefined, undefined, undefined]);
  assert.equal(read, 0, "the browser's proxy settings never read");
  // Refused: still nothing.
  assert.equal(await api.permissions.request({ permissions: ["proxy"] }), false);
  listeners.created({ id: 38, url: "https://x/u.zip", filename: "C:\\dl\\u2.zip" });
  await settle();
  assert.equal(natives(calls)[1].proxy, undefined);
  assert.equal(complaints.length, before, `the console: ${JSON.stringify(complaints.slice(before))}`);
  ok("Firefox, not switched on or refused: no proxy read or sent, nothing in the console");
}
{
  // Firefox: switched on (granted while it runs), then off (removed): it follows at once, and
  // what it saw while on is forgotten.
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  await settle();
  assert.equal(await api.permissions.request({ permissions: ["proxy"] }), true);
  await settle();
  listeners.responseStarted({ url: "https://x/v.zip", proxyInfo: { type: "http", host: "10.0.0.9", port: 3128 } });
  listeners.created({ id: 39, url: "https://x/v.zip", filename: "C:\\dl\\v.zip" });
  await settle();
  assert.equal(natives(calls)[0].proxy, "http://10.0.0.9:3128", "granted: the request's proxy goes");
  listeners.responseStarted({ url: "https://x/w.zip", proxyInfo: { type: "http", host: "10.0.0.9", port: 3128 } });
  await api.permissions.remove({ permissions: ["proxy"] });
  await settle();
  listeners.created({ id: 40, url: "https://x/w.zip", filename: "C:\\dl\\w.zip" });
  await settle();
  assert.equal(natives(calls)[1].proxy, undefined, "removed: nothing, not even what it saw before");
  ok("Firefox: switched on, then off: the proxy goes only while on");
}
{
  // A Chromium manifest without the "proxy" permission (0.2.4's, the API absent), even with the
  // switch's setting there: as before #42, and the extension never asks about the permission;
  // nothing in the console.
  const before = complaints.length;
  const { api, calls, listeners } = fakeApi({ firefox: false, proxyOn: true });
  load(api, false);
  listeners.authRequired && listeners.authRequired({ isProxy: true, challenger: { host: "p", port: 1 } });
  listeners.determining({ id: 41, url: "https://x/y.zip", filename: "y.zip" }, () => {});
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyPac, m.proxyLogin], [undefined, undefined, undefined]);
  assert.ok(!calls.some((c) => c[0] === "contains-proxy" || c[0] === "request"), "no permission call");
  assert.equal(listeners.authRequired, undefined, "no proxy-login listener");
  assert.equal(complaints.length, before, `the console: ${JSON.stringify(complaints.slice(before))}`);
  ok("Chromium without the proxy permission: no reading, no permission call, nothing in the console");
}
{
  // No proxy in use (switched on): nothing extra.
  const { api, calls, listeners } = fakeApi({ firefox: false, proxyPerm: "required", proxyOn: true });
  load(api, false);
  listeners.determining({ id: 36, url: "https://x/t.zip", filename: "t.zip" }, () => {});
  await settle();
  const m = natives(calls)[0];
  assert.deepEqual([m.proxy, m.proxyPac, m.proxyLogin], [undefined, undefined, undefined]);
  ok("no proxy: nothing extra goes");
}

// ---- #208: why a download stayed with the browser, and older apps --------------------------

// What the status page hears.
const pageStatus = (listeners) =>
  new Promise((r) => listeners.message({ type: "status" }, { id: "ours", url: "chrome-extension://ours/install.html" }, (v) => r(JSON.parse(JSON.stringify(v)))));

// The host refuses by Options (its rules changed since): the rule it names is kept for the status page
// (the host logged it; no notice); a download taken later clears it.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false, reply: { ok: false, why: "options", declined: "browser", error: "left to the browser by Options (Browser)" } });
  load(api, false);
  let kept = false;
  listeners.determining({ id: 60, url: "https://x/a.zip", filename: "a.zip" }, () => (kept = true));
  await settle();
  assert.ok(kept);
  assert.deepEqual(notices(calls), []);
  const s = await pageStatus(listeners);
  assert.deepEqual([s.declined.why, s.declined.name, s.declined.site, s.connected], ["browser", "a.zip", "x", true]);
  ok("#208: the host's refusal is kept for the status page, by its rule");
}
// #376: the app hands the download back (the user chose Download with the browser for it after
// the site's bot check): the browser goes on with its own download, nothing is cancelled, the
// status page says why under its own code, no notice goes back (the host logged it), and the
// connection stays fine. Firefox's paused download is resumed; its page load is left alone.
{
  const handedBack = { ok: false, why: "options", declined: "handed-back", error: "handed back to the browser by DDM after the site's bot check" };
  const { api, calls, listeners } = fakeApi({ firefox: false, reply: handedBack });
  load(api, false);
  let kept = false;
  listeners.determining({ id: 68, url: "https://x/a.zip", finalUrl: "https://cdn.x/a.zip", filename: "a.zip" }, () => (kept = true));
  await settle();
  assert.ok(kept, "the browser goes on with it");
  assert.deepEqual(steps(calls), [], "nothing cancelled or erased");
  assert.deepEqual(notices(calls), []);
  const sent = natives(calls)[0];
  assert.deepEqual([sent.url, sent.finalUrl, sent.explicit], ["https://x/a.zip", "https://cdn.x/a.zip", undefined]);
  const s = await pageStatus(listeners);
  assert.deepEqual([s.declined.why, s.declined.name, s.declined.site, s.connected, s.why], ["handed-back", "a.zip", "x", true, null]);
  const ff = fakeApi({ firefox: true, reply: handedBack });
  load(ff.api, true);
  ff.listeners.created({ id: 69, url: "https://x/b.zip", filename: "C:\\dl\\b.zip" });
  await settle();
  assert.deepEqual(steps(ff.calls), ["pause", "resume"]);
  assert.equal((await pageStatus(ff.listeners)).declined.why, "handed-back");
  ok("#376: a download handed back stays with the browser, said on the status page");
}
{
  const { api, listeners, store } = fakeApi({ firefox: false });
  load(api, false);
  store.declined = { why: "file-type", name: "p.png", site: "x", at: 1 };
  listeners.determining({ id: 61, url: "https://x/b.zip", filename: "b.zip" }, () => {});
  await settle();
  assert.equal(store.declined, undefined, "a download taken clears it");
  assert.equal((await pageStatus(listeners)).declined, null);
  ok("#208: a download taken clears the last refusal");
}

// An app before #208 (0.2.4, 0.3.0): its rules say only the types (0.2.4) or no reason for
// `capture` (0.3.0), its refusal only in the error text, and it takes a notice for an address it
// can't download. Downloads go as before, the status page still names the rule, the status stays.
{
  const old024 = { ok: true, types: "ZIP EXE" };
  const notice024 = { ok: false, error: "not an http(s) or ftp(s) URL" };
  const { api, calls, listeners } = fakeApi({ firefox: false, rulesReply: old024, noticeReply: notice024 });
  load(api, false);
  listeners.determining({ id: 62, url: "https://x/a.zip", filename: "a.zip" }, () => {});
  await settle();
  assert.equal(natives(calls).length, 1, "a listed type is offered");
  listeners.determining({ id: 63, url: "https://x/p.png", filename: "p.png" }, () => {});
  await settle();
  assert.deepEqual(notices(calls).map((n) => n.why), ["file-type"]);
  const s = await pageStatus(listeners);
  assert.ok(s.connected && !s.why, `the older host's answer to a notice changes no status: ${JSON.stringify(s)}`);
  assert.equal(s.declined.why, "file-type");
  ok("#208: a 0.2.4 app: downloads as before, a notice it can't read is harmless");
}
{
  const { api, listeners } = fakeApi({ firefox: true, reply: { ok: false, error: "left to the browser by Options (Small)" } });
  load(api, true);
  listeners.created({ id: 64, url: "https://x/c.zip", filename: "C:\\dl\\c.zip" });
  await settle();
  assert.equal((await pageStatus(listeners)).declined.why, "small");
  const off = fakeApi({ firefox: false, rules: { capture: false } });
  load(off.api, false);
  off.listeners.determining({ id: 65, url: "https://x/d.zip", filename: "d.zip" }, () => {});
  await settle();
  assert.deepEqual(notices(off.calls).map((n) => n.why), ["browser"], "0.3.0: capture off, no reason: the browser's box");
  const integration = fakeApi({ firefox: false, rules: { capture: false, captureOff: "integration-off" } });
  load(integration.api, false);
  integration.listeners.determining({ id: 66, url: "https://x/e.zip", filename: "e.zip" }, () => {});
  await settle();
  assert.deepEqual(natives(integration.calls), []);
  assert.deepEqual(notices(integration.calls).map((n) => n.why), ["integration-off"]);
  ok("#208: an older host's refusal read from its words; capture off by the reason given");
}

// A private window's: neither its name nor its site is kept or sent.
{
  const { api, calls, listeners, store } = fakeApi({ firefox: false });
  load(api, false);
  listeners.determining({ id: 67, url: "https://secret.test/p.png", filename: "p.png", incognito: true }, () => {});
  await settle();
  assert.deepEqual([store.declined.why, store.declined.name, store.declined.site], ["file-type", "", ""]);
  const sent = calls.find((c) => c[0] === "native" && c[2].declined)[2];
  assert.ok(sent.private && !JSON.stringify(sent).includes("secret"));
  ok("#208: a private window's refusal names nothing");
}

// The codes agree end to end: the app's (mdm_ipc::capture::Declined::code), the extension's own
// ("site-access"), and a status page text for each.
{
  const rust = readFileSync(new URL("../../crates/ipc/src/capture.rs", import.meta.url), "utf8");
  const body = /pub fn code\(self\)[\s\S]*?\n    \}\n/.exec(rust)[0];
  const codes = [...body.matchAll(/=> "([a-z-]+)"/g)].map((m) => m[1]);
  assert.deepEqual(codes, ["integration-off", "browser", "file-type", "site", "address", "small"]);
  const en = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
  // The app's own code for a download handed back to the browser (#376, mdm_ipc::HANDED_BACK).
  const ipc = readFileSync(new URL("../../crates/ipc/src/lib.rs", import.meta.url), "utf8");
  const handedBack = /pub const HANDED_BACK: &str = "([a-z-]+)";/.exec(ipc)[1];
  assert.equal(handedBack, "handed-back");
  for (const c of [...codes, "site-access", handedBack]) assert.ok(en[`declined_${c.replace(/-/g, "_")}`], `a status page text for ${c}`);
  assert.ok(en.popup_declined && en.popup_declined_unnamed);
  const bg = code;
  for (const name of ["IntegrationOff", "Browser", "FileType", "Site", "Address", "Small"]) assert.ok(bg.includes(`${name}: "`), `older hosts' ${name}`);
  ok("#208: refusal codes, the app's and the status page's, agree");
}

// #208: Firefox (and Waterfox, which always asks first) creates a download only after its "What
// should … do with this file?" dialog, having downloaded the whole file behind it. The answer to
// the page load is caught before that instead: offered with its name (Content-Disposition), size,
// type and the page that linked it, then cancelled (no dialog, no browser download); the
// download it would still become is left alone, never asked twice.
const answer = (url, headers, more = {}) => ({
  requestId: "r1",
  url,
  method: "GET",
  type: "main_frame",
  tabId: 3,
  statusCode: 200,
  responseHeaders: Object.entries(headers).map(([name, value]) => ({ name, value })),
  ...more,
});
// The listener's answer, as a plain object (it is made in the sandbox).
const held = async (listeners, d) => JSON.parse(JSON.stringify(await listeners.headersReceived(d)));
const ask = (listeners) => (m, sender) => new Promise((r) => listeners.message(m, sender, r));
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  await settle();
  listeners.beforeSendHeaders({ requestId: "r1", method: "GET", url: "https://dl.test/get?id=1", requestHeaders: [{ name: "Referer", value: "https://site.test/page" }] });
  const res = await held(listeners, 
    answer(
      "https://dl.test/get?id=1",
      {
        "Content-Type": "application/octet-stream",
        "Content-Disposition": "attachment; filename=\"fallback.zip\"; filename*=UTF-8''r%C3%A9sum%C3%A9%20v2.zip",
        "Content-Length": "4096",
      },
      { cookieStoreId: "firefox-container-3" },
    ),
  );
  assert.deepEqual(res, { cancel: true });
  const m = natives(calls)[0];
  assert.deepEqual(
    [m.url, m.filename, m.size, m.mime, m.referrer, m.cookies],
    ["https://dl.test/get?id=1", "résumé v2.zip", 4096, "application/octet-stream", "https://site.test/page", "sid=1"],
  );
  assert.deepEqual(calls.find((c) => c[0] === "cookies")[1], { url: "https://dl.test/get?id=1", storeId: "firefox-container-3" });
  // Were the browser to create the download all the same: left alone, not asked twice.
  listeners.created({ id: 30, url: "https://dl.test/get?id=1", filename: "C:\\dl\\résumé v2.zip" });
  await settle();
  assert.equal(natives(calls).length, 1);
  assert.deepEqual(steps(calls), []);
  ok("#208 Firefox: a download caught from its answer, before the browser's dialog");
}
// #447: Firefox hands a header's value over byte for byte, each byte one character. A name sent as
// raw UTF-8 is read as UTF-8; bytes that aren't valid UTF-8 (ISO-8859-1) and a real percent sign stay.
{
  const asBytes = (text) => String.fromCharCode(...new TextEncoder().encode(text));
  for (const [value, want] of [
    [`attachment; filename="${asBytes("تقرير.pdf")}"`, "تقرير.pdf"],
    [`attachment; filename="${asBytes("été à Paris.pdf")}"`, "été à Paris.pdf"],
    [`attachment; filename="${asBytes("日本語.pdf")}"`, "日本語.pdf"],
    ["attachment; filename=\"plain report.pdf\"", "plain report.pdf"],
    // ISO-8859-1 bytes (é alone is not valid UTF-8).
    ["attachment; filename=\"caf\u00e9.pdf\"", "café.pdf"],
    // Percent-encoded UTF-8 in the plain name, as Firefox and the app read it.
    ["attachment; filename=\"%D8%AA%D9%82%D8%B1%D9%8A%D8%B1.pdf\"", "تقرير.pdf"],
    ["attachment; filename=\"100%25.pdf\"", "100%25.pdf"],
    ["attachment; filename=\"50%.pdf\"", "50%.pdf"],
    // RFC 8187 keeps winning, ASCII as it comes.
    ["attachment; filename=\"fallback.pdf\"; filename*=UTF-8''%D8%AA%D9%82%D8%B1%D9%8A%D8%B1.pdf", "تقرير.pdf"],
  ]) {
    const { api, calls, listeners } = fakeApi({ firefox: true });
    load(api, true);
    await settle();
    await held(listeners, answer("https://dl.test/get?id=2", { "Content-Type": "application/octet-stream", "Content-Disposition": value }));
    assert.equal(natives(calls)[0].filename, want, value);
  }
  ok("#447 Firefox: a raw UTF-8 file name in a header reaches the app readable, other names as they were");
}
// A download taken from its answer (#208) is cancelled, so its response never starts: the proxy a
// VPN add-on chose for it (proxy.onRequest, in the answer's proxyInfo) still goes with it.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, proxyGranted: true });
  load(api, true);
  await settle();
  const res = await held(
    listeners,
    answer("https://dl.test/vpn.zip", { "Content-Type": "application/zip", "Content-Disposition": "attachment" }, { proxyInfo: { type: "socks", host: "10.8.0.1", port: 1080, proxyDNS: true } }),
  );
  assert.deepEqual(res, { cancel: true });
  assert.deepEqual([natives(calls)[0].proxy, natives(calls)[0].proxyLogin], ["socks5h://10.8.0.1:1080", undefined]);
  ok("#208 Firefox: a download taken from its answer keeps its request's proxy (a VPN add-on's)");
}
// Firefox's own proxy settings never cover loopback addresses (allow_hijacking_localhost is off):
// nothing is sent for them, the proxy for anything else.
{
  const settings = { proxyType: "manual", http: "10.0.0.9:3128", httpProxyAll: true, passthrough: "" };
  const { api, calls, listeners } = fakeApi({ firefox: true, proxyGranted: true, proxy: settings });
  api.proxy.settings.get = async () => ({ value: settings });
  load(api, true);
  await settle();
  const urls = ["http://127.0.0.1:8231/a.zip", "http://localhost/b.zip", "http://[::1]/c.zip", "http://127.9.9.9/d.zip", "https://x.test/e.zip"];
  urls.forEach((url, i) => listeners.created({ id: 60 + i, url, filename: `C:\dl\f${i}.zip` }));
  await settle();
  assert.deepEqual(natives(calls).map((m) => m.proxy), [undefined, undefined, undefined, undefined, "http://10.0.0.9:3128"]);
  ok("Firefox: loopback addresses skip its proxy settings' proxy, as Firefox itself does");
}
// A type the app doesn't take: the browser goes on (its dialog shows), DDM's log hears why once,
// and the download after the dialog isn't paused or asked again.
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  await settle();
  const res = await held(listeners, answer("https://dl.test/data.xyz", { "Content-Type": "application/octet-stream", "Content-Disposition": "attachment" }));
  assert.deepEqual(res, {});
  listeners.created({ id: 31, url: "https://dl.test/data.xyz", filename: "C:\\dl\\data.xyz" });
  await settle();
  assert.deepEqual(natives(calls), []);
  assert.deepEqual(notices(calls), [{ why: "file-type", name: "data.xyz", site: "dl.test" }]);
  assert.deepEqual(steps(calls), []);
  ok("#208 Firefox: a type not taken goes on to the browser, said once");
}
// Pages, pictures, feeds, add-ons, redirects, pieces and errors are never held; an inline PDF
// opens when Options don't take the files the browser would open; a type the browser can't show
// is a download even without "attachment".
{
  const { api, calls, listeners } = fakeApi({ firefox: true, rules: { opened: false } });
  load(api, true);
  await settle();
  for (const [url, headers, more] of [
    ["https://x.test/", { "Content-Type": "text/html; charset=utf-8" }],
    ["https://x.test/p.png", { "Content-Type": "image/png" }],
    ["https://x.test/doc.pdf", { "Content-Type": "application/pdf" }],
    ["https://x.test/a.json", { "Content-Type": "application/json" }],
    ["https://x.test/feed", { "Content-Type": "application/rss+xml" }],
    ["https://x.test/addon.xpi", { "Content-Type": "application/x-xpinstall" }],
    ["https://x.test/old.zip", { "Content-Type": "application/zip", Location: "https://x.test/new.zip" }, { statusCode: 302 }],
    ["https://x.test/part.zip", { "Content-Type": "application/zip" }, { statusCode: 206 }],
    ["https://x.test/gone.zip", { "Content-Type": "application/zip" }, { statusCode: 404 }],
    ["https://x.test/untyped", {}],
  ]) {
    assert.deepEqual(await held(listeners, answer(url, headers, more)), {}, url);
  }
  assert.deepEqual(natives(calls), []);
  assert.deepEqual(notices(calls), []);
  assert.deepEqual(await held(listeners, answer("https://x.test/dl/setup.exe", { "Content-Type": "application/x-msdownload", "Content-Length": "10" })), { cancel: true });
  assert.equal(natives(calls)[0].filename, "setup.exe");
  ok("#208 Firefox: pages, pictures, redirects, inline PDFs untouched; an unshowable type taken");
}
// A PDF the browser would show is taken when Options say so; the app saying no lets the browser
// go on.
{
  const taken = fakeApi({ firefox: true });
  load(taken.api, true);
  await settle();
  assert.deepEqual(await held(taken.listeners, answer("https://x.test/doc.pdf", { "Content-Type": "application/pdf" })), { cancel: true });
  const refused = fakeApi({ firefox: true, reply: { ok: false } });
  load(refused.api, true);
  await settle();
  assert.deepEqual(await held(refused.listeners, answer("https://x.test/a.zip", { "Content-Type": "application/zip" })), {});
  assert.equal(natives(refused.calls).length, 1);
  ok("#208 Firefox: a PDF taken when Options say so; the app's no lets the browser go on");
}
// Alt held, Options' excluded sites, a file under the minimum size, a form that can't be posted
// again, paused: the browser keeps it, nothing asked.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, rules: { excludedSites: "*.skip.test", minKb: 100 } });
  load(api, true);
  await settle();
  const send = ask(listeners);
  const zip = { "Content-Type": "application/zip" };
  await send({ type: "bypass", url: "https://x.test/alt.zip" }, { id: "ours", url: "https://x.test/", tab: { url: "https://x.test/" } });
  assert.deepEqual(await held(listeners, answer("https://x.test/alt.zip", zip)), {});
  assert.deepEqual(await held(listeners, answer("https://dl.skip.test/a.zip", zip)), {});
  assert.deepEqual(await held(listeners, answer("https://x.test/small.zip", { ...zip, "Content-Length": "2048" })), {});
  listeners.beforeRequest({ method: "POST", url: "https://x.test/upload", requestId: "p1", requestBody: { raw: [{ file: "C:\\f" }] } });
  const upload = answer("https://x.test/upload", { ...zip, "Content-Disposition": "attachment; filename=r.zip" }, { method: "POST", requestId: "p1" });
  assert.deepEqual(await held(listeners, upload), {});
  assert.deepEqual(natives(calls), []);
  await send({ type: "pause", paused: true }, { id: "ours", url: "chrome-extension://ours/options.html" });
  assert.deepEqual(await held(listeners, answer("https://x.test/paused.zip", zip)), {});
  assert.deepEqual(natives(calls), []);
  ok("#208 Firefox: Alt, excluded sites, small files, file uploads and pause stay the browser's");
}
// A private window's download goes as one; a tab opened only for it closes with it.
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  await settle();
  listeners.tabCreated({ id: 12 });
  const res = await held(listeners, answer("https://x.test/p.zip", { "Content-Type": "application/zip" }, { incognito: true, tabId: 12 }));
  assert.deepEqual(res, { cancel: true });
  await settle();
  assert.equal(natives(calls)[0].private, true);
  assert.ok(calls.some((c) => c[0] === "closeTab" && c[1] === 12));
  ok("#208 Firefox: private stays private; a tab opened for the download closes");
}
// Chromium has its own pause (onDeterminingFilename): no answer is held there.
{
  const { api, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  assert.equal(listeners.headersReceived, undefined);
  ok("#208 Chromium: answers aren't held");
}
// #603: only a tokenized Colab runtime-proxy transfer stays with the browser. A normal
// download linked from a Colab notebook is still offered to DDM.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  let suggested;
  listeners.determining(
    {
      id: 91,
      url: "https://us-west1-colab.googleusercontent.com/files/out.zip?colab-runtime-proxy-token=secret",
      finalUrl: "https://us-west1-colab.googleusercontent.com/files/out.zip?colab-runtime-proxy-token=secret",
      referrer: "https://colab.research.google.com/drive/abc",
      filename: "out.zip",
      totalBytes: 1024,
      mime: "application/zip",
    },
    (...args) => (suggested = args),
  );
  await settle();
  assert.deepEqual(suggested, [], "the browser keeps the authenticated runtime-proxy download");
  assert.equal(natives(calls).length, 0);
  ok("#603 Chromium: tokenized Colab runtime-proxy download stays in the browser");
}
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  listeners.determining(
    {
      id: 92,
      url: "https://github.com/example/data/archive/refs/heads/main.zip",
      finalUrl: "https://github.com/example/data/archive/refs/heads/main.zip",
      referrer: "https://colab.research.google.com/drive/abc",
      filename: "data.zip",
      totalBytes: 4096,
      mime: "application/zip",
    },
    () => {},
  );
  await settle();
  assert.equal(natives(calls).length, 1, "ordinary notebook links remain capturable");
  ok("#603 Chromium: a GitHub file linked from Colab is still handed to DDM");
}
{
  const { api, calls, listeners } = fakeApi({ firefox: true });
  load(api, true);
  await settle();
  const url = "https://us-west1.prod.r.colab.dev/files/out.zip?colab-runtime-proxy-token=secret";
  listeners.beforeSendHeaders({
    requestId: "r-colab",
    method: "GET",
    type: "main_frame",
    url,
    originUrl: "https://colab.research.google.com/drive/abc",
    requestHeaders: [],
  });
  const response = await held(
    listeners,
    answer(
      url,
      {
        "Content-Type": "application/zip",
        "Content-Disposition": "attachment; filename=out.zip",
        "Content-Length": "1024",
      },
      { requestId: "r-colab" },
    ),
  );
  assert.deepEqual(response, {});
  assert.equal(natives(calls).length, 0, "Firefox leaves the authenticated transfer to the browser");
  ok("#603 Firefox: tokenized Colab runtime-proxy download stays in the browser");
}

// A token alone is not enough: unrelated hosts must still be captured.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  listeners.determining(
    {
      id: 93,
      url: "https://downloads.example/data.zip?colab-runtime-proxy-token=not-colab",
      finalUrl: "https://downloads.example/data.zip?colab-runtime-proxy-token=not-colab",
      referrer: "https://colab.research.google.com/drive/abc",
      filename: "data.zip",
      totalBytes: 4096,
      mime: "application/zip",
    },
    () => {},
  );
  await settle();
  assert.equal(natives(calls).length, 1, "a token on a non-proxy host must not bypass DDM");
  ok("#603 Chromium: tokenized non-Colab URLs remain capturable");
}
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await settle();
  listeners.determining(
    {
      id: 94,
      url: "https://prod.r.colab.dev/files/out.zip",
      finalUrl: "https://prod.r.colab.dev/files/out.zip",
      referrer: "https://colab.research.google.com/drive/abc",
      filename: "out.zip",
      totalBytes: 4096,
      mime: "application/zip",
    },
    () => {},
  );
  await settle();
  assert.equal(natives(calls).length, 1, "a proxy host without its token must not bypass DDM");
  ok("#603 Chromium: an un-tokenized Colab host remains capturable");
}

// The manifests (#42): "proxy" optional in Firefox's, required in Chrome's (it can't be optional
// there); the options page's switch: the permission in Firefox, the extension's own setting in
// Chrome.
{
  const chrome = JSON.parse(readFileSync(new URL("../../extension/manifest.json", import.meta.url), "utf8"));
  const firefox = JSON.parse(readFileSync(new URL("../../extension/manifest.firefox.json", import.meta.url), "utf8"));
  assert.ok((chrome.permissions || []).includes("proxy") && !(chrome.optional_permissions || []).includes("proxy"), "Chrome: proxy required");
  assert.ok(!(firefox.permissions || []).includes("proxy"), "Firefox: proxy not required");
  assert.deepEqual(firefox.optional_permissions, ["proxy"], "Firefox: proxy optional");
  const options = readFileSync(new URL("../../extension/options.js", import.meta.url), "utf8");
  assert.ok(/optional_permissions \|\| \[\]\)\.includes\("proxy"\)/.test(options), "Firefox's switch where proxy is optional");
  assert.ok(/permissions\.request\(PROXY\)/.test(options) && /permissions\.remove\(PROXY\)/.test(options), "asked for and taken back on the switch");
  assert.ok(/storage\.local\s*\.get\(\{ proxyOn: false \}\)/.test(options) && /storage\.local\.set\(\{ proxyOn: box\.checked \}\)/.test(options), "Chrome's switch: its own setting");
  assert.ok(/const PROXY_SWITCH = "proxyOn"/.test(code), "the same setting in background.js");
  const en = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
  assert.ok(en.popup_proxy && en.popup_proxy_title && en.popup_proxy_title_chromium, "the switch's texts");
  ok("manifests: proxy optional in Firefox, required in Chrome; the options page's switch in each");
}
// Another DDM (#157): the file types caught for it are the app's defaults, and every message
// the pairing can end with is there.
{
  const capture = readFileSync(new URL("../../crates/ipc/src/capture.rs", import.meta.url), "utf8");
  const app = /pub const DEFAULT_AUTO_TYPES: &str = "([^"]+)"/.exec(capture)[1];
  const ours = /const DEFAULT_TYPES =\s*"([^"]+)"/.exec(code)[1];
  assert.equal(ours, app, "DEFAULT_TYPES is the app's DEFAULT_AUTO_TYPES");
  const english = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
  for (const why of ["address", "unreachable", "wrong_code", "no_code", "too_soon", "failed", "device_name"]) {
    assert.ok(english[`remote_${why}`], `remote_${why}`);
  }
  assert.ok(english.status_remote, "status_remote");
  ok("another DDM: the app's file types, every pairing message");
}
// Safari has no downloads API: the extension still starts, and a link page.js holds (a file type
// Options lists) is offered to the app as a browser's download is; the app taking it is told to
// page.js, which lets the click go on when it isn't.
{
  const { api, calls, listeners } = fakeApi({ firefox: true, safari: true });
  load(api, true);
  const send = (m, sender) => new Promise((r) => listeners.message(m, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  const sender = { id: "ours", frameId: 0, url: "https://page.test/", tab: { id: 3, url: "https://page.test/dl", incognito: false } };
  assert.deepEqual(await send({ type: "link", url: "https://x.test/files/a.zip" }, sender), { taken: true });
  const m = natives(calls)[0];
  assert.deepEqual([m.url, m.filename, m.referrer, m.cookies], ["https://x.test/files/a.zip", "a.zip", "https://page.test/dl", "sid=1"]);
  assert.ok(!("explicit" in m), "Options' rules apply, as for the browser's own downloads");
  // Not a listed type: the click goes on, and DDM's log hears why.
  calls.length = 0;
  assert.deepEqual(await send({ type: "link", url: "https://x.test/page.html" }, sender), { taken: false });
  await settle();
  assert.deepEqual(natives(calls), []);
  assert.deepEqual(notices(calls), [{ why: "file-type", name: "page.html", site: "x.test" }]);
  // A `download` name (page.js sends it) decides the type and names the file.
  calls.length = 0;
  assert.deepEqual(await send({ type: "link", url: "https://x.test/dl?id=4", name: "b.iso" }, sender), { taken: true });
  assert.equal(natives(calls)[0].filename, "b.iso");
  // An address's own name reaches the app readable, not percent-encoded.
  calls.length = 0;
  assert.deepEqual(await send({ type: "link", url: "https://x.test/f/My%20Report.zip?x=1" }, sender), { taken: true });
  assert.equal(natives(calls)[0].filename, "My Report.zip");
  calls.length = 0;
  assert.deepEqual(await send({ type: "link", url: "https://x.test/f/bad%ZZ.zip" }, sender), { taken: true });
  assert.equal(natives(calls)[0].filename, "bad%ZZ.zip", "not percent-encoding: left as it is");
  // page.js's "rules" at a page's start: fresh ones aren't asked for again.
  calls.length = 0;
  assert.deepEqual(await send({ type: "rules" }, sender), { ok: true });
  await settle();
  assert.deepEqual(natives(calls), []);
  // A PDF is the browser's own to open unless Options say to take such files too.
  const closed = fakeApi({ firefox: true, safari: true, rules: { opened: false } });
  load(closed.api, true);
  const closedAsk = (m) => new Promise((r) => closed.listeners.message(m, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(await closedAsk({ type: "link", url: "https://x.test/d.pdf" }, sender), { taken: false });
  assert.deepEqual(await closedAsk({ type: "link", url: "https://x.test/d.zip" }, sender), { taken: true });
  // The app says no: the click goes on.
  const refused = fakeApi({ firefox: true, safari: true, reply: { ok: false } });
  load(refused.api, true);
  const ask = (m, sender) => new Promise((r) => refused.listeners.message(m, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(await ask({ type: "link", url: "https://x.test/b.iso" }, sender), { taken: false });
  // Alt held: the browser's own (page.js sends `bypass`).
  await ask({ type: "bypass", url: "https://x.test/c.zip" }, sender);
  assert.deepEqual(await ask({ type: "link", url: "https://x.test/c.zip" }, sender), { taken: false });
  ok("Safari: no downloads API; a held link is offered to the app, else the click goes on");
}

// page.js, in a page: Safari holds a plain click on a link to a listed file type, tells the app,
// and (when it isn't taken) clicks again; Alt and other types are never held.
{
  const pageCode = readFileSync(new URL("../../extension/page.js", import.meta.url), "utf8");
  const run = async ({ safari, taken, types = "ZIP ISO R0*", capture = true }) => {
    const sent = [];
    const handlers = {};
    class Element {
      constructor(href, attrs = {}) {
        Object.assign(this, { href, attrs });
        this.clicks = 0;
      }
      closest() {
        return this;
      }
      getAttribute(n) {
        return this.attrs[n] || "";
      }
      click() {
        this.dispatchEvent(new MouseEvent("click", {}));
      }
      dispatchEvent(ev) {
        this.clicks++;
        this.again = ev;
        handlers.click({ type: "click", button: 0, isTrusted: false, target: this, preventDefault() {} });
      }
    }
    class MouseEvent {
      constructor(type, init) {
        Object.assign(this, { type }, init);
      }
    }
    const stored = { rules: { types, capture } };
    const api = {
      runtime: {
        id: "ours",
        getManifest: () => (safari ? { browser_specific_settings: { safari: {} } } : {}),
        sendMessage: async (m) => (sent.push(JSON.parse(JSON.stringify(m))), m.type === "link" ? { taken } : { ok: true }),
      },
      storage: { local: { get: async () => stored }, onChanged: { addListener() {}, removeListener() {} } },
    };
    vm.runInNewContext(pageCode, {
      browser: api,
      Element,
      MouseEvent,
      URL,
      Promise,
      addEventListener: (n, f) => (handlers[n] = f),
      removeEventListener() {},
    });
    await settle();
    const click = (href, extra = {}, attrs) => {
      const a = new Element(href, attrs);
      const e = { type: "click", button: 0, isTrusted: true, altKey: false, target: a, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...extra };
      handlers.click(e);
      return { a, e };
    };
    return { click, sent, settle };
  };
  let t = await run({ safari: true, taken: true });
  let { a, e } = t.click("https://x.test/f/a.zip");
  await t.settle();
  assert.ok(e.defaultPrevented && a.clicks === 0);
  // At the page's start it has the rules refreshed when old (Safari's background sees no downloads).
  assert.deepEqual(t.sent, [{ type: "rules" }, { type: "link", url: "https://x.test/f/a.zip" }]);
  // R0* is a wildcard; the type is the last name's, the query is not part of it.
  ({ e } = t.click("https://x.test/f/a.r07?x=1.html"));
  assert.ok(e.defaultPrevented);
  ({ e } = t.click("https://x.test/f/page.html"));
  assert.ok(!e.defaultPrevented, "not a listed type");
  ({ e } = t.click("https://x.test/f/a.zip", { altKey: true }));
  assert.ok(!e.defaultPrevented, "Alt: the browser's");
  assert.deepEqual(t.sent.at(-1), { type: "bypass", url: "https://x.test/f/a.zip" });
  ({ e } = t.click("https://x.test/dl?id=4", {}, { download: "b.iso" }));
  assert.ok(e.defaultPrevented, "a download attribute names the type");
  assert.deepEqual(t.sent.at(-1), { type: "link", url: "https://x.test/dl?id=4", name: "b.iso" }, "and is sent with it");
  ({ e } = t.click("https://x.test/f/a.zip", { button: 1 }));
  assert.ok(!e.defaultPrevented, "only the primary button");
  // The app doesn't take it: the same link is clicked again, untrusted, and let through.
  t = await run({ safari: true, taken: false });
  ({ a, e } = t.click("https://x.test/f/a.zip"));
  await t.settle();
  assert.ok(e.defaultPrevented && a.clicks === 1, "clicked again");
  // Cmd held (a new tab): the click let through holds it too.
  ({ a } = t.click("https://x.test/f/a.zip", { metaKey: true, shiftKey: false, ctrlKey: false }));
  await t.settle();
  assert.ok(a.clicks === 1 && a.again.metaKey === true && a.again.bubbles, "Cmd kept");
  assert.equal(t.sent.filter((m) => m.type === "link").length, 2, "the clicks let through are not held");
  // Capture off in Options, or another browser: nothing held.
  t = await run({ safari: true, taken: true, capture: false });
  assert.ok(!t.click("https://x.test/f/a.zip").e.defaultPrevented);
  t = await run({ safari: false, taken: true });
  assert.ok(!t.click("https://x.test/f/a.zip").e.defaultPrevented, "Chrome and Firefox take it from the downloads API");
  ok("Safari: page.js holds a click on a listed file type, never Alt or other types");
}

// Right-click on a page: all its pictures, or all its videos and sounds (with the files its players
// played), to DDM's Link grabber, as "Download all links" sends a page's links.
{
  const tab = { id: 9, url: "https://gallery.test/p", incognito: false };
  const { api, calls, listeners } = fakeApi({ firefox: false, pageMediaResult: ["https://img.test/1.jpg", "https://img.test/2.png"] });
  load(api, false);
  await listeners.menu({ menuItemId: "mdm-images", pageUrl: "https://gallery.test/p", frameId: 0 }, tab);
  await settle();
  assert.deepEqual(calls.filter((c) => c[0] === "pageMedia").map((c) => c[1]), [["images"]]);
  let m = natives(calls).at(-1);
  assert.deepEqual([m.links, m.referrer, m.cookies], [["https://img.test/1.jpg", "https://img.test/2.png"], "https://gallery.test/p", "sid=1"]);
  // Videos and sounds: the page's, then the files its players played without naming them in the
  // page; a stream's manifest is no file (not sent).
  const media = fakeApi({ firefox: false, pageMediaResult: ["https://v.test/clip.webm", "https://files.test/movie.mp4"] });
  load(media.api, false);
  media.listeners.responseStarted(res("https://files.test/movie.mp4", "media", H("video/mp4")));
  media.listeners.responseStarted(res("https://files.test/song.mp3", "media", H("audio/mpeg")));
  media.listeners.responseStarted(res("https://live.test/s/master.m3u8", "xmlhttprequest", H("application/vnd.apple.mpegurl")));
  await media.listeners.menu({ menuItemId: "mdm-media", pageUrl: "https://gallery.test/p", frameId: 0 }, tab);
  await settle();
  assert.deepEqual(media.calls.filter((c) => c[0] === "pageMedia").map((c) => c[1]), [["media"]]);
  m = natives(media.calls).at(-1);
  assert.deepEqual(m.links, ["https://v.test/clip.webm", "https://files.test/movie.mp4", "https://files.test/song.mp3"], "each once, no manifest");
  // Nothing found: nothing sent.
  const none = fakeApi({ firefox: false, pageMediaResult: [] });
  load(none.api, false);
  await none.listeners.menu({ menuItemId: "mdm-images", pageUrl: "https://gallery.test/p", frameId: 0 }, tab);
  await settle();
  assert.deepEqual(natives(none.calls), []);
  ok("right-click: all images, all videos and sounds (with the files its players played) to the Link grabber");
}

// pageMedia, in a page: a picture at the largest size its srcset offers, the address a lazy page
// keeps, links to such files; a player's file, its sources; never data:, blob: or the page itself.
{
  const { api } = fakeApi({ firefox: false });
  const ctx = load(api, false);
  const el = (tag, attrs = {}, props = {}, parent = "") => ({ tag, attrs, parent, getAttribute: (n) => attrs[n] ?? null, ...props });
  // querySelectorAll for the selectors pageMedia uses: `tag`, `parent tag`, `tag[attr]`.
  const page = (els) => ({
    baseURI: "https://gallery.test/p/",
    querySelectorAll: (sel) =>
      els.filter((e) =>
        sel.split(",").some((part) => {
          const [, parent, tag, attr] = /^(?:(\w+) )?(\w+)(?:\[(\w+)\])?$/.exec(part.trim());
          return e.tag === tag && (!attr || e.attrs[attr] != null) && (!parent || e.parent === parent);
        }),
      ),
  });
  ctx.document = page([
    el("img", { srcset: "small.jpg 400w, big.jpg 1600w, mid.jpg 800w", src: "small.jpg" }, { currentSrc: "https://gallery.test/p/mid.jpg", src: "https://gallery.test/p/small.jpg" }),
    el("img", { src: "data:image/gif;base64,R0l", "data-src": "/lazy/real.webp" }, { currentSrc: "data:image/gif;base64,R0l", src: "data:image/gif;base64,R0l" }),
    el("img", { src: "https://cdn.test/a.png" }, { currentSrc: "https://cdn.test/a.png", src: "https://cdn.test/a.png" }),
    el("img", { src: "blob:https://gallery.test/1" }, { currentSrc: "blob:https://gallery.test/1", src: "blob:https://gallery.test/1" }),
    el("source", { srcset: "https://cdn.test/hero.avif 1x, https://cdn.test/hero@2x.avif 2x" }, {}, "picture"),
    el("a", { href: "x" }, { href: "https://cdn.test/full/photo.JPG?size=orig" }),
    el("a", { href: "x" }, { href: "https://gallery.test/page.html" }),
    el("a", { href: "x" }, { href: "https://cdn.test/a.png" }),
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.pageMedia("images"))), [
    "https://gallery.test/p/big.jpg",
    "https://gallery.test/lazy/real.webp",
    "https://cdn.test/a.png",
    "https://cdn.test/hero@2x.avif",
    "https://cdn.test/full/photo.JPG?size=orig",
  ]);
  ctx.document = page([
    el("video", { src: "https://v.test/clip.mp4" }, { currentSrc: "https://v.test/clip.mp4", src: "https://v.test/clip.mp4" }),
    el("video", {}, { currentSrc: "blob:https://v.test/9", src: "" }),
    el("source", { src: "alt.webm" }, { src: "https://v.test/alt.webm" }, "video"),
    el("audio", {}, { currentSrc: "", src: "" }),
    el("a", { href: "x" }, { href: "https://v.test/song.mp3" }),
    el("a", { href: "x" }, { href: "https://v.test/doc.pdf" }),
    el("a", { href: "x" }, { href: "https://v.test/pic.jpg" }),
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(ctx.pageMedia("media"))), ["https://v.test/clip.mp4", "https://v.test/alt.webm", "https://v.test/song.mp3"]);
  ok("pageMedia: largest pictures, lazy ones, links to files; players' files; nothing else");
}

// The extension's pages (the options page, the status page): only two, no credit footer (the
// owner's choice: the copyright and the credit are in DDM's About), the footer's two quiet links
// (Help, Support DDM: the GitHub Sponsors page in a new tab, no badge or nag), their texts in English, no
// markup written from data, nothing loaded from outside (no remote code: MV3), only their own
// scripts, Comet's colours by day and by night, no side stripes; nothing of the Downloads and
// links window, Open DDM, media names or the keys hint left.
{
  const read = (f) => readFileSync(new URL(`../../extension/${f}`, import.meta.url), "utf8");
  const scripts = ["pages.js", "options.js", "install.js", "popup.js"].map(read).join("\n");
  const pages = ["options.html", "install.html"];
  const html = [...pages, "popup.html"].map(read).join("\n");
  const css = read("pages.css");
  const en = JSON.parse(read("_locales/en/messages.json"));
  assert.throws(() => read("popup.css"), /ENOENT/, "popup.css is gone");
  // The toolbar button's panel: drawn from its own page at once (no wait for the background), the
  // pages' styles and helpers, its texts in _locales, no footer (Help is on the status page).
  {
    const popup = read("popup.html");
    const srcs = [...popup.matchAll(/<script src="([^"]+)"/g)].map((m) => m[1]);
    assert.deepEqual(srcs, ["pages.js", "popup.js"], "popup.html: pages.js, then its own script");
    assert.ok(/<link rel="stylesheet" href="pages\.css">/.test(popup) && !/<footer>/.test(popup), "popup.html: the pages' styles, no footer");
    for (const [, key] of popup.matchAll(/data-i18n="(\w+)"/g)) assert.ok(en[key], `popup.html: ${key} has English`);
    assert.ok(/body\.popup\s*\{[^}]*width:\s*\d+px/.test(css), "the panel has its width");
  }
  for (const page of pages) {
    assert.ok(!/id="credit"|popup_credit/.test(read(page)), `${page}: no credit link`);
    const footer = /<footer>([\s\S]*?)<\/footer>/.exec(read(page))[1];
    assert.deepEqual([...footer.matchAll(/<a id="(\w+)"[^>]*data-i18n="(\w+)"[^>]*target="_blank" rel="noopener"><\/a>/g)].map((m) => [m[1], m[2]]), [["help", "popup_help"], ["support", "page_support"]], `${page}: the footer has Help and Support DDM, each in a new tab`);
    assert.ok(/<span id="version"><\/span>/.test(footer), `${page}: and the version`);
    assert.ok(new RegExp(`<script src="${page.replace(".html", ".js")}"`).test(read(page)) && /\$\("support"\)\.href = SUPPORT;/.test(read(page.replace(".html", ".js"))), `${page}: its script points Support DDM at the page`);
    assert.ok(!/popup\.css/.test(read(page)), `${page}: its own styles only`);
    // pages.js first: the page's own script uses what it shares.
    const srcs = [...read(page).matchAll(/<script src="([^"]+)"/g)].map((m) => m[1]);
    assert.equal(srcs[0], "pages.js", page);
    assert.ok(srcs.every((s) => /^[\w-]+\.js$/.test(s)), `${page}: its own scripts only`);
  }
  assert.ok(!/UPSTREAM|misterdownloadmanager|popup_credit/.test(scripts), "no credit link in the scripts");
  for (const key of ["popup_credit", "popup_credit_title", "popup_open", "toolbar_open", "toolbar_panel", "toolbar_options", "toolbar_status", "popup_media_title_name", "options_media_note", "popup_alt", "popup_keys_prevent", "popup_keys_force", "popup_media", "popup_downloads", "status_connected", "status_connected_version", "status_active", "page_turn_off"]) {
    assert.equal(en[key], undefined, `${key} is gone`);
  }
  assert.ok(!/open-app|open-panel|keys-hint|media-title-name|"downloads"|add-url/.test(scripts + html), "nothing of the old window, Open DDM, media names or the keys hint");
  assert.ok(read("pages.js").includes('const SUPPORT = "https://github.com/sponsors/Riskraptor";'), "Support DDM: the owner's GitHub Sponsors page");
  assert.equal(en.page_support.message, "Support DDM");
  assert.equal((scripts + html).match(/github\.com\/sponsors/gi).length, 1, "one link, nowhere else: no badge or nag");
  assert.ok(!/innerHTML|insertAdjacentHTML|outerHTML|eval\(|new Function/.test(scripts), "built as text, no code from data");
  assert.ok(!/\bfetch\(|XMLHttpRequest/.test(scripts), "the pages fetch nothing (links only)");
  assert.ok(!/https?:\/\/(?!www\.w3\.org\/2000\/svg)/.test(css) && !/<(?:link|script)[^>]+https?:/.test(html), "nothing from outside");
  assert.ok(!/\b(left|right)\s*:|margin-(left|right)|padding-(left|right)|text-align:\s*(left|right)|border-(left|right)/.test(css), "only logical sides (right to left)");
  assert.ok(/prefers-color-scheme: dark/.test(css), "dark");
  for (const colour of ["#1e2a66", "#070a1e", "#fff7e6", "#ffb23e", "#ff6a45", "#b43a8c"]) assert.ok(css.toLowerCase().includes(colour), `Comet's ${colour}`);
  assert.ok(!/border-inline-(start|end)\s*:\s*[2-9]px|border-(top|bottom|block-start|block-end)\s*:\s*[2-9]px solid/.test(css), "no thick side stripe on a card");
  ok("the extension's pages: two, Help in the footer, their texts, no markup from data, nothing from outside, logical sides, Comet day and night");
}

// ---- Options → General: Keys..., Customize DDM menu items, Download panels -------------------

// The rules carry the keys a click is held with and the right-click items to show, as Options
// offers them (anything else: none); the menu is made again only when its items change; the
// browser's window getting the focus back asks for the rules again when a few seconds old, so a
// change in Options applies at the next click. An app before them: Alt and every item, as before.
{
  const clock = { t: Date.now() };
  class FakeDate extends Date {
    static now() {
      return clock.t;
    }
  }
  const said = { keys: { prevent: "shift", force: "ctrl+alt" }, menu: ["all", "link", "later"] };
  const { api, calls, listeners, store } = fakeApi({ firefox: false, rules: said });
  load(api, false, { Date: FakeDate });
  const made = () => {
    // The page's items of the last time the menu was made (the toolbar button's own follow).
    const at = calls.map((c) => c[0]).lastIndexOf("menu-clear");
    return calls.slice(at + 1).filter((c) => c[0] === "menu-item" && !c[1].contexts.includes("action")).map((c) => c[1].id);
  };
  const rebuilt = () => calls.filter((c) => c[0] === "menu-clear").length;
  const asked = () => calls.filter((c) => c[0] === "rules").length;
  await listeners.installed({ reason: "install" });
  await settle();
  assert.deepEqual(store.rules.keys, { prevent: "shift", force: "ctrl+alt" });
  assert.deepEqual(store.rules.menu, ["link", "all"], "Options' order kept, a name it doesn't know left out");
  assert.deepEqual(made(), ["mdm-link", "mdm-all"], "made again with Options' items");
  assert.equal(store.menus, "link all | exclude help");
  // Options changed; the browser's window gets the focus back: asked again (once in a few
  // seconds), the menu made again with every item.
  said.menu = ["link", "selected", "all", "images", "media"];
  clock.t += 6000;
  const before = asked();
  listeners.focus(1);
  await settle();
  assert.equal(asked(), before + 1, "asked again at the focus");
  assert.deepEqual(made(), ["mdm-link", "mdm-selected", "mdm-all", "mdm-images", "mdm-media"]);
  clock.t += 1000;
  listeners.focus(2);
  listeners.focus(-1);
  await settle();
  assert.equal(asked(), before + 1, "not again within seconds, nor for no window");
  // The same items again: the menu stays as it is.
  const times = rebuilt();
  clock.t += 6000;
  listeners.focus(1);
  await settle();
  assert.equal(asked(), before + 2);
  assert.equal(rebuilt(), times, "unchanged: not made again");
  // Keys Options doesn't offer, or the same for both: none, or the browser's.
  said.keys = { prevent: "meta", force: "ctrl+alt" };
  await ownAsk(listeners, { type: "state" }, "options.html");
  assert.deepEqual(store.rules.keys, { prevent: "", force: "ctrl+alt" });
  said.keys = { prevent: "alt", force: "alt" };
  await ownAsk(listeners, { type: "state" }, "options.html");
  assert.deepEqual(store.rules.keys, { prevent: "alt", force: "" });
  // An app before them: Alt leaves a link to the browser, nothing forces one, every item.
  delete said.keys;
  delete said.menu;
  await ownAsk(listeners, { type: "state" }, "options.html");
  assert.deepEqual([store.rules.keys, store.rules.menu], [{ prevent: "alt", force: "" }, ["link", "selected", "all", "images", "media"]]);
  // A page that loads asks for them when old (15 s), not each time.
  const sender = { id: "ours", frameId: 0, url: "https://page.test/", tab: { id: 3, url: "https://page.test/" } };
  const fromPage = (m) => new Promise((r) => listeners.message(m, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  const n = asked();
  await fromPage({ type: "rules" });
  await settle();
  assert.equal(asked(), n, "fresh: not asked");
  clock.t += 16000;
  await fromPage({ type: "rules" });
  await settle();
  assert.equal(asked(), n + 1, "old: asked");
  ok("Keys... and the menu items: the rules carry them, the menu made again when they change, asked again at the window's focus");
}

// A link clicked with Options' keys to force it (page.js): DDM takes it whatever its type, with
// the page as referrer, the link's download name and its cookies, as Download with DDM does;
// only web and FTP addresses; DDM not reached: the help page's section opens.
{
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  const sender = { id: "ours", frameId: 0, url: "https://page.test/list", tab: { id: 3, url: "https://page.test/list", incognito: false } };
  const send = (l, m) => new Promise((r) => l.message(m, sender, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(await send(listeners, { type: "force", url: "https://x.test/notes.html", name: "C:\\x\\notes.html" }), { ok: true });
  const m = natives(calls).at(-1);
  assert.deepEqual(
    [m.url, m.explicit, m.referrer, m.filename, m.cookies],
    ["https://x.test/notes.html", true, "https://page.test/list", "notes.html", "sid=1"],
  );
  assert.deepEqual(await send(listeners, { type: "force", url: "ftp://files.test/a.iso" }), { ok: true });
  assert.equal(natives(calls).at(-1).url, "ftp://files.test/a.iso");
  const count = natives(calls).length;
  assert.deepEqual(await send(listeners, { type: "force", url: "javascript:alert(1)" }), { ok: false });
  assert.deepEqual(await send(listeners, { type: "force", url: 7 }), { ok: false });
  assert.equal(natives(calls).length, count, "nothing sent");
  const away = fakeApi({ firefox: false, host: false });
  load(away.api, false);
  assert.equal((await send(away.listeners, { type: "force", url: "https://x.test/a.html" })).ok, false);
  await settle();
  assert.ok(away.calls.some((c) => c[0] === "openTab" && c[1].endsWith("/install.html")), "the status page");
  ok("a link clicked with the keys to force it: DDM's whatever its type, as Download with DDM");
}

// page.js goes by the keys the rules hold: exactly the keys held (the force keys stop the click
// and send the link; the prevent keys leave it to the browser), the primary button only for
// forcing, the user's own clicks only; changed in the stored rules, the next click goes by them;
// a Mac's Command is Options' Ctrl. Without rules (or an app before Keys...): Alt, as before.
{
  const pageCode = readFileSync(new URL("../../extension/page.js", import.meta.url), "utf8");
  const run = async ({ rules, platform = "Win32" }) => {
    const sent = [];
    const handlers = {};
    let changed = null;
    class Element {
      constructor(href, attrs = {}) {
        Object.assign(this, { href, attrs });
      }
      closest() {
        return this;
      }
      getAttribute(n) {
        return this.attrs[n] || "";
      }
    }
    const api = {
      runtime: { id: "ours", getManifest: () => ({}), sendMessage: async (m) => (sent.push(JSON.parse(JSON.stringify(m))), { ok: true }) },
      storage: { local: { get: async () => ({ rules }) }, onChanged: { addListener: (f) => (changed = f), removeListener() {} } },
    };
    vm.runInNewContext(pageCode, { chrome: api, Element, URL, Promise, navigator: { platform }, addEventListener: (n, f) => (handlers[n] = f), removeEventListener() {} });
    await settle();
    const click = (href, keys = {}, { type = "click", button = 0, trusted = true, attrs } = {}) => {
      const e = {
        type,
        button,
        isTrusted: trusted,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
        ...keys,
        target: new Element(href, attrs),
        defaultPrevented: false,
        stopped: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
        stopPropagation() {
          this.stopped = true;
        },
      };
      handlers[type](e);
      return e;
    };
    const last = () => sent.at(-1);
    return { click, sent, last, change: (r) => changed({ rules: { newValue: r } }, "local") };
  };
  let p = await run({ rules: { keys: { prevent: "shift", force: "ctrl+alt" }, types: "ZIP", capture: true } });
  assert.deepEqual(p.sent, [{ type: "rules" }], "the top frame asks for fresh rules at its start");
  let e = p.click("https://x.test/notes.html", { ctrlKey: true, altKey: true });
  assert.ok(e.defaultPrevented && e.stopped, "forced: the page and the browser do nothing else");
  assert.deepEqual(p.last(), { type: "force", url: "https://x.test/notes.html" });
  e = p.click("https://x.test/get?id=1", { ctrlKey: true, altKey: true }, { attrs: { download: "a.bin" } });
  assert.deepEqual(p.last(), { type: "force", url: "https://x.test/get?id=1", name: "a.bin" });
  p.click("ftp://files.test/a.iso", { ctrlKey: true, altKey: true });
  assert.deepEqual(p.last(), { type: "force", url: "ftp://files.test/a.iso" }, "FTP: DDM's");
  const n = p.sent.length;
  e = p.click("https://x.test/a.zip", { ctrlKey: true, altKey: true }, { type: "auxclick", button: 1 });
  assert.ok(!e.defaultPrevented && p.sent.length === n, "a middle click opens its tab as always");
  e = p.click("https://x.test/a.zip", { ctrlKey: true, altKey: true, shiftKey: true });
  assert.ok(!e.defaultPrevented && p.sent.length === n, "more keys than Options': nothing");
  e = p.click("https://x.test/a.zip", { ctrlKey: true, altKey: true }, { trusted: false });
  assert.ok(!e.defaultPrevented && p.sent.length === n, "a page's own made-up click: nothing");
  e = p.click("javascript:void(0)", { ctrlKey: true, altKey: true });
  assert.ok(!e.defaultPrevented && p.sent.length === n, "not an address DDM downloads");
  e = p.click("https://x.test/a.zip", { shiftKey: true });
  assert.ok(!e.defaultPrevented, "the browser's own (a new window)");
  assert.deepEqual(p.last(), { type: "bypass", url: "https://x.test/a.zip" });
  const m = p.sent.length;
  p.click("https://x.test/a.zip", { altKey: true });
  assert.equal(p.sent.length, m, "Alt isn't Options' key now");
  // Changed in Options (the stored rules): the next click goes by them.
  p.change({ keys: { prevent: "alt", force: "" } });
  e = p.click("https://x.test/a.zip", { ctrlKey: true, altKey: true });
  assert.ok(!e.defaultPrevented && p.sent.length === m, "nothing forces now");
  p.click("https://x.test/a.zip", { altKey: true });
  assert.deepEqual(p.last(), { type: "bypass", url: "https://x.test/a.zip" });
  // A Mac: Command for Ctrl; a Control-click (the secondary click) is no choice.
  p = await run({ rules: { keys: { prevent: "alt", force: "ctrl+alt" } }, platform: "MacIntel" });
  p.click("https://x.test/notes.html", { metaKey: true, altKey: true });
  assert.deepEqual(p.last(), { type: "force", url: "https://x.test/notes.html" });
  const k = p.sent.length;
  e = p.click("https://x.test/notes.html", { ctrlKey: true, altKey: true });
  assert.ok(!e.defaultPrevented && p.sent.length === k, "Control on a Mac");
  // Windows' key held too: no choice.
  p = await run({ rules: { keys: { prevent: "alt", force: "ctrl+alt" } } });
  e = p.click("https://x.test/notes.html", { ctrlKey: true, altKey: true, metaKey: true });
  assert.ok(!e.defaultPrevented);
  // No rules yet, or an app before Keys...: Alt leaves the link, nothing forces; the same keys for
  // both: the browser's.
  // Keys it doesn't know: none.
  for (const [rules, altLeaves] of [
    [null, true],
    [{ types: "ZIP", capture: true }, true],
    [{ keys: { prevent: "alt", force: "alt" } }, true],
    [{ keys: { prevent: "<b>", force: 7 } }, false],
  ]) {
    p = await run({ rules });
    e = p.click("https://x.test/a.zip", { ctrlKey: true, altKey: true });
    assert.ok(!e.defaultPrevented, `${JSON.stringify(rules)}: nothing forced`);
    const before = p.sent.length;
    p.click("https://x.test/a.zip", { altKey: true });
    assert.equal(p.sent.length > before && p.last().type === "bypass", altLeaves, `${JSON.stringify(rules)}: Alt`);
  }
  ok("page.js: the keys to force or leave a link, exactly as held, changed at the next click; Command on a Mac");
}

// Download panels: the app's answer carries what the button shows over (videos, sound players),
// the least length and its first corner, checked; "Never on" names the site without its www.
{
  const ask = (l, url) => new Promise((r) => l.message({ type: "panel" }, { id: "ours", url, frameId: 0, tab: { url } }, (v) => r(JSON.parse(JSON.stringify(v)))));
  const said = fakeApi({ firefox: false, panelLookReply: { video: false, audio: true, minSeconds: 30, corner: "bottom-left" } });
  load(said.api, false);
  assert.deepEqual(await ask(said.listeners, "https://tube.test/v"), { on: true, video: false, audio: true, minSeconds: 30, corner: "bottom-left" });
  const odd = fakeApi({ firefox: false, panelLookReply: { video: "no", minSeconds: -5, corner: "<b>x</b>" } });
  load(odd.api, false);
  assert.deepEqual(await ask(odd.listeners, "https://tube.test/v"), ON, "what isn't one of them: as installed");
  const off = fakeApi({ firefox: false, panelOn: false, panelLookReply: { video: true } });
  load(off.api, false);
  assert.deepEqual(await ask(off.listeners, "https://tube.test/v"), { on: false });
  const { api, calls, listeners } = fakeApi({ firefox: false });
  load(api, false);
  await new Promise((r) => listeners.message({ type: "hide-site" }, { id: "ours", url: "https://www.tube.test/watch", frameId: 0, tab: { url: "https://www.tube.test/watch" } }, r));
  assert.equal(natives(calls).find((x) => x.hideSite).hideSite, "tube.test", "www. left out: the app's name takes its subdomains");
  assert.deepEqual(await ask(listeners, "https://www.tube.test/other"), { on: false }, "off at once on that host");
  // Button settings...: whether DDM was reached, for the button to say so.
  const away = fakeApi({ firefox: false, host: false });
  load(away.api, false);
  const opened = await new Promise((r) => away.listeners.message({ type: "options" }, { id: "ours", url: "https://tube.test/", frameId: 0, tab: {} }, (v) => r(JSON.parse(JSON.stringify(v)))));
  assert.deepEqual(opened, { ok: false, why: "not-installed" });
  ok("Download panels: over what, from what length, which corner; Never on names the site without www.");
}

// video.js: Download panels' settings read from the answer (as installed when it says none), and
// the least length: a length not known yet, or a live stream's, shows the button.
{
  const source = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  const fn = (name) => {
    const start = source.indexOf(`function ${name}(`);
    assert.ok(start > 0, `video.js has ${name}`);
    let depth = 0;
    for (let i = source.indexOf(") {", start) + 2; i < source.length; i += 1) {
      if (source[i] === "{") depth += 1;
      if (source[i] === "}" && --depth === 0) return source.slice(start, i + 1);
    }
    throw new Error(`${name}: no end`);
  };
  const { panelLook, longEnough } = new Function(`${fn("panelLook")}\n${fn("longEnough")}\nreturn { panelLook, longEnough };`)();
  const plain = (x) => JSON.parse(JSON.stringify(x));
  assert.deepEqual(plain(panelLook(null)), { video: true, audio: true, minSeconds: 0, corner: "top-right" });
  assert.deepEqual(plain(panelLook({ on: true, video: false, audio: true, minSeconds: 45, corner: "bottom-right" })), { video: false, audio: true, minSeconds: 45, corner: "bottom-right" });
  assert.deepEqual(plain(panelLook({ minSeconds: "9", corner: "middle", audio: 0 })), { video: true, audio: true, minSeconds: 0, corner: "top-right" });
  assert.equal(longEnough(10, 0), true, "any length");
  assert.equal(longEnough(10, 30), false);
  assert.equal(longEnough(30, 30), true);
  assert.equal(longEnough(NaN, 30), true, "not loaded yet");
  assert.equal(longEnough(Infinity, 30), true, "a live stream");
  // The button goes by them: eligible() asks both, the answer sets them, Options' corner leads
  // unless the button was dragged on the site.
  assert.ok(fn("eligible").includes("if (!(audio ? panelOpts.audio : panelOpts.video)) return false;"));
  assert.ok(fn("eligible").includes("longEnough(v.duration, panelOpts.minSeconds)"));
  assert.ok(/panelOpts = panelLook\(r\);\s*\/\/[^\n]*\n\s*if \(!chosen\) place = CORNER_SPOTS\[panelOpts\.corner\];/.test(fn("ask")));
  // The whole script parses (a name it declares twice would stop it in every page).
  assert.doesNotThrow(() => new vm.Script(source));
  assert.ok(fn("spot").includes("const order = [place, ...CORNERS.filter((c) => c !== place)];"));
  assert.ok(source.includes('ext.i18n.getMessage("video_never_site", [site])'), "Never on the site without www.");
  ok("video.js: Download panels' settings and the least length");
}

// video.js: ✕ hides the button on what the player plays now, not on the element: YouTube plays
// every video in one element as the page moves in place, and its ad and video are two blob:s.
{
  const source = readFileSync(new URL("../../extension/video.js", import.meta.url), "utf8");
  const start = source.indexOf("  const dismissed = new WeakMap();");
  const end = source.indexOf("\n", source.indexOf("  const isDismissed = "));
  assert.ok(start > 0 && end > start, "video.js has its dismissal");
  const location = { href: "https://www.youtube.com/watch?v=one" };
  const { dismissed, dismissedSrc, playing, isDismissed } = new Function(
    "location",
    `${source.slice(start, end)}\nreturn { dismissed, dismissedSrc, playing, isDismissed };`,
  )(location);
  const hide = (v) => {
    dismissed.set(v, playing(v));
    if (v.currentSrc && !v.currentSrc.startsWith("blob:")) dismissedSrc.add(v.currentSrc);
  };
  assert.ok(source.includes("dismissed.set(video, playing(video));"), "✕ remembers what plays");
  const yt = { currentSrc: "blob:https://www.youtube.com/ad" };
  hide(yt);
  assert.equal(isDismissed(yt), true);
  yt.currentSrc = "blob:https://www.youtube.com/video";
  assert.equal(isDismissed(yt), true, "the ad over, the same video: still hidden");
  location.href = "https://www.youtube.com/watch?v=two";
  yt.currentSrc = "blob:https://www.youtube.com/next";
  assert.equal(isDismissed(yt), false, "the next video in the same element: the button again");
  const file = { currentSrc: "https://cdn.example/a.mp4" };
  hide(file);
  const again = { currentSrc: "https://cdn.example/a.mp4" };
  assert.equal(isDismissed(again), true, "that file in another player: still hidden");
  file.currentSrc = "https://cdn.example/b.mp4";
  assert.equal(isDismissed(file), false, "another file in that player: the button");
  ok("video.js: ✕ hides the button on this video only, the next one in the same player shows it");
}

// ---- the extension's pages, run against a fake page -------------------------------------------

const pageEnglish = JSON.parse(readFileSync(new URL("../../extension/_locales/en/messages.json", import.meta.url), "utf8"));
// English's text of `key`, with its placeholders filled.
const english = (key, subs = []) => {
  const m = pageEnglish[key];
  if (!m) return "";
  return m.message.replace(/\$(\w+)\$/g, (_, name) => subs[Number(m.placeholders[name].content.slice(1)) - 1]);
};
// One of the extension's pages (`file`, after pages.js, as its <script>s load them) in a fake
// document: `answer(m)` is the background's answer to each message; `nav` the browser's navigator;
// its timers are run by hand (`run()`: those due, in order). Returns its elements by ID, what it
// asked (and the whole messages), what it opened, and the timers.
function runPage(file, { answer, nav = { platform: "Win32", userAgent: "Chrome/141" }, search = "", manifest = { permissions: [] }, extra = {} }) {
  const els = {};
  const element = (tag = "div") => {
    const classes = new Set();
    const listeners = {};
    const el = {
      tag,
      hidden: false,
      textContent: "",
      dataset: {},
      style: {},
      children: [],
      classList: {
        add: (...c) => c.forEach((x) => classes.add(x)),
        remove: (...c) => c.forEach((x) => classes.delete(x)),
        toggle: (c, on) => (on ?? !classes.has(c) ? classes.add(c) : classes.delete(c)),
        contains: (c) => classes.has(c),
      },
      set className(v) {
        classes.clear();
        String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x));
      },
      get className() {
        return [...classes].join(" ");
      },
      setAttribute() {},
      hasAttribute: () => true,
      focus() {},
      addEventListener: (type, f) => (listeners[type] = f),
      click: () => listeners.click && listeners.click({ target: el }),
      // An event of `type` (a switch's change), as the browser sends it.
      fire: async (type, ev = {}) => listeners[type] && listeners[type]({ target: el, ...ev }),
      append: (...kids) => el.children.push(...kids),
      replaceChildren: (...kids) => (el.children = kids),
    };
    return el;
  };
  const timers = [];
  const storageListeners = [];
  const asked = [];
  const sent = [];
  const opened = [];
  const document = {
    documentElement: {},
    visibilityState: "visible",
    addEventListener() {},
    querySelectorAll: () => [],
    getElementById: (id) => (els[id] ||= element()),
    createElement: element,
    createElementNS: element,
  };
  const api = {
    runtime: {
      id: "ours",
      sendMessage: async (m) => {
        asked.push(m.type);
        sent.push(JSON.parse(JSON.stringify(m)));
        return JSON.parse(JSON.stringify(answer(m)));
      },
      getManifest: () => manifest,
      getURL: (p) => `chrome-extension://ours/${p}`,
      openOptionsPage: async () => opened.push("options"),
      reload: () => opened.push("reload"),
    },
    storage: { local: { get: async (d) => d, set: async () => {} }, onChanged: { addListener: (f) => storageListeners.push(f) } },
    tabs: { query: async () => [], get: async (id) => ({ id, url: "https://x.test/" }), create: async (t) => opened.push(t.url) },
    permissions: { contains: async () => false, request: async (q) => opened.push(["request", JSON.parse(JSON.stringify(q))]) },
    management: { uninstallSelf: async (o) => opened.push(["uninstall", JSON.parse(JSON.stringify(o))]) },
    i18n: { getMessage: (k, subs) => english(k, subs), getUILanguage: () => "en" },
    ...extra,
  };
  const code = readFileSync(new URL("../../extension/pages.js", import.meta.url), "utf8") + "\n" + readFileSync(new URL(`../../extension/${file}`, import.meta.url), "utf8");
  const fakeTimeout = (f, ms) => {
    timers.push({ f, ms });
    return timers.length;
  };
  vm.runInNewContext(code, { chrome: api, document, navigator: nav, location: { search }, URL, URLSearchParams, Intl, Promise, JSON, setTimeout: fakeTimeout, clearTimeout() {}, window: { close() {} } });
  // The timers set meanwhile, run once each (the page sets the next one as it goes).
  const run = async () => {
    const due = timers.splice(0);
    for (const t of due) await t.f();
    await settle();
    return due.map((t) => t.ms);
  };
  // A change in the extension's storage, as the browser tells the page.
  const changed = async (changes) => {
    for (const f of storageListeners) await f(JSON.parse(JSON.stringify(changes)), "local");
    await settle();
  };
  return { els, asked, sent, opened, timers, run, changed };
}
// A status page note's sentences (one line each), as one text.
const noteText = (li) => li.children.filter((c) => c.tag === "p" && !c.classList.contains("note")).map((c) => c.textContent).join(" ");

// What a page shows a person: its visible texts (headline, lead, steps, notes, buttons, the
// switch's label), in English.
const shown = (els) => {
  const texts = [];
  const walk = (el) => {
    if (!el || el.hidden) return;
    if (el.textContent) texts.push(el.textContent);
    for (const c of el.children || []) walk(c);
  };
  for (const id of ["headline", "lead", "steps", "already", "notes", "retry", "app-options", "remove", "extensions", "download-files", "status-text"]) walk(els[id]);
  return texts.join(" | ");
};
// Words a person who isn't a developer doesn't use.
const TECHNICAL = /\b(connected|connection|native|host|helper|handshake|origin|nmhost|integration module|extension id)\b/i;

// The status page: DDM missing, then installed. It asks every two seconds (never more while
// hidden), says plainly what's missing, offers DDM's download for this system (the website's
// download page at its files; GitHub's latest release beside it) with three short steps, and
// turns into "Dannex Download Manager is installed" by itself once DDM answers, with its on/off
// switch and no version in the headline; then asks every five seconds.
{
  let s = { connected: false, why: "not-installed", version: "0.6.5", browser: "chromium", copy: "store", paused: false, allSites: true, incognito: true };
  const page = runPage("install.js", { answer: (m) => (m.type === "probe" ? s : { ok: true }) });
  await settle();
  const { els } = page;
  assert.equal(els.headline.textContent, "Dannex Download Manager isn't installed");
  assert.equal(els.lead.textContent, english("page_lead_not_installed"));
  assert.ok(els.hero.classList.contains("bad") && !els.hero.classList.contains("wait"));
  assert.ok(!els.get.hidden);
  assert.equal(els.download.href, "https://dannex.kavangi.co.ke/download.html#windows");
  assert.equal(els["download-files"].textContent, english("page_files_windows"));
  assert.equal(els["all-downloads"].href, "https://github.com/Riskraptor/dannexdownloadmanager/releases/latest");
  assert.deepEqual(els.steps.children.map((li) => li.textContent), ["page_step_windows", "page_step_wait", "page_step_restart"].map((k) => english(k)), "three short steps");
  assert.ok(!els.already.hidden, "and a line for one already installed");
  assert.ok(els.retry.hidden && els["toggle-row"].hidden, "the download is its one button");
  assert.equal(els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html#not-installed");
  assert.equal(els.version.textContent, "Dannex Download Manager 0.6.5", "the version small, at the bottom");
  assert.equal(els.support.href, "https://github.com/sponsors/Riskraptor", "Support DDM, beside Help");
  assert.ok(!TECHNICAL.test(shown(els)), shown(els));
  assert.deepEqual(await page.run(), [2000], "asked again in two seconds");
  // DDM installed meanwhile: installed, by itself.
  s = { ...s, connected: true, why: null, app: "0.6.5" };
  await page.run();
  assert.equal(els.headline.textContent, "Dannex Download Manager is installed");
  assert.equal(els.lead.textContent, "Downloads you start in this browser go to Dannex Download Manager.");
  assert.ok(!/\d+\.\d+/.test(els.headline.textContent + els.lead.textContent), "no version in the headline");
  assert.ok(els.hero.classList.contains("ok") && els.get.hidden && els.steps.hidden && els.already.hidden);
  assert.ok(!els["toggle-row"].hidden && els.toggle.checked, "the switch, on");
  assert.ok(els.retry.hidden && els["app-options"].hidden && !els.open, "no other button: nothing opens DDM");
  assert.equal(els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html");
  assert.ok(!TECHNICAL.test(shown(els)), shown(els));
  assert.deepEqual(await page.run(), [5000], "then every five seconds");
  assert.ok(page.asked.every((t) => t === "probe"));
  // Its switch is the toolbar button's.
  s = { ...s, paused: true };
  els.toggle.checked = false;
  await els.toggle.fire("change");
  await settle();
  assert.deepEqual(page.sent.find((m) => m.type === "pause"), { type: "pause", paused: true });
  assert.ok(!els.toggle.checked && els.hero.classList.contains("off"));
  assert.equal(els.lead.textContent, english("page_off_lead"));
  ok("the status page: missing, the download and three steps, then installed by itself, with its switch");
}

// The status page picks the download as the website does: a Mac's .dmg, Linux's AppImage, .deb
// or .rpm, and the whole page on a system DDM doesn't run on (a phone, a Chromebook), where the one
// step says so.
for (const [nav, system] of [
  [{ platform: "MacIntel", userAgent: "Mac OS X", maxTouchPoints: 0 }, "macos"],
  [{ userAgentData: { platform: "Linux" }, userAgent: "X11; Linux x86_64" }, "linux"],
  [{ platform: "Linux armv8l", userAgent: "Android 14; Mobile" }, "other"],
  [{ platform: "MacIntel", userAgent: "Mac OS X", maxTouchPoints: 5 }, "other"],
  [{ platform: "Linux x86_64", userAgent: "X11; CrOS x86_64" }, "other"],
]) {
  const page = runPage("install.js", { answer: () => ({ connected: false, why: "not-installed", version: "0.6.5", browser: "chromium" }), nav });
  await settle();
  assert.equal(page.els.download.href, `https://dannex.kavangi.co.ke/download.html${system === "other" ? "" : `#${system}`}`, system);
  assert.equal(page.els["download-files"].textContent, english(`page_files_${system}`));
  assert.equal(page.els.steps.children[0].textContent, english(`page_step_${system}`));
  assert.equal(page.els.steps.children.length, system === "other" ? 1 : 3);
  assert.equal(page.els.already.hidden, system === "other");
}
ok("the status page: the download for this computer's system");

// The status page's other states: one or two plain sentences and one clear button each, the
// help page's section, and no developer's words.
{
  const at = async (s, nav) => {
    const page = runPage("install.js", { answer: () => ({ version: "0.6.5", browser: "chromium", ...s }), nav });
    await settle();
    return page;
  };
  const buttons = (p) => ["retry", "app-options", "remove", "extensions"].filter((id) => !p.els[id].hidden);
  const plain = (p) => assert.ok(!TECHNICAL.test(shown(p.els)), shown(p.els));
  let p = await at({ connected: false, why: "not-running" });
  assert.equal(p.els.headline.textContent, "Dannex Download Manager didn't start");
  assert.equal(p.els.lead.textContent, english("page_lead_not_running"));
  assert.ok(p.els.get.hidden && p.els.steps.hidden);
  assert.deepEqual(buttons(p), ["retry"]);
  assert.equal(p.els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html#not-running");
  plain(p);
  p = await at({ connected: false, why: "host-failed" });
  assert.equal(p.els.headline.textContent, english("status_host_failed"));
  assert.equal(p.els.lead.textContent, english("page_lead_host_failed"));
  assert.ok(!p.els.get.hidden && p.els.steps.hidden, "install again: the download");
  assert.deepEqual(buttons(p), []);
  plain(p);
  p = await at({ connected: false, why: "forbidden" });
  assert.equal(p.els.headline.textContent, "Update Dannex Download Manager");
  assert.equal(p.els.lead.textContent, english("page_lead_forbidden"));
  assert.ok(!p.els.get.hidden);
  plain(p);
  p = await at({ connected: false, why: "too-large" });
  assert.equal(p.els.lead.textContent, english("page_lead_too_large"));
  assert.deepEqual(buttons(p), ["retry"]);
  assert.equal(p.els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html#too-large");
  plain(p);
  // A reason this version doesn't know: as the helper failing.
  p = await at({ connected: false, why: "something-new" });
  assert.equal(p.els.lead.textContent, english("page_lead_host_failed"));
  // DDM's own Options take nothing from this browser: its one button opens them in DDM.
  p = await at({ connected: true, captureOff: "integration-off", app: "0.6.5" });
  assert.equal(p.els.headline.textContent, english("status_off_in_app"));
  assert.equal(p.els.lead.textContent, english("declined_integration_off"));
  assert.deepEqual(buttons(p), ["app-options"]);
  assert.ok(p.els["toggle-row"].hidden, "this switch can't turn DDM's on");
  assert.equal(p.els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html#options");
  p.els["app-options"].click();
  await settle();
  assert.ok(p.asked.includes("open-options"));
  plain(p);
  // DDM older than the extension: installed, with how to update it below.
  p = await at({ connected: true, why: "update-app", app: "0.5.0" });
  assert.ok(p.els.hero.classList.contains("ok") && !p.els["toggle-row"].hidden);
  assert.equal(noteText(p.els.notes.children[0]), `${english("status_update_app")} ${english("page_update_app_how")}`);
  // Another DDM chosen: where downloads go, and the switch.
  p = await at({ connected: true, why: null, app: "0.6.5", remote: "https://nas.test:8463/" });
  assert.equal(p.els.headline.textContent, english("page_remote_title"));
  assert.equal(p.els.lead.textContent, english("page_remote_lead", ["https://nas.test:8463/"]));
  assert.ok(!p.els["toggle-row"].hidden && p.els.hero.classList.contains("ok"));
  // Two copies: this one stands aside; Remove this copy asks the browser to remove it.
  p = await at({ connected: true, duplicate: "store", app: "0.6.5" });
  assert.equal(p.els.headline.textContent, english("page_duplicate_title"));
  assert.deepEqual(buttons(p), ["remove", "extensions"]);
  assert.ok(p.els["toggle-row"].hidden);
  assert.equal(p.els.help.href, "https://dannex.kavangi.co.ke/help/browser-help.html#two-copies");
  p.els.remove.click();
  await settle();
  assert.deepEqual(p.opened.at(-1), ["uninstall", { showConfirmDialog: true }]);
  p.els.extensions.click();
  assert.equal(p.opened.at(-1), "chrome://extensions/?id=ours");
  plain(p);
  ok("the status page: not running, blocked, too old, too large, off in DDM, update DDM, another DDM, two copies");
}

// The status page's notes: DDM newer (the store's copy checks for updates, the unpacked one
// reloads), the Chrome Web Store's listing for the unpacked copy, Firefox's temporary add-on,
// limited site access (Allow on all sites asks the browser), private windows, and the last
// download the browser kept, with why (#208).
{
  const notes = async (s, nav) => {
    const page = runPage("install.js", { answer: () => ({ connected: true, app: "0.7.0", version: "0.6.5", browser: "chromium", ...s }), nav });
    await settle();
    return { page, texts: page.els.notes.children.map((li) => noteText(li)), buttons: page.els.notes.children.flatMap((li) => li.children.filter((c) => c.children && c.children.length).flatMap((row) => row.children)), links: page.els.notes.children.flatMap((li) => li.children.filter((c) => c.tag === "a")) };
  };
  let n = await notes({ behind: true, copy: "store" });
  assert.deepEqual(n.texts, [`${english("page_behind", ["0.7.0"])} ${english("page_behind_store")}`]);
  n = await notes({ behind: true, copy: "unpacked" });
  assert.equal(n.buttons[0].textContent, english("page_reload"));
  n.buttons[0].click();
  assert.ok(n.page.opened.includes("reload"));
  n = await notes({ store: "https://chromewebstore.google.com/detail/dannex-download-manager/plmckdlgphkcfkifahbkonbibfedckih", copy: "unpacked" });
  assert.deepEqual(n.texts, [english("page_store")]);
  n.buttons[0].click();
  assert.equal(n.page.opened.at(-1), "https://chromewebstore.google.com/detail/dannex-download-manager/plmckdlgphkcfkifahbkonbibfedckih");
  n = await notes({ temporary: true, browser: "firefox" }, { platform: "Win32", userAgent: "Firefox/145.0" });
  assert.deepEqual(n.texts, [english("page_temporary")]);
  assert.deepEqual([n.links[0].textContent, n.links[0].href], [english("page_learn_more"), "https://dannex.kavangi.co.ke/help/browser-help.html#firefox-temporary"]);
  n = await notes({ allSites: false });
  n.buttons[0].click();
  assert.deepEqual(n.page.opened.at(-1), ["request", { origins: ["<all_urls>"] }]);
  n = await notes({ incognito: false });
  assert.deepEqual(n.texts, [`${english("page_private")} ${english("page_private_chromium")}`]);
  n = await notes({ incognito: false, browser: "firefox" }, { platform: "Win32", userAgent: "Firefox/145.0" });
  assert.deepEqual([n.texts[0], n.buttons.length], [`${english("page_private")} ${english("page_private_firefox")}`, 0], "Firefox: no page to open");
  n = await notes({ declined: { why: "file-type", name: "report.xyz", site: "x.test", at: 1 } });
  assert.deepEqual(n.texts, [`${english("popup_declined", ["report.xyz"])} ${english("declined_file_type")}`]);
  n = await notes({ declined: { why: "site-access", name: "", site: "", at: 1 } });
  assert.deepEqual(n.texts, [`${english("popup_declined_unnamed")} ${english("declined_site_access")}`]);
  n = await notes({ declined: { why: "a-reason-from-later", name: "a.zip", site: "x.test", at: 1 } });
  assert.deepEqual(n.texts, [], "a reason this version can't name: nothing");
  n = await notes({ captureOff: "browser", declined: { why: "browser", name: "a.zip", site: "x.test", at: 1 } });
  assert.deepEqual(n.texts, [], "off in DDM: the headline says it already");
  n = await notes({ connected: false, why: "not-installed", allSites: false, incognito: false, declined: { why: "small", name: "a.zip", at: 1 } });
  assert.deepEqual(n.texts, [], "DDM not there: the one thing to do, nothing else");
  n = await notes({});
  assert.deepEqual(n.texts, [], "nothing more to say");
  ok("the status page's notes: an update, the store's copy, temporary, site access, private windows, the last download kept");
}

// The options page, as small as it can be: the switch follows the toolbar button's (`paused`); a
// line only when something is wrong (with the way to the status page) or downloads go to another
// DDM; under More, the proxy switch and Send downloads to; the Chrome Web Store's page offered to
// the unpacked copy once listed, never otherwise; nothing that opens DDM, no keys hint, no media
// names.
{
  const page = runPage("options.js", { answer: (m) => (m.type === "state" ? { connected: true, app: "0.6.5", version: "0.6.5", paused: true, copy: "unpacked", store: "https://chromewebstore.google.com/detail/dannex-download-manager/plmckdlgphkcfkifahbkonbibfedckih" } : { ok: true }) });
  await settle();
  assert.equal(page.els.take.checked, false);
  assert.ok(!page.els.store.hidden);
  assert.ok(page.els.status.hidden, "all well: no line");
  for (const gone of ["open", "app-options", "panel", "keys-hint", "media-title-name", "credit"]) assert.equal(page.els[gone], undefined, `no #${gone}`);
  const plain = runPage("options.js", { answer: (m) => (m.type === "state" ? { connected: true, app: "0.6.5", version: "0.6.5", paused: false } : { ok: true }) });
  await settle();
  assert.ok(plain.els.take.checked && plain.els.store.hidden && plain.els.status.hidden);
  assert.ok(!(plain.els.more && plain.els.more.open), "More closed");
  assert.equal(plain.els.version.textContent, "Dannex Download Manager 0.6.5");
  assert.deepEqual([plain.els.help.href, plain.els.support.href], ["https://dannex.kavangi.co.ke/help/browser-help.html", "https://github.com/sponsors/Riskraptor"]);
  // Turned off here: the toolbar button's switch.
  plain.els.take.checked = false;
  await plain.els.take.fire("change", { target: { checked: false } });
  await settle();
  assert.deepEqual(plain.sent.find((m) => m.type === "pause"), { type: "pause", paused: true });
  // Turned off by the toolbar button meanwhile (a change in storage): the page follows from the
  // change itself, without asking DDM again; only a new status asks.
  const states = () => plain.asked.filter((t) => t === "state").length;
  const asked = states();
  await plain.changed({ paused: { oldValue: false, newValue: true } });
  assert.equal(plain.els.take.checked, false);
  await plain.changed({ paused: { oldValue: true, newValue: false } });
  assert.ok(plain.els.take.checked && plain.els.status.hidden);
  await plain.changed({ menus: { newValue: "x" }, declined: { newValue: null } });
  assert.equal(states(), asked, "nothing asked of DDM for those");
  await plain.changed({ status: { oldValue: { why: null }, newValue: { why: "not-installed" } } });
  assert.equal(states(), asked + 1, "a new status: asked");
  await plain.changed({ remote: { oldValue: null, newValue: { url: "https://nas.test:8463/", token: "t" } } });
  assert.equal(states(), asked + 2, "another DDM paired: asked");
  // DDM not reached: a line says so, with the way to the status page.
  const missing = runPage("options.js", { answer: () => ({ connected: false, why: "not-installed", version: "0.6.5", paused: false }) });
  await settle();
  assert.ok(!missing.els.status.hidden && missing.els.status.classList.contains("bad"));
  assert.equal(missing.els["status-text"].textContent, "Dannex Download Manager isn't installed");
  assert.ok(!missing.els["status-page"].hidden);
  // Another DDM: where downloads go, More open on it.
  const remote = runPage("options.js", { answer: () => ({ connected: true, why: null, version: "0.6.5", paused: false, remote: "https://nas.test:8463/" }) });
  await settle();
  assert.ok(!remote.els.status.hidden && remote.els.status.classList.contains("ok") && remote.els["status-page"].hidden);
  assert.equal(remote.els["status-text"].textContent, english("status_remote", ["https://nas.test:8463/"]));
  assert.ok(remote.els.more.open && remote.els["to-remote"].checked);
  ok("the options page: the switch, a line only when needed, More; the store's page for the unpacked copy only");
}

// The options page's own words, as a person reads them: plain, no developer's words.
{
  const read = (f) => readFileSync(new URL(`../../extension/${f}`, import.meta.url), "utf8");
  for (const page of ["options.html", "install.html"]) {
    for (const [, k] of read(page).matchAll(/data-i18n(?:-placeholder)?="(\w+)"/g)) {
      assert.ok(!TECHNICAL.test(k === "name" ? "" : english(k)), `${page}: ${k}: ${english(k)}`);
    }
  }
  for (const k of Object.keys(pageEnglish).filter((k) => /^(status|page|tip|click|toolbar|options|declined|popup)_/.test(k))) {
    assert.ok(!TECHNICAL.test(english(k, ["https://nas.test/", "x"])), `${k}: ${english(k)}`);
  }
  ok("the extension's words: plain, no developer's words");
}
assert.deepEqual(complaints, [], "nothing written to the console as an error or warning");

console.log(`extension: ${passed} checks passed`);
