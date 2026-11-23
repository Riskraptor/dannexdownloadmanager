// The toolbar button's panel (popup.html): drawn at once from the page itself, then filled in by
// the background's answer (`popup`): how things stand, the on/off switch, the tab's video and
// links for DDM, and the status and options pages. pages.js has what the extension's pages share.

localize();

const CLASS = { on: "ok", off: "off", problem: "bad", duplicate: "warn", "off-in-app": "warn" };
let tabId = -1;

function close() {
  globalThis.window && window.close();
}

// Asked for the tab under the panel, then each click does its one thing and the panel closes
// (the background says what went wrong in a page of its own).
async function run(type) {
  const reply = await ask({ type, tab: tabId });
  if (reply && reply.why === "no-links") {
    $("said").textContent = t("popup_no_links");
    return;
  }
  close();
}

function draw(s) {
  if (!s || !s.ok) return;
  const hint = new Set([t("click_fix"), t("click_details")]);
  const lines = String(s.title || "").split("\n").filter((l) => l && !hint.has(l));
  $("hero").className = `hero ${CLASS[s.state] || "bad"}`;
  $("headline").textContent = lines[0] || t("name");
  $("lead").textContent = lines.slice(1).join(" ");
  $("toggle-row").hidden = !s.toggle;
  $("toggle").checked = !s.paused;
  $("fix").hidden = Boolean(s.toggle);
  $("page-actions").hidden = !s.web || !s.toggle;
  $("video").classList.toggle("primary", s.videos > 0);
}

async function refresh() {
  draw(await ask({ type: "popup", tab: tabId }));
}

$("toggle").addEventListener("change", async (e) => {
  await ask({ type: "pause", paused: !e.target.checked });
  await refresh();
});
$("fix").addEventListener("click", () => run("popup-status"));
$("status").addEventListener("click", () => run("popup-status"));
$("video").addEventListener("click", () => run("popup-video"));
$("all").addEventListener("click", () => run("popup-all"));
$("options").addEventListener("click", () => {
  Promise.resolve(ext.runtime.openOptionsPage()).catch(() => {});
  close();
});

(async () => {
  try {
    const [tab] = (await ext.tabs.query({ active: true, currentWindow: true })) || [];
    if (tab && Number.isInteger(tab.id)) tabId = tab.id;
  } catch {
    // No tab to read: the switch and the pages only.
  }
  await refresh();
  ready();
})();
