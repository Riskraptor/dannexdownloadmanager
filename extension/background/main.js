// Last: the messages' listener, and what runs as the script starts. A part of the background:
// background.js loads the parts in order, into one scope.

ext.runtime.onMessage.addListener((m, sender, sendResponse) => {
  // Only our own scripts (pages can't message an extension without being allowed to).
  if (!m || typeof m.type !== "string" || sender.id !== ext.runtime.id) return false;
  // The status and options pages are the extension's own (in a tab of their own too); content
  // scripts send from the web page they are in.
  const own = String(sender.url || "").startsWith(ext.runtime.getURL(""));
  const fromPage = Boolean(sender.tab) && !own;
  (fromPage ? pageMessage(m, sender) : ownPageMessage(m)).then(sendResponse, () => sendResponse({ ok: false }));
  return true; // answered later
});

showStatus();
// Whether the Chrome Web Store's copy is here too (this copy then stands aside); the toolbar
// button and the menus follow.
copyState().catch(() => {});
// Rules kept from before, else asked now (a first start, storage cleared).
openedReady = storedRules()
  .then((r) => (r ? syncOpenedRule() : refreshRules()))
  .catch(() => {});
