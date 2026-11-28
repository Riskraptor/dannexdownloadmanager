// Files the browser would open itself (a PDF, an MP3), made downloads. A part of the background:
// background.js loads the parts in order, into one scope.

// ---- files the browser would open itself (a PDF, an MP3): made downloads -----------------

// Options → General in the app: also take the listed types the browser shows or plays when a
// link leads to one. Chromium: a session rule marks such answers as attachments, so they come
// down the download path above (where the rules and Alt apply). Firefox: taken from the answer
// as a download is (`headersReceived`), else it opens.
const OPENED = {
  PDF: ["application/pdf"],
  MP3: ["audio/mpeg", "audio/mp3"],
  MP4: ["video/mp4"],
  M4A: ["audio/mp4", "audio/x-m4a"],
  M4V: ["video/x-m4v"],
  OGG: ["audio/ogg"],
  OGV: ["video/ogg"],
  WAV: ["audio/wav", "audio/x-wav", "audio/wave"],
  AAC: ["audio/aac"],
  MOV: ["video/quicktime"],
  TIF: ["image/tiff"],
  TIFF: ["image/tiff"],
  WMA: ["audio/x-ms-wma"],
  WMV: ["video/x-ms-wmv"],
  AVI: ["video/x-msvideo", "video/avi"],
  MKV: ["video/x-matroska"],
};
const OPENED_RULE = 1;
// The allow rules for exact excluded hosts follow it (ids from 2).
const OPENED_ALLOW = 2;
let openedTypes = [];
// The start's first reading of the rules (`openedTypes` set from them), set where this script
// starts. A Firefox page load can be what wakes this script: its answer waits for it, else a PDF
// or an MP3 Options take was left to open in the browser at the first click after a while idle.
let openedReady = Promise.resolve();

// Options' excluded sites as the rules need them, read as the app reads them: `*.example.com`
// is the domain and its subdomains (a rule's domain condition), a plain name that host only
// (an allow rule of its own). As the browser writes a host: no port, punycode.
function excludedHosts() {
  const wide = [];
  const exact = [];
  for (const raw of ((rules && rules.excludedSites) || "").split(/\s+/)) {
    const p = raw.trim().replace(/^[a-z]+:\/\//i, "").split("/")[0];
    const sub = p.startsWith("*.");
    let host = "";
    try {
      host = new URL(`http://${sub ? p.slice(2) : p}`).hostname;
    } catch {
      continue;
    }
    if (!host || host.includes("*")) continue;
    (sub ? wide : exact).push(host);
  }
  return { wide, exact };
}

function openedMimes(r) {
  if (!r || !r.capture || !r.opened) return [];
  const listed = new Set(r.types.split(/\s+/).map((s) => s.toUpperCase()));
  return [...new Set(Object.entries(OPENED).flatMap(([k, m]) => (listed.has(k) ? m : [])))];
}

// Options' excluded address pattern (`*` any run of characters; without a scheme, the address after
// it) as a rule's regular expression, read as `addressMatches` reads it.
function addressRegex(pattern) {
  const p = String(pattern).trim().toLowerCase();
  if (!p) return null;
  const body = p
    .split("*")
    .map((s) => s.replace(/[.+?^${}()|[\]\\]/g, "\\$&"))
    .join(".*");
  return p.includes("://") ? `^${body}$` : `^[a-z][a-z0-9+.-]*://${body}$`;
}

// One update at a time: two at once (the start's, a pause from the options page) each read the rules
// before either wrote, and the later one's could fail and leave the earlier one's in place.
let openedWork = Promise.resolve();
function syncOpenedRule() {
  openedWork = openedWork.then(syncOpenedRuleNow, syncOpenedRuleNow);
  return openedWork;
}

async function syncOpenedRuleNow() {
  const { paused } = await ext.storage.local.get({ paused: false });
  const { why } = await readStatus();
  // Off, DDM not reached, or this copy standing aside (the other copy keeps its own rule).
  const aside = (await copyState()).dormant;
  openedTypes = paused || aside || (why && why !== "update-app") ? [] : openedMimes(rules);
  const dnr = ext.declarativeNetRequest;
  if (FIREFOX || !dnr || !dnr.updateSessionRules) return;
  const hosts = excludedHosts();
  const rule = {
    id: OPENED_RULE,
    priority: 1,
    action: {
      type: "modifyHeaders",
      responseHeaders: [{ header: "content-disposition", operation: "set", value: "attachment" }],
    },
    condition: {
      resourceTypes: ["main_frame"],
      responseHeaders: [{ header: "content-type", values: openedTypes.map((m) => `${m}*`) }],
      // Options' excluded sites keep opening in the browser.
      ...(hosts.wide.length ? { excludedRequestDomains: hosts.wide } : {}),
    },
  };
  const esc = (h) => h.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Options' excluded addresses keep opening too: the extension leaves them to the browser, which
  // would otherwise save the file the rule made an attachment instead of showing it.
  const filters = [
    ...hosts.exact.map((h) => `^[a-z]+://${esc(h)}(:[0-9]+)?/`),
    ...((rules && rules.excludedAddresses) || []).map(addressRegex).filter(Boolean),
  ];
  const allows = filters.slice(0, 500).map((regexFilter, i) => ({
    id: OPENED_ALLOW + i,
    priority: 2,
    action: { type: "allow" },
    condition: { resourceTypes: ["main_frame"], regexFilter },
  }));
  try {
    const old = (await dnr.getSessionRules()).map((r) => r.id);
    await dnr.updateSessionRules({
      removeRuleIds: old.filter((id) => id >= OPENED_RULE),
      addRules: openedTypes.length ? [rule, ...allows] : [],
    });
  } catch {
    // A browser without header conditions (Chrome before 128): it opens them as before.
  }
}
