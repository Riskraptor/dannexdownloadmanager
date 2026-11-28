// Firefox: a download caught from its answer, before the browser's dialog (#208). A part of the
// background: background.js loads the parts in order, into one scope.

// ---- Firefox: a download caught from its answer, before the browser's dialog (#208) ----------

// Firefox creates a download (downloads.onCreated, above) only once it has a file to save to:
// when it asks first ("What should Firefox do with this file?", which Waterfox always asks for
// a new type), only after the user answered, while it downloads the whole file behind the
// dialog. So a page load whose answer would be a download is caught as the answer arrives: the
// load waits while the app is asked, as Chromium's download does, and is cancelled when the app
// took it (no dialog, no browser download). Else it goes on as before, and the download it
// becomes is left to the browser (`decided`). A file the browser would show or play (a PDF, an
// MP3) is taken the same way when Options say so (`openedTypes`); else it opens.

// What Firefox shows or plays itself: a download only when its answer says "attachment".
const SHOWN = /^(text\/|image\/|video\/|audio\/|multipart\/|application\/(pdf|xhtml\+xml|xml|json|javascript|x-javascript|ecmascript|ogg|x-xpinstall|[\w.+-]+\+(xml|json))$)/;

// The file name a Content-Disposition gives (filename*=UTF-8''…, else filename="…"), or "".
function dispositionName(cd) {
  const star = /filename\*\s*=\s*([\w-]+)'[^']*'([^;]+)/i.exec(cd);
  if (star) {
    try {
      return decodeURIComponent(star[2].trim().replace(/^"|"$/g, ""));
    } catch {
      // Not percent-encoded as it says: the plain name below.
    }
  }
  const plain = /(?:^|;)\s*filename\s*=\s*("((?:[^"\\]|\\.)*)"|[^;]+)/i.exec(cd);
  if (!plain) return "";
  return decodedPlain((plain[2] !== undefined ? plain[2].replace(/\\(.)/g, "$1") : plain[1]).trim());
}

// A plain `filename` as sent, or its percent-encoded UTF-8 decoded when that is what it holds
// (non-ASCII once decoded: `100%25.txt` stays as it is), as the app reads it (naming.rs).
function decodedPlain(name) {
  if (!name.includes("%")) return name;
  try {
    const decoded = decodeURIComponent(name);
    return /[^\x00-\x7f]/.test(decoded) ? decoded : name;
  } catch {
    return name; // not percent-encoded UTF-8
  }
}

// A response header's value as text. Firefox gives its bytes one character each (ISO-8859-1), so
// a name sent as raw UTF-8 (`filename="تقرير.pdf"`) arrived as `ØªÙ…` (#447). It is read as UTF-8
// when those bytes are valid UTF-8, as Firefox reads the name for its own downloads and the app
// for its own requests; bytes that aren't (ISO-8859-1's `café`) stay as they came.
function headerText(value) {
  if (!/[\x80-\xff]/.test(value) || /[^\x00-\xff]/.test(value)) return value;
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(value, (c) => c.charCodeAt(0)));
  } catch {
    return value;
  }
}

// {name, size, mime, opened} when this answer to a page load becomes a download (or is one of
// the files Options take though the browser would open them), else null: a page, a picture, a
// redirect, an error, a piece of a file.
function answerFile(d) {
  const code = d.statusCode || 0;
  if (code < 200 || code > 299 || code === 204 || code === 205 || code === 206) return null;
  const headers = d.responseHeaders || [];
  const get = (n) => ((headers.find((h) => h.name.toLowerCase() === n) || {}).value || "").trim();
  const mime = get("content-type").split(";")[0].trim().toLowerCase();
  const cd = get("content-disposition");
  const attachment = /^attachment/i.test(cd);
  const opened = !attachment && openedTypes.some((m) => mime.startsWith(m));
  // No type: the browser guesses from the bytes (a page, mostly); a download it becomes is
  // still seen by onCreated.
  if (!attachment && !opened && (!mime || SHOWN.test(mime))) return null;
  let name = dispositionName(headerText(cd));
  if (!name) {
    try {
      name = decodeURIComponent(pathOf(d.url).split("/").pop() || "");
    } catch {
      name = pathOf(d.url).split("/").pop() || "";
    }
  }
  name = name.split(/[\\/]/).pop() || "";
  // The size the file will have: the length of an answer that isn't compressed on the way.
  const size = get("content-encoding") ? 0 : Number(get("content-length")) || 0;
  return { name, size, mime, opened };
}

// The page loads decided here (by their first and last addresses, two minutes): the download
// one of them becomes after the browser's dialog is left to the browser (asked once).
const decided = new Map();

// A tab opened for the download only (a link opened in a new tab or window), closed with it.
async function closeDownloadTab(tabId, urls) {
  forget(newTabs, 10000);
  if (!(tabId >= 0) || !newTabs.has(tabId)) return;
  try {
    const tab = await ext.tabs.get(tabId);
    const at = tab.url || "";
    if (at === "about:blank" || urls.includes(at)) {
      newTabs.delete(tabId);
      await ext.tabs.remove(tabId);
    }
  } catch {
    // Closed already.
  }
}

async function headersReceived(d) {
  if (d.method !== "GET" && d.method !== "POST") return {};
  await openedReady;
  const file = answerFile(d);
  if (!file) return {};
  const ref = referrers.get(d.requestId);
  const item = {
    url: d.url,
    filename: file.name,
    totalBytes: file.size,
    fileSize: file.size,
    mime: file.mime,
    referrer: (ref && ref.url) || "",
    incognito: Boolean(d.incognito),
    cookieStoreId: d.cookieStoreId,
  };
  if (PROXY_DECLARED) {
    await proxyChecked;
    noteProxy(d);
  }
  const first = firstUrl(item);
  forget(decided, 120000);
  decided.set(d.url, { at: Date.now() });
  decided.set(first, { at: Date.now() });
  const c = await candidate(item, d.url).catch(() => null);
  if (!c || c.left) {
    // A file the browser opens stays a page: there is no download to say anything about.
    if (!file.opened) leftAlone(c, item, d.url);
    return {};
  }
  const taken = await offer(item, d.url, c.name).catch(() => false);
  if (!taken) return {};
  closeDownloadTab(d.tabId, [first, d.url]).catch(() => {});
  return { cancel: true };
}

if (FIREFOX && ext.webRequest && ext.webRequest.onHeadersReceived) {
  ext.webRequest.onHeadersReceived.addListener(
    (d) => headersReceived(d).catch(() => ({})),
    { urls: ["<all_urls>"], types: ["main_frame", "sub_frame"] },
    ["blocking", "responseHeaders"],
  );
}
