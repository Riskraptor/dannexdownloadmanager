// What the extension's two pages share: install.html (the status page) and options.html. Their
// texts in the browser's language (_locales), laid out in its direction. Loaded first, before each
// page's own script (one global scope).
const ext = globalThis.browser ?? globalThis.chrome;
const HELP = "https://dannex.kavangi.co.ke/help/browser-help.html";
// Support DDM: the maintainer's GitHub Sponsors page, a plain link in each page's footer (a new
// tab, opened only by a click).
const SUPPORT = "https://github.com/sponsors/Riskraptor";
const t = (key, ...subs) => ext.i18n.getMessage(key, subs) || key;
const $ = (id) => document.getElementById(id);
const ask = (m) => Promise.resolve(ext.runtime.sendMessage(m)).catch(() => ({ ok: false }));
const lang = ext.i18n.getUILanguage();

// The page's texts: `data-i18n` (its text), `data-i18n-placeholder` (a box's hint, which names
// a box with no label of its own for screen readers), `data-i18n-title` (its tooltip).
function localize() {
  document.documentElement.lang = lang;
  document.documentElement.dir = t("@@bidi_dir") === "rtl" ? "rtl" : "ltr";
  for (const el of document.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const el of document.querySelectorAll("[data-i18n-placeholder]")) {
    el.placeholder = t(el.dataset.i18nPlaceholder);
    if (!el.hasAttribute("aria-labelledby")) el.setAttribute("aria-label", el.placeholder);
  }
  for (const el of document.querySelectorAll("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle);
}

// The page drawn as it opens: from now on its switches move when they change (not as it opens).
// Two frames on: the first draws the switches as they are, unmoved.
function ready() {
  const frame = globalThis.requestAnimationFrame || ((f) => f());
  frame(() => frame(() => document.body && document.body.classList.add("ready")));
}

// The browser's page for this extension (its details: site access, private windows, Remove).
// Chromium's (Edge, Brave, Opera and Vivaldi open chrome:// addresses as their own); Firefox
// lets no extension open about:addons, so its pages say where to go instead.
const EXTENSIONS_PAGE = /Firefox\//.test(navigator.userAgent || "") ? "" : `chrome://extensions/?id=${ext.runtime.id}`;
function openExtensionsPage() {
  if (EXTENSIONS_PAGE) Promise.resolve(ext.tabs.create({ url: EXTENSIONS_PAGE })).catch(() => {});
}

// This copy removed, the browser asking first (an extension may remove itself without any
// permission); where the browser can't, its extensions page. Cancelled: nothing.
async function removeThisCopy() {
  if (!ext.management || !ext.management.uninstallSelf) return openExtensionsPage();
  try {
    await ext.management.uninstallSelf({ showConfirmDialog: true });
  } catch {
    // Cancelled in the browser's own question.
  }
}
