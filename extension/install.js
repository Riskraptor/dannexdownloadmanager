// The status page (install.html): what the toolbar button opens while it can't hand downloads to
// Dannex Download Manager, what opens by itself once when the extension is installed where DDM
// isn't, and the toolbar button's right-click menu → Help. In plain words: DDM is installed (and
// the on/off switch), or what is wrong and the one thing to do, for each of the background's
// reasons (`why`, the help page's sections: docs/browser-help.md), with DDM's download for this
// computer's system when DDM is missing. It asks again every two seconds (every five once DDM
// answers, never while hidden), so it turns into "Dannex Download Manager is installed" by itself.
// Below, what else is worth knowing: an update, the browser's site access, private windows,
// Firefox's temporary add-on, the Chrome Web Store's copy, the last download the browser kept.
// pages.js has what the extension's pages share.

localize();
$("help").href = HELP;
$("support").href = SUPPORT;

// Where DDM is downloaded: the website's download page, at this system's files (it always has
// the latest release's, and picks the file as its own buttons do); GitHub's latest release beside
// it. Links only: the extension itself fetches nothing.
const DOWNLOADS = "https://dannex.kavangi.co.ke/download.html";
const RELEASES = "https://github.com/Riskraptor/dannexdownloadmanager/releases/latest";
const FAST = 2000;
const SLOW = 5000;

// This computer's system, as the website's download page reads it (website/main.js detect):
// "windows", "macos", "linux", or "other" (a phone, a tablet, a Chromebook: DDM runs on none).
function systemOf(nav) {
  const uad = nav.userAgentData;
  const platform = String((uad && uad.platform) || nav.platform || "").toLowerCase();
  const ua = String(nav.userAgent || "").toLowerCase();
  if (/android|iphone|ipad|ipod|mobile|cros/.test(ua) || (uad && uad.mobile)) return "other";
  // iPadOS says it is a Mac; a touch screen gives it away.
  if (platform.startsWith("mac") && nav.maxTouchPoints > 1) return "other";
  if (platform.startsWith("win") || ua.includes("windows")) return "windows";
  if (platform.startsWith("mac") || ua.includes("mac os")) return "macos";
  if (/linux|x11|bsd/.test(`${platform} ${ua}`)) return "linux";
  return "other";
}
const SYSTEM = systemOf(navigator);
const DOWNLOAD = { windows: `${DOWNLOADS}#windows`, macos: `${DOWNLOADS}#macos`, linux: `${DOWNLOADS}#linux`, other: DOWNLOADS };

const PROBLEMS = ["not-installed", "not-running", "host-failed", "forbidden", "too-large"];
// What the page shows for the background's answer `s`: "duplicate" (this copy stands aside for
// the Chrome Web Store's), one of PROBLEMS (DDM not reached), "off-in-app" (DDM's Options take
// nothing from this browser), "remote" (downloads go to another DDM), or "installed".
function stateOf(s) {
  if (s.duplicate) return "duplicate";
  if (!s.connected) return PROBLEMS.includes(s.why) ? s.why : "host-failed";
  if (s.captureOff) return "off-in-app";
  if (s.remote) return "remote";
  return "installed";
}

// The three steps to get DDM (on a system DDM doesn't run on, the one that says so).
const STEPS = SYSTEM === "other" ? ["page_step_other"] : [`page_step_${SYSTEM}`, "page_step_wait", "page_step_restart"];
// The help page's section for each state.
const SECTION = { "not-installed": "not-installed", "not-running": "not-running", "host-failed": "host-failed", forbidden: "forbidden", "too-large": "too-large", "off-in-app": "options", duplicate: "two-copies" };
const key = (s) => String(s).replace(/-/g, "_");

// A list rebuilt only when what it holds changed (a button keeps its focus while the page asks
// again every few seconds).
const drawn = {};
function once(id, k, draw) {
  if (drawn[id] === k) return;
  drawn[id] = k;
  draw();
}

function button(label, run, primary = false) {
  const b = document.createElement("button");
  b.textContent = t(label);
  if (primary) b.className = "primary";
  b.addEventListener("click", () => run(b));
  return b;
}

