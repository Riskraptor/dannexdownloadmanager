// Dannex Download Manager's browser extension: the background part (Chrome, Edge, Brave, Opera
// and Vivaldi run it as a service worker, Firefox and Safari as background scripts; see
// manifest.json, manifest.firefox.json and manifest.safari.json).
//
// Downloads of the file types Dannex Download Manager takes are handed to it: the browser's own
// download is cancelled, and the address, referrer, cookies, User-Agent, file name, size, type
// and (for a form) the posted body go to the app through the native messaging host
// (crates/nmhost). When it can't be reached the download stays with the browser, so nothing is
// lost, and the toolbar button says why: a click opens the extension's status page (install.html),
// which says what is missing and how to fix it (docs/browser-help.md). While DDM is reached, a
// click on the toolbar button turns taking downloads from this browser off and on (the button
// grey with an X and "OFF" while off); its right-click menu has "Don't take downloads from this
// site" and Help (the status page). Everything else lives in the app. The browser may stop this
// script whenever it is idle: what must last (the rules, the status, the pause) is kept in
// storage, not in memory.
//
// The background is the plain scripts below, in background/, run in this order in one global
// scope: they share their top-level names as one file would, and each runs its own top-level
// code (listeners, the start) when it loads, in the order they were written. Chromium's service
// worker imports them here; Firefox's and Safari's manifests list the same scripts in the same
// order (tools/extension/test.mjs and check-pack.mjs keep the three lists alike). A part's
// top-level code may use only what the parts before it (or itself) declare.
importScripts(
  "background/core.js",
  "background/native.js",
  "background/remote.js",
  "background/status.js",
  "background/copies.js",
  "background/rules.js",
  "background/requests.js",
  "background/tab-media.js",
  "background/cookies.js",
  "background/proxy.js",
  "background/capture.js",
  "background/opened.js",
  "background/firefox-answers.js",
  "background/menus.js",
  "background/start.js",
  "background/messages.js",
  "background/lookup.js",
  "background/played.js",
  "background/panel.js",
  "background/main.js",
);
