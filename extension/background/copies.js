// Two copies in one browser (the store's and the unpacked one): which one stands aside. A part of
// the background: background.js loads the parts in order, into one scope.

// ---- two copies in one browser ------------------------------------------------------------------

// This copy's rank among COPIES (an unknown ID, a build with a key of its own: after them all).
const ownRank = () => {
  const i = COPIES.indexOf(ext.runtime.id);
  return i < 0 ? COPIES.length : i;
};
// Which copy this is: "store", "unpacked", or "" (Firefox's, Safari's, a build of its own).
const thisCopy = () => ["store", "unpacked"][COPIES.indexOf(ext.runtime.id)] || "";
// Only Chromium browsers can have two: Firefox's add-on has one ID wherever it comes from, and
// Safari's is the app's. Extensions reach each other by ID there (no externally_connectable in the
// manifest: any extension may ask, no page can); a copy answers ours only.
const CROSS = !SAFARI && !FIREFOX && typeof ext.runtime.sendMessage === "function";
// {dormant, better, at}: whether a preferred copy is in this browser, working, and this one stands
// aside; null until asked. Kept in the browser's session storage too (as the number of downloads
// is), so a script the browser starts again knows at once, and never draws the button "on", takes
// a download or flips its switch before it has asked (`copyState` reads it back first).
let copies = null;
let copyCheck = null;
const COPY_MS = 60000;
// At a download, asked again when older than this (the other copy answers within milliseconds):
// a copy turned off or removed meanwhile gives the work back at the next download, and one turned
// on again takes it, never both.
const COPY_DOWNLOAD_MS = 3000;
const COPY_ASK_MS = 2500;
const ALONE = { dormant: false, better: null };
const copiesBack =
  CROSS && ownRank() > 0 && ext.storage.session
    ? Promise.resolve(ext.storage.session.get({ copies: null }))
        .then((v) => {
          const kept = v && v.copies;
          if (!copies && kept && typeof kept.dormant === "boolean" && Number.isFinite(kept.at)) copies = kept;
        })
        .catch(() => {})
    : Promise.resolve();

// The copy `id` when it is installed, turned on and answers: {version, why, reached, paused}, else
// null.
async function askCopy(id) {
  try {
    const reply = await withTimeout(Promise.resolve(ext.runtime.sendMessage(id, { type: "ddm-copy", version: VERSION })), COPY_ASK_MS);
    return reply && reply.ddmCopy === true ? reply : null;
  } catch {
    return null; // not installed, turned off, or not one of ours
  }
}

// Whether a copy's answer means this one stands aside: when it reaches DDM and takes downloads
// (its `why` none, or only "update DDM"; it asked DDM again just before answering, so a reason
// from before doesn't count), or when the user turned it off. A store copy that can't reach DDM
// (DDM refusing its ID, not found, its helper failing, never reached) takes nothing, so this one
// goes on. A store copy turned off keeps this one off too: in one browser, OFF means OFF, never
// a silent takeover by the other copy.
function standsAsideFor(there) {
  if (!there) return false;
  if (there.paused === true) return true;
  return there.reached === true && (!there.why || there.why === "update-app");
}

async function checkCopies() {
  let better = null;
  for (const id of COPIES.slice(0, ownRank())) {
    if (id === ext.runtime.id) continue;
    if (standsAsideFor(await askCopy(id))) {
      better = id;
      break;
    }
  }
  const was = Boolean(copies && copies.dormant);
  copies = { dormant: Boolean(better), better, at: Date.now() };
  if (ext.storage.session) Promise.resolve(ext.storage.session.set({ copies })).catch(() => {});
  // Not awaited: they ask `copyState` themselves, which answers from `copies` now.
  if (was !== copies.dormant) {
    syncMenus();
    syncOpenedRule();
    showStatus();
  }
  return copies;
}

// Whether this copy stands aside, asked again when older than `maxAge` (a download, a page's
// question, the toolbar button); never answered before it is known: read back from the session,
// else asked.
async function copyState(maxAge = COPY_MS) {
  if (!CROSS || ownRank() === 0) return ALONE;
  await copiesBack;
  if (copies && Date.now() - copies.at < maxAge) return copies;
  copyCheck = copyCheck || checkCopies().finally(() => (copyCheck = null));
  return copyCheck;
}

// What this copy answers another copy of ours: whether it reaches DDM and takes downloads. With no
// rules yet, or a reason from before (a download DDM didn't start in time, the helper failing
// once), it asks DDM again first, briefly: a stale reason would have the other copy take what this
// one takes too.
async function copyAnswer() {
  let s = await readStatus();
  if (!(await storedRules()) || (s.why && s.why !== "update-app")) {
    await withTimeout(refreshRules(), 1500).catch(() => {});
    s = await readStatus();
  }
  const { paused } = await ext.storage.local.get({ paused: false });
  return { ddmCopy: true, version: VERSION, why: s.why || null, reached: Boolean(await storedRules()), paused: Boolean(paused) };
}

// Another copy of ours asking (only ours: their IDs): whether this one is here and works. A copy
// installed, updated, started or turned on tells the ones after it, which ask again.
if (ext.runtime.onMessageExternal) {
  ext.runtime.onMessageExternal.addListener((m, sender, sendResponse) => {
    if (!m || !sender || !COPIES.includes(sender.id) || sender.id === ext.runtime.id) return false;
    if (m.type === "ddm-copy") {
      copyAnswer()
        .then(sendResponse)
        .catch(() => sendResponse({ ddmCopy: true, version: VERSION, why: null, reached: false, paused: false }));
      return true;
    }
    if (m.type === "ddm-copy-changed") copyState(0).catch(() => {});
    return false;
  });
}

// This copy installed, updated, started or turned on: the copies after it ask again whether to
// stand aside. (Being turned off or removed, it can tell no one: the browser says so only to an
// extension with the "management" permission, which this one doesn't ask for. The other copy asks
// again at its next download instead: COPY_DOWNLOAD_MS.)
function tellOtherCopies() {
  if (!CROSS) return;
  for (const id of COPIES.slice(ownRank() + 1)) {
    Promise.resolve(ext.runtime.sendMessage(id, { type: "ddm-copy-changed" })).catch(() => {});
  }
}

// Turned on again in the browser's extensions page, this copy's script starts with nothing else
// said (no install, no browser start): it tells the other copies then too, once each time (the
// browser empties the session storage when an extension is turned off, updated or the browser
// restarts), not at every wake of its script.
if (CROSS && ext.storage.session && ownRank() + 1 < COPIES.length) {
  Promise.resolve(ext.storage.session.get({ toldCopies: false }))
    .then((v) => {
      if (v && v.toldCopies) return;
      tellOtherCopies();
      return ext.storage.session.set({ toldCopies: true });
    })
    .catch(() => {});
}