function link(label, href) {
  const a = document.createElement("a");
  a.className = "link";
  a.textContent = t(label);
  a.href = href;
  a.target = "_blank";
  a.rel = "noopener";
  return a;
}

// What else is worth knowing while DDM is there: [{kind, text, buttons, link}].
function notesOf(s, state) {
  const out = [];
  if (!["installed", "remote", "off-in-app"].includes(state)) return out;
  if (s.why === "update-app") out.push({ kind: "warn", text: [t("status_update_app"), t("page_update_app_how")] });
  if (s.behind) {
    const said = document.createElement("p");
    said.className = "note";
    said.setAttribute("role", "status");
    if (s.copy === "store") {
      out.push({ kind: "info", text: [t("page_behind", s.app), t("page_behind_store")], buttons: [["page_check_updates", (b) => checkUpdates(b, said)]], after: said });
    } else if (s.copy === "unpacked") {
      out.push({ kind: "info", text: [t("page_behind", s.app), t("page_behind_unpacked")], buttons: [["page_reload", () => ext.runtime.reload()]] });
    } else {
      out.push({ kind: "info", text: [t("page_behind", s.app), t("page_behind_other")] });
    }
  }
  if (s.store) {
    out.push({ kind: "offer", text: [t("page_store")], buttons: [["page_open_store", () => ext.tabs.create({ url: s.store }), true], ["page_remove", () => removeThisCopy()]] });
  }
  if (s.temporary) out.push({ kind: "warn", text: [t("page_temporary")], link: ["page_learn_more", `${HELP}#firefox-temporary`] });
  if (s.allSites === false) {
    out.push({
      kind: "warn",
      text: [t("page_site_access")],
      // Asked for in the click itself (the browser allows it from a user's action only).
      buttons: [["page_allow_sites", () => Promise.resolve(ext.permissions.request({ origins: ["<all_urls>"] })).catch(() => false).then(() => tick(true))]],
      link: ["page_learn_more", `${HELP}#site-access`],
    });
  }
  if (s.incognito === false && s.browser !== "safari") {
    const chromium = s.browser === "chromium";
    out.push({
      kind: "info",
      text: [t("page_private"), t(chromium ? "page_private_chromium" : "page_private_firefox")],
      buttons: chromium && EXTENSIONS_PAGE ? [["page_extensions", () => openExtensionsPage()]] : [],
    });
  }
  // The last download DDM's Options (or the browser's site access) left to the browser, and why
  // (#208): the answer to "why did this one stay in the browser?". A reason this version can't
  // name says no more. Turned off in DDM's Options, the headline says it already.
  const d = s.declined;
  const why = d && d.why ? t(`declined_${key(d.why)}`) : "";
  if (state !== "off-in-app" && why && why !== `declined_${key(d.why)}`) {
    out.push({ kind: "info", text: [d.name ? t("popup_declined", d.name) : t("popup_declined_unnamed"), why], link: ["page_learn_more", `${HELP}#options`] });
  }
  return out;
}

function drawNotes(notes) {
  const ul = $("notes");
  ul.replaceChildren(
    ...notes.map((n) => {
      const li = document.createElement("li");
      li.className = n.kind;
      // Each sentence a line of its own: each is whole in its language, punctuation and all.
      for (const text of n.text.filter(Boolean)) {
        const p = document.createElement("p");
        p.textContent = text;
        li.append(p);
      }
      if (n.buttons && n.buttons.length) {
        const row = document.createElement("div");
        row.className = "actions";
        row.append(...n.buttons.map(([label, run, primary]) => button(label, run, primary)));
        li.append(row);
      }
      if (n.after) li.append(n.after);
      if (n.link) li.append(link(...n.link));
      return li;
    }),
  );
}

// The store's copy asks the store now; a newer one waiting is put in place at once.
async function checkUpdates(b, said) {
  b.disabled = true;
  try {
    const r = await ext.runtime.requestUpdateCheck();
    const status = Array.isArray(r) ? r[0] : r && r.status;
    if (status === "update_available") {
      ext.runtime.reload();
      return;
    }
  } catch {
    // Not offered here (a copy no store updates): as no update.
  }
  said.textContent = t("page_up_to_date");
  b.disabled = false;
}

