// Download capture: a browser download checked against the rules, offered to the app, and cancelled
// once the app has it (or left to the browser, said why). A part of the background: background.js
// loads the parts in order, into one scope.

// `explicit`: the user asked for this one (the right-click item), so Options' automatic-capture
// rules (file types, excluded sites) don't apply; the host checks them for everything else.
async function handOff(d) {
  const message = {
    url: d.url,
    finalUrl: d.finalUrl && d.finalUrl !== d.url ? d.finalUrl : "",
    referrer: d.referrer || "",
    cookies: await cookieHeader(d.url, d),
    filename: d.filename || "",
    ...(d.size > 0 ? { size: d.size } : {}),
    ...(d.mime ? { mime: d.mime } : {}),
    ...(d.post ? { post: d.post.body, postType: d.post.type || "" } : {}),
    ...(d.incognito ? { private: true } : {}),
    ...(d.explicit ? { explicit: true } : {}),
    // Through the browser's proxy too (#42): the one its own request for the file took.
    ...(await browserProxy(d.finalUrl || d.url, d.incognito)),
  };
  return native(message);
}

// Colab's runtime proxy uses an ephemeral token tied to the browser's partitioned session.
// Only the actual proxy transfer URL is left to the browser: a normal file linked from a Colab
// notebook must still be capturable by DDM (#603).
function colabRuntimeProxy(...urls) {
  return urls.some((value) => {
    if (!value) return false;
    try {
      const u = new URL(value);
      const host = u.hostname.toLowerCase();
      const proxyHost =
        host === "prod.r.colab.dev"
        || host.endsWith(".prod.r.colab.dev")
        || /(?:^|[-.])colab\.googleusercontent\.com$/.test(host);
      return proxyHost && u.searchParams.has("colab-runtime-proxy-token");
    } catch {
      return false;
    }
  });
}

