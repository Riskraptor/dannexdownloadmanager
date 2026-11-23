// The extension's options page (the browser's own Options item on the toolbar button's
// right-click menu, or the extension's details → Options): as small as it can be. The on/off
// switch (the same as a click on the toolbar button), a line saying what is wrong when DDM can't
// be reached (with the way to the status page, install.html), and under More the two things only
// the extension can do: download through the browser's proxy or VPN extension (#42), and send
// downloads to another DDM (#157). Everything else is in DDM's own Options. pages.js has what the
// extension's pages share.

localize();
$("help").href = HELP;
$("support").href = SUPPORT;

let storePage = "";
// The background's last answer, kept to draw the page again from a change in storage alone.
let last = null;

async function show() {
  render(await ask({ type: "state" }));
}

// How things stand, in one line, when there is something to say: {kind: "ok" | "warn" | "bad",
// text}, or null while DDM takes this browser's downloads as it should (the switch says it all).
function statusLine(s) {
  if (s.duplicate) return { kind: "warn", text: t("status_duplicate") };
  if (!s.connected) return { kind: "bad", text: t(`status_${String(s.why || "not_installed").replace(/-/g, "_")}`) };
  if (s.captureOff) return { kind: "warn", text: t("status_off_in_app") };
  if (s.remote) return { kind: "ok", text: t("status_remote", s.remote) };
  if (s.why === "update-app") return { kind: "warn", text: t("status_update_app") };
  return null;
}

function render(s) {
  last = s;
  $("take").checked = !s.paused;
  const line = statusLine(s);
  const box = $("status");
  box.hidden = !line;
  box.classList.remove("ok", "bad", "warn");
  if (line) {
    box.classList.add(line.kind);
    $("status-text").textContent = line.text;
    $("status-page").hidden = line.kind === "ok";
  }
  $("version").textContent = s.version ? `${t("name")} ${s.version}` : "";
  // The copy from DDM's folder, once the Chrome Web Store lists the extension: its page, once.
  storePage = s.store || "";
  $("store").hidden = !storePage;
  // Send downloads to: this computer, or another DDM (More opens by itself while one is chosen).
  $("to-here").checked = !s.remote && $("remote-form").hidden;
  $("to-remote").checked = Boolean(s.remote) || !$("remote-form").hidden;
  if (s.remote) $("more").open = true;
  ready();
}

// Turned off or on here, or by the toolbar button meanwhile: the page follows. The switch is drawn
// from the change itself, with no question to DDM; a new status (DDM reached or not), or another
// DDM paired or let go (rare, and what "reached" means changes), is asked of the background
// again.
$("take").addEventListener("change", async (e) => {
  await ask({ type: "pause", paused: !e.target.checked });
  show();
});
ext.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.status || changes.remote || (changes.paused && !last)) {
    show();
    return;
  }
  if (changes.paused) render({ ...last, paused: Boolean(changes.paused.newValue) });
});

// #42: reading the browser's proxy (a VPN or proxy extension's) is an opt-in, this switch, off as
// installed. Firefox: the optional "proxy" permission, asked for on it. Chromium can't make that
// permission optional, so the extension has it from the install, and the switch is a setting of
// its own (`proxyOn`, which background.js reads before each download). Safari has no proxy API.
const PROXY = { permissions: ["proxy"] };
const manifest = ext.runtime.getManifest();
const proxyNote = (k) => ($("proxy-note").textContent = t(k));
if (!(manifest.optional_permissions || []).includes("proxy") && (manifest.permissions || []).includes("proxy")) {
  const box = $("proxy");
  $("proxy-row").hidden = false;
  proxyNote("popup_proxy_title_chromium");
  Promise.resolve(ext.storage.local.get({ proxyOn: false }))
    .then((v) => (box.checked = Boolean(v.proxyOn)))
    .catch(() => {});
  box.addEventListener("change", () => Promise.resolve(ext.storage.local.set({ proxyOn: box.checked })).catch(() => {}));
}
if ((manifest.optional_permissions || []).includes("proxy")) {
  const box = $("proxy");
  $("proxy-row").hidden = false;
  proxyNote("popup_proxy_title");
  const sync = () =>
    Promise.resolve(ext.permissions.contains(PROXY))
      .catch(() => false)
      .then((on) => (box.checked = Boolean(on)));
  sync();
  box.addEventListener("change", () => {
    // Asked for in the click itself (the browser allows it from a user's action only): nothing
    // awaited before it. Refused, or taken back: the switch shows what the browser says.
    const asked = box.checked ? ext.permissions.request(PROXY) : ext.permissions.remove(PROXY);
    Promise.resolve(asked).catch(() => false).then(sync);
  });
}

// Send downloads to (#157): another DDM is paired here with its address and a code it shows.
$("to-remote").addEventListener("change", () => {
  $("remote-form").hidden = false;
  $("remote-url").focus();
});
$("to-here").addEventListener("change", async () => {
  $("remote-form").hidden = true;
  await ask({ type: "remote-off" });
  show();
});
$("remote-open").addEventListener("click", () => {
  try {
    const u = new URL($("remote-url").value.trim());
    if (u.protocol === "https:") ext.tabs.create({ url: u.origin + "/" });
  } catch {
    $("remote-said").textContent = t("remote_address");
    $("remote-said").hidden = false;
  }
});
$("remote-pair").addEventListener("click", async () => {
  const said = $("remote-said");
  said.hidden = true;
  const r = await ask({ type: "remote-pair", url: $("remote-url").value, code: $("remote-code").value });
  if (r.ok) {
    $("remote-form").hidden = true;
    $("remote-code").value = "";
    show();
    return;
  }
  said.textContent = t(`remote_${String(r.why || "failed").replace(/-/g, "_")}`);
  said.hidden = false;
});

$("store-open").addEventListener("click", () => {
  if (storePage) ext.tabs.create({ url: storePage });
});
$("store-remove").addEventListener("click", () => removeThisCopy());

show();
