// The status: the toolbar button's look and tooltip, its click, and the status page. A part of the
// background: background.js loads the parts in order, into one scope.

// ---- status: the toolbar button's look and tooltip, the status page --------------------------

let status = null; // {why, app, at, videoLookup}
async function readStatus() {
  if (!status) status = (await ext.storage.local.get({ status: { why: null } })).status;
  return status;
}

// `why`: null when connected, else the reason (the help page's section). Written only when it
// changes.
async function noteStatus(why, app) {
  const now = await readStatus();
  if (now.why === why && (!app || now.app === app)) return;
  const was = !now.why || now.why === "update-app";
  status = { ...now, why, app: app || now.app || null, at: Date.now() };
  await ext.storage.local.set({ status });
  await showStatus();
  // Unreachable, PDFs and music open in the browser again (and back once connected).
  if (was !== (!why || why === "update-app")) await syncOpenedRule();
}

// Whether this computer's DDM looks videos up for the button over videos (its qualities listed in
// the page, video.js), kept with the status beside the app's version: from every answer to
// `rules`, and dropped when a lookup's answer says otherwise (an older host or app).
async function noteLookup(on) {
  const now = await readStatus();
  if (Boolean(now.videoLookup) === on) return;
  status = { ...now, videoLookup: on };
  await ext.storage.local.set({ status });
}

// What the toolbar button shows and what a click on it does, from what is known now:
// - `duplicate`: this copy stands aside for the Chrome Web Store's (`copyState`): grey, an amber
//   "!", a click opens the status page (which offers to remove this copy);
// - `problem`: DDM can't be reached (`why`): grey, a red "!", a click opens the status page;
// - `off-in-app`: DDM's Options take nothing from this browser (integration off, the browser
//   unticked): grey, "OFF", a click opens the status page (the switch here can't change that);
// - `off`: turned off here: grey, "OFF", a click turns it on;
// - `on`: in colour and no badge, so it looks as any button does; a click turns it off.
// Its tooltip says, in plain words, how things stand and what a click does. {state, icon, badge,
// color, title, click}.
const BADGE_RED = "#C42B1C";
const BADGE_AMBER = "#9D5D00";
const BADGE_GREY = "#6B6B6B";
async function toolbarLook() {
  const lines = (...l) => l.filter(Boolean).join("\n");
  const remote = await remoteTarget();
  const s = await readStatus();
  const why = remote ? null : s.why;
  const r = await storedRules();
  const { paused } = await ext.storage.local.get({ paused: false });
  // Asked first when not known yet (a script the browser just started): never drawn "on" by
  // mistake, and a click never flips the switch of a copy standing aside.
  if ((await copyState()).dormant) {
    return { state: "duplicate", icon: "off", badge: "!", color: BADGE_AMBER, title: lines(t("status_duplicate"), t("click_fix")), click: "status" };
  }
  if (why && why !== "update-app") {
    return { state: "problem", icon: "off", badge: "!", color: BADGE_RED, title: lines(t(`status_${why.replace(/-/g, "_")}`), t("click_fix")), click: "status" };
  }
  if (!remote && r && r.capture === false) {
    const reason = t(`declined_${String(r.captureOff || "browser").replace(/-/g, "_")}`);
    return { state: "off-in-app", icon: "off", badge: t("badge_off"), color: BADGE_AMBER, title: lines(t("status_off_in_app"), reason, t("click_details")), click: "status" };
  }
  if (paused) {
    return { state: "off", icon: "off", badge: t("badge_off"), color: BADGE_GREY, title: t("tip_off"), click: "toggle" };
  }
  return {
    state: "on",
    icon: "on",
    badge: "",
    color: BADGE_GREY,
    title: lines(remote ? t("tip_remote", remote.url) : t("tip_on"), why === "update-app" ? t("status_update_app") : ""),
    // Never reached yet (just installed, storage cleared): asked first.
    click: r || remote ? "toggle" : "check",
  };
}

// A moment's sign on the badge (a site left to the browser from the right-click menu: "✓").
let flash = null; // {text, color, until}
function flashBadge(text, color, ms = 2500) {
  flash = { text, color, until: Date.now() + ms };
  showStatus();
  setTimeout(() => {
    if (flash && Date.now() >= flash.until) flash = null;
    showStatus();
  }, ms);
}

// The toolbar button as `toolbarLook` says, for every tab (the switch is the browser's, not a
// tab's: a tab opened later is as the others).
async function showStatus() {
  const look = await toolbarLook();
  const badge = flash && Date.now() < flash.until ? flash : { text: look.badge, color: look.color };
  try {
    await action.setBadgeBackgroundColor({ color: badge.color });
    await action.setBadgeText({ text: badge.text });
    await action.setTitle({ title: look.title });
    if (action.setIcon) await action.setIcon({ path: ICONS[look.icon] });
  } catch {
    // No toolbar button (a browser without one): the status page and the help page still say it.
  }
}

// Taking downloads from this browser turned off (`paused`) or on: the toolbar button, the rule
// that makes PDFs and music downloads, and the pages showing it follow.
async function setPaused(paused) {
  await ext.storage.local.set({ paused: Boolean(paused) });
  await syncOpenedRule();
  await showStatus();
}

// A click on the toolbar button: turns taking downloads off or on while DDM is reached; else the
// status page says what is wrong (or that this copy should go).
async function toolbarClick() {
  let look = await toolbarLook();
  if (look.click === "check") {
    await refreshRules();
    look = await toolbarLook();
  }
  if (look.click === "toggle") {
    const { paused } = await ext.storage.local.get({ paused: false });
    return setPaused(!paused);
  }
  return openStatusPage();
}

if (action && action.onClicked) action.onClicked.addListener(() => toolbarClick().catch(() => {}));

// The extension's page `page` in a tab: the one open already, brought forward, else a new one.
async function openOwnPage(page) {
  const url = ext.runtime.getURL(page);
  const open = await Promise.resolve(ext.tabs.query({})).catch(() => []);
  const tab = (open || []).find((x) => x && typeof x.url === "string" && x.url.split(/[?#]/)[0] === url);
  if (tab) {
    await Promise.resolve(ext.tabs.update(tab.id, { active: true })).catch(() => {});
    if (ext.windows && ext.windows.update && tab.windowId !== undefined) await Promise.resolve(ext.windows.update(tab.windowId, { focused: true })).catch(() => {});
    return;
  }
  await Promise.resolve(ext.tabs.create({ url })).catch(() => {});
}
const openStatusPage = () => openOwnPage(STATUS_PAGE);