// The file name to offer the app the download under (`{name}`), or why Options leave it to
// the browser (`{left, name}`), or null: the browser keeps it and there is nothing to say (not a
// web address, paused, Alt held, a form that can't be posted again, DDM not reached).
async function candidate(item, url) {
  if (!/^https?:/i.test(url)) return null; // blob:, data:, file: stay the browser's
  const { paused } = await ext.storage.local.get({ paused: false });
  if (paused) return null;
  // The Chrome Web Store's copy is in this browser too and takes it: never both.
  if ((await copyState(COPY_DOWNLOAD_MS)).dormant) return null;
  // An Alt-click is the next download of that link's, however slow its server is to answer (half
  // a minute), and that one only: a plain click on the same link afterwards is DDM's again.
  forget(bypass, 30000);
  const first = firstUrl(item);
  const held = [item.url, url, first].filter((u) => bypass.has(u));
  if (held.length) {
    for (const u of held) bypass.delete(u);
    return null;
  }
  if (postOf(first) === null) return null; // a form not to post again
  if (colabRuntimeProxy(first, url)) return null;
  const r = await currentRules();
  if (!r) return null;
  const name = (item.filename || "").split(/[\\/]/).pop() || url.split(/[?#]/)[0].split("/").pop() || "";
  const left = (why) => ({ left: why, name });
  if (!r.capture) return left(r.captureOff || "browser");
  const by = excludedBy(r, first) || excludedBy(r, url);
  if (by) return left(by);
  const type = extensionOf(name);
  if (!type || !typePatterns(r.types).some((p) => p.test(type))) return left("file-type");
  // Smaller than Options' minimum (when the browser knows the size already).
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize;
  if (r.minKb > 0 && size > 0 && size < r.minKb * 1024) return left("small");
  if (!(await siteAccess(first))) return left("site-access");
  return { name };
}

// Why it stays with the browser, said (the status page, DDM's log).
function leftAlone(c, item, url) {
  if (c && c.left) noteLeft(c.left, c.name, firstUrl(item) || url, item.incognito, false).catch(() => {});
}

// Offer the download to the app under `name`; true if it took it.
async function offer(item, url, name) {
  const first = firstUrl(item);
  const reply = await handOff({
    url: first,
    finalUrl: url,
    referrer: item.referrer,
    filename: name,
    size: item.totalBytes > 0 ? item.totalBytes : item.fileSize,
    mime: item.mime,
    post: postOf(first),
    incognito: item.incognito,
    storeId: item.cookieStoreId,
  });
  if (reply.ok) {
    await ext.storage.local.remove("declined").catch(() => {});
  } else {
    // Refused by Options in the app (changed since the extension asked, or a rule only the app
    // checks, as a redirect's end): the host logged it; the status page says it.
    const why = declinedOf(reply);
    if (why) await noteLeft(why, name, first, item.incognito, true).catch(() => {});
  }
  return Boolean(reply.ok);
}

// Safari: a link clicked in a page (page.js) that may be a download, offered as the browser's own
// downloads are: through `candidate` (Options' rules, Alt, pause) then `offer`. `{taken: true}`
// when the app took it; else page.js lets the click go on, so nothing is lost. The link's file
// name is its `download` attribute's (`name`), else its address's (Safari names a download only
// when it starts, which is too late).
// The name an address gives, readable: the app saves a name as it is told it (the browsers' own
// downloads come already named), so `My%20file.zip` would stay that.
function addressName(url) {
  const last = url.split(/[?#]/)[0].split("/").pop() || "";
  try {
    return decodeURIComponent(last);
  } catch {
    return last; // not valid percent-encoding
  }
}

async function link(url, referrer, incognito, name) {
  // What the browser opens itself (a PDF, a song) is taken only when Options say so (`opened`,
  // "Also take files the browser would open"), as in the other browsers.
  const r = await currentRules();
  const type = extensionOf(name || url.split(/[?#]/)[0]).toUpperCase();
  if (r && r.opened === false && OPENED[type]) return { taken: false };
  const filename = name || addressName(url);
  const item = { url, referrer, incognito, ...(filename ? { filename } : {}) };
  const c = await candidate(item, url).catch(() => null);
  if (!c || c.left) {
    leftAlone(c, item, url);
    return { taken: false };
  }
  return { taken: await offer(item, url, c.name).catch(() => false) };
}

// Tabs opened in the last seconds: one the download was opened in (a link opened in a new tab
// or window, Shift+click) is closed with it, as when the browser keeps the download.
const newTabs = new Map();
ext.tabs.onCreated.addListener((tab) => {
  forget(newTabs, 10000);
  newTabs.set(tab.id, { at: Date.now() });
});

async function closeLeftTab(urls) {
  forget(newTabs, 10000);
  for (const id of newTabs.keys()) {
    try {
      const tab = await ext.tabs.get(id);
      const at = tab.pendingUrl || tab.url || "";
      // Only a tab opened for the download's own address (never a new tab the user opened).
      if (urls.includes(at)) {
        newTabs.delete(id);
        await ext.tabs.remove(id);
      }
    } catch {
      newTabs.delete(id);
    }
  }
}

// The browser's download, gone without a trace once the app has it.
async function drop(item) {
  const id = item.id;
  try {
    await ext.downloads.cancel(id);
  } catch {
    // Already finished: its file goes.
  }
  try {
    const [d] = await ext.downloads.search({ id });
    if (d && d.state === "complete" && d.exists !== false) await ext.downloads.removeFile(id);
  } catch {
    // Gone already.
  }
  ext.downloads.erase({ id }).catch(() => {});
  closeLeftTab([firstUrl(item), item.url, item.finalUrl].filter(Boolean)).catch(() => {});
}

// A download left to the browser keeps the name it has, or the one another extension gives it
// (Save In's folders, #578; a `downloads.download` with a `filename`). Chromium counts every
// listener's answer as a name of its own, an empty one too: `suggest()` without a suggestion
// reaches it as an empty name with no conflict action, and when the most recently installed
// extension answers so, its empty name wins and the others' are dropped (the download lands in
// the Downloads folder, and the browser may warn of a conflict). Only an answer with
// "uniquify" and no name is no suggestion at all (downloads_api.cc, DeterminerCallback), and
// the binding lets no empty name through: a name the browser refuses, which it empties after
// counting the answer, is how a listener gives one, but the browser logs it as an error
// ("Unchecked runtime.lastError: Invalid filename") in the extension's Errors. So it is given
// only for a download another extension started (its `downloads.download` name and folder, Save
// In's); a page's download is answered with no suggestion, which leaves the browser's own name.
function noName(suggest, item) {
  if (item && item.byExtensionId) suggest({ filename: "..", conflictAction: "uniquify" });
  else suggest();
}

if (SAFARI) {
  // Nothing to listen to: see `link`.
} else if (!FIREFOX) {
  // Chromium: once the final file name is known (after redirects and Content-Disposition). The
  // download waits for `suggest`, writing nothing, while the app is asked: taken, it is
  // cancelled; else it goes on as the browser's. So it is never lost or kept by both (the app
  // asks the address once more: a form's download is posted again with its body).
  ext.downloads.onDeterminingFilename.addListener((item, suggest) => {
    const url = item.finalUrl || item.url;
    candidate(item, url)
      .then(async (c) => {
        const taken = Boolean(c && c.name && !c.left) && (await offer(item, url, c.name));
        if (taken) return drop(item);
        // The browser goes on first; why is said after.
        noName(suggest, item);
        leftAlone(c, item, url);
      })
      .catch(() => noName(suggest, item));
    return true; // suggest() comes later
  });
} else {
  // Firefox names the file before the download is created and has no such pause. Most are
  // caught earlier, from the answer to the page load (`headersReceived`, below); the others (a
  // download started by a page's script, a type the answer didn't say) here: the download is
  // paused at once when it may be the app's, then cancelled or resumed; one that finished
  // meanwhile (a small file) has its file removed.
  // A page load decided from its answer already (`headersReceived`, #208) isn't asked again.
  ext.downloads.onCreated.addListener(async (item) => {
    const url = item.finalUrl || item.url;
    forget(decided, 120000);
    if (decided.has(url) || decided.has(firstUrl(item))) return;
    const c = await candidate(item, url).catch(() => null);
    if (!c || c.left) return leftAlone(c, item, url);
    const name = c.name;
    let paused = false;
    try {
      await ext.downloads.pause(item.id);
      paused = true;
    } catch {
      // Not pausable (no resume support), or finished already.
    }
    const taken = await offer(item, url, name).catch(() => false);
    if (taken) drop(item);
    else if (paused) ext.downloads.resume(item.id).catch(() => {});
  });
}
