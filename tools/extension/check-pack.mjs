// Checks what tools/extension/pack.ps1 built (target/extension) before it is uploaded:
//   node tools/extension/check-pack.mjs
// Each package carries the app's version and every file its manifest names; the unpacked copy's
// key gives the ID the native host serves (crates/ipc/src/extension.rs); the Chrome Web Store's
// copy has no key; Firefox's has its add-on ID; Safari's (a folder, tools/safari builds it into the
// app) asks for nothing Safari lacks.
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import assert from "node:assert/strict";

const root = new URL("../../", import.meta.url);
const out = new URL("target/extension/", root);
const cargo = readFileSync(new URL("Cargo.toml", root), "utf8");
const version = cargo.match(/^version\s*=\s*"([^"]+)"/m)[1];
const ipc = readFileSync(new URL("crates/ipc/src/extension.rs", root), "utf8");
const unpackedId = ipc.match(/UNPACKED_ID: &str = "([a-p]{32})"/)[1];
const firefoxId = ipc.match(/FIREFOX_ID: &str = "([^"]+)"/)[1];

// The background's parts as extension/background.js imports them (`importScripts`), in order;
// every script in extension/background/ is one of them.
const partsIn = (code) => [...code.matchAll(/"(background\/[\w.-]+\.js)"/g)].map((x) => x[1]);
const backgroundParts = partsIn(readFileSync(new URL("extension/background.js", root), "utf8"));
assert.ok(backgroundParts.length > 1, "background.js imports its parts");
assert.deepEqual(
  [...backgroundParts].sort(),
  readdirSync(new URL("extension/background/", root)).filter((f) => f.endsWith(".js")).map((f) => `background/${f}`).sort(),
  "every script in extension/background/ is loaded",
);

// A Chromium extension's ID: the first 32 hex digits of SHA-256 of its public key, 0-f as a-p.
const idOf = (key) =>
  [...createHash("sha256").update(Buffer.from(key, "base64")).digest("hex").slice(0, 32)]
    .map((c) => String.fromCharCode(97 + parseInt(c, 16)))
    .join("");

for (const kind of ["chromium", "chrome-store", "firefox", "safari"]) {
  const dir = new URL(`${kind}/`, out);
  const m = JSON.parse(readFileSync(new URL("manifest.json", dir), "utf8"));
  assert.equal(m.version, version, `${kind}: the app's version`);
  // The toolbar button opens its panel (popup.html): the switch, the tab's video and links, the pages.
  assert.equal(m.action && m.action.default_popup, "popup.html", `${kind}: the toolbar button opens its panel`);
  assert.ok(m.options_ui && m.options_ui.page, `${kind}: an options page`);
  const files = [
    m.background.service_worker,
    ...(m.background.scripts || []),
    m.options_ui.page,
    ...Object.values(m.icons),
    ...Object.values(m.action.default_icon),
    ...m.content_scripts.flatMap((c) => c.js),
    // Opened by background.js: the status page, the toolbar button's grey look.
    m.action.default_popup,
    "install.html",
    "icons/icon-off-16.png",
    "icons/icon-off-24.png",
    "icons/icon-off-32.png",
  ].filter(Boolean);
  for (const f of files) assert.ok(existsSync(new URL(f, dir)), `${kind}: ${f} is there`);
  // The background's parts, in this order everywhere: Chromium's service worker (background.js)
  // imports them, Firefox's and Safari's manifests list them.
  if (m.background.service_worker) {
    const worker = readFileSync(new URL(m.background.service_worker, dir), "utf8");
    assert.deepEqual(partsIn(worker), backgroundParts, `${kind}: the service worker imports the background's parts`);
    assert.equal(m.background.scripts, undefined, `${kind}: a service worker only`);
  } else {
    assert.deepEqual(m.background.scripts, backgroundParts, `${kind}: the manifest lists the background's parts`);
    assert.ok(!existsSync(new URL("background.js", dir)), `${kind}: no Chromium loader`);
  }
  for (const f of backgroundParts) assert.ok(existsSync(new URL(f, dir)), `${kind}: ${f} is there`);
  // The old Downloads and links window's stylesheet is gone: the panel uses the pages' own.
  assert.ok(!existsSync(new URL("popup.css", dir)), `${kind}: no popup.css`);
  // Nothing asks for a permission it no longer uses (the toolbar button counts no downloads).
  assert.ok(!(m.permissions || []).includes("alarms"), `${kind}: no alarms permission`);
  // Every script and stylesheet the pages load is in the package.
  for (const page of ["install.html", "options.html", "popup.html"]) {
    const html = readFileSync(new URL(page, dir), "utf8");
    for (const [, f] of html.matchAll(/(?:src|href)="([\w.-]+\.(?:js|css))"/g)) assert.ok(existsSync(new URL(f, dir)), `${kind}: ${page} loads ${f}, which is there`);
  }
  assert.ok(existsSync(new URL("_locales/en/messages.json", dir)), `${kind}: English texts`);
  if (kind === "chromium") assert.equal(idOf(m.key), unpackedId, "the key gives the ID the host serves");
  if (kind === "chrome-store") {
    assert.equal(m.key, undefined, "the store signs it with its own key");
    const video = m.content_scripts.find((c) => c.js.includes("video.js"));
    assert.ok(video && !(video.exclude_matches || []).length, "the video button runs everywhere, YouTube included");
  }
  if (kind === "firefox") assert.equal(m.browser_specific_settings.gecko.id, firefoxId);
  // media.js runs in the page's own world, at its start, in every frame (players out of the
  // document, MediaKeys); Safari's manifest leaves it out.
  const world = m.content_scripts.find((c) => c.js.includes("media.js"));
  if (kind === "safari") assert.ok(!world, "Safari: no script in the page's world");
  else assert.ok(world && world.world === "MAIN" && world.run_at === "document_start" && world.all_frames, `${kind}: media.js in the page's world, at its start, in every frame`);
  if (kind === "safari") {
    // page.js tells Safari by this; Safari has no downloads or proxy API to ask permission for.
    assert.ok(m.browser_specific_settings.safari, "page.js knows Safari by its manifest");
    for (const p of ["downloads", "proxy", "webRequestBlocking", "declarativeNetRequestWithHostAccess"]) {
      assert.ok(!(m.permissions || []).includes(p), `Safari: no ${p}`);
    }
    assert.ok(m.permissions.includes("nativeMessaging"), "Safari: the app extension is reached by native messaging");
  }
}
for (const zip of ["ddm-chromium.zip", "ddm-firefox.xpi", "ddm-chrome-store.zip"]) {
  assert.ok(existsSync(new URL(zip, out)), zip);
}
// Their former names are gone (pack.ps1 removes them): nothing ships an mdm-named file.
for (const old of ["mdm-chromium.zip", "mdm-firefox.xpi", "mdm-chrome-store.zip"]) {
  assert.ok(!existsSync(new URL(old, out)), `${old} is gone`);
}
console.log(`packages ok (version ${version}, unpacked ID ${unpackedId})`);