let last = null;
function render(s) {
  last = s;
  const state = stateOf(s);
  const working = state === "installed" || state === "remote";
  const hero = $("hero");
  hero.classList.remove("wait", "ok", "off", "bad", "warn");
  hero.classList.add(working ? (s.paused ? "off" : "ok") : state === "duplicate" || state === "off-in-app" ? "warn" : "bad");
  const lead = $("lead");
  if (state === "installed") {
    $("headline").textContent = t("status_installed");
    lead.textContent = t(s.paused ? "page_off_lead" : "page_on_lead");
  } else if (state === "remote") {
    $("headline").textContent = t("page_remote_title");
    lead.textContent = s.paused ? t("page_off_lead") : t("page_remote_lead", s.remote);
  } else if (state === "duplicate") {
    $("headline").textContent = t("page_duplicate_title");
    lead.textContent = t("page_duplicate_lead");
  } else if (state === "off-in-app") {
    $("headline").textContent = t("status_off_in_app");
    lead.textContent = t(`declined_${key(s.captureOff)}`);
  } else {
    $("headline").textContent = t(`status_${key(state)}`);
    lead.textContent = t(`page_lead_${key(state)}`);
  }
  // The switch: the same as a click on the toolbar button.
  $("toggle-row").hidden = !working;
  $("toggle").checked = !s.paused;
  // DDM to download: missing, broken, or too old for this extension.
  $("get").hidden = !["not-installed", "host-failed", "forbidden"].includes(state);
  $("download").href = DOWNLOAD[SYSTEM];
  $("download-files").textContent = t(`page_files_${SYSTEM}`);
  $("all-downloads").href = RELEASES;
  const steps = state === "not-installed" ? STEPS : [];
  once("steps", steps.join(" "), () => {
    $("steps").hidden = !steps.length;
    $("steps").replaceChildren(
      ...steps.map((k) => {
        const li = document.createElement("li");
        li.textContent = t(k);
        return li;
      }),
    );
  });
  $("already").hidden = !(state === "not-installed" && SYSTEM !== "other");
  // One clear button for each state that needs one.
  const show = {
    retry: state === "not-running" || state === "too-large",
    "app-options": state === "off-in-app",
    remove: state === "duplicate",
    extensions: state === "duplicate" && Boolean(EXTENSIONS_PAGE),
  };
  for (const [id, on] of Object.entries(show)) $(id).hidden = !on;
  const notes = notesOf(s, state);
  once("notes", JSON.stringify(notes.map((n) => [n.kind, n.text, (n.buttons || []).map((b) => b[0])])), () => drawNotes(notes));
  $("help").href = SECTION[state] ? `${HELP}#${SECTION[state]}` : HELP;
  // Versions only here, small: the extension's.
  $("version").textContent = s.version ? `${t("name")} ${s.version}` : "";
  ready();
}

// Asked again: at once (`now`: a button), else on the clock; nothing while the page is hidden
// (it asks again as it shows).
let timer = null;
let busy = false;
async function tick(now = false) {
  clearTimeout(timer);
  if (busy) return;
  if (!now && document.visibilityState === "hidden") return;
  busy = true;
  if (now) $("hero").classList.add("wait");
  const s = await ask({ type: "probe" });
  busy = false;
  if (s && typeof s.version === "string") render(s);
  else $("hero").classList.remove("wait");
  timer = setTimeout(tick, last && last.connected && !last.duplicate ? SLOW : FAST);
}
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") tick();
});

$("retry").addEventListener("click", () => tick(true));
$("toggle").addEventListener("change", async (e) => {
  await ask({ type: "pause", paused: !e.target.checked });
  tick(true);
});
$("app-options").addEventListener("click", () => ask({ type: "open-options" }));
$("remove").addEventListener("click", () => removeThisCopy());
$("extensions").addEventListener("click", () => openExtensionsPage());

tick(true);
