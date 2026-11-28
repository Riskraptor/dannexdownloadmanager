// The native host: messages to the app, and why one failed. A part of the background: background.js
// loads the parts in order, into one scope.

// ---- the native host ---------------------------------------------------------------------

// How long an answer may take: the host starts Dannex Download Manager when it isn't running
// and waits for it (crates/nmhost START_WAIT); the browser's download waits meanwhile.
const HOST_WAIT = 30000;

function withTimeout(promise, ms) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error("no answer")), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}

// Why the browser couldn't reach the host, as the help page's sections name it.
function hostProblem(error) {
  const m = String((error && error.message) || error);
  if (/not found|no such native application/i.test(m)) return "not-installed";
  if (/forbidden/i.test(m)) return "forbidden";
  if (/no answer/i.test(m)) return "not-running";
  return "host-failed";
}

// A message that gives the app something to download (not a question, a setting or a notice).
const handsOver = (m) => Boolean((m.url && !m.panel) || (Array.isArray(m.links) && m.links.length) || m.videoPick);

// Every message says which browser and extension version it comes from (the app keeps each
// browser's status, and the rules depend on the browser). Never throws: failures come back as
// `{ok: false, why}`.
async function native(message, ms = HOST_WAIT) {
  // A download for another DDM (a server, #157): sent there, not to this computer's. Only a file
  // by its address goes (the remote control adds files by address): the video button's question
  // about a page, and a video (looked up with this browser's sign-ins), stay this computer's DDM's;
  // sent there, the page itself was added as a file, every time a page with a video opened.
  if (typeof message.url === "string" && message.url && handsOver(message) && !message.video && !message.videoPick) {
    const target = await remoteTarget();
    if (target) return remoteSend(target, message);
  }
  const app = await APP_NAME;
  const brands = app ? [...BRANDS, app] : BRANDS;
  const sent = { ...message, userAgent: navigator.userAgent, brands, v: VERSION };
  try {
    const reply = (await withTimeout(ext.runtime.sendNativeMessage(HOST, sent), ms)) || {};
    if (reply.version) {
      await noteStatus(older(reply.version, VERSION) ? "update-app" : null, reply.version);
      // The answer to `rules` also says whether the app looks videos up for the button.
      await noteLookup(reply.videoLookup === true);
    } else if (reply.ok || reply.why === "options") {
      // Reached, which says nothing of the app's version: a nudge to update it stays until the
      // rules say otherwise.
      if ((await readStatus()).why !== "update-app") await noteStatus(null);
    } else if (reply.why && reply.why !== "refused") await noteStatus(reply.why);
    return reply;
  } catch (e) {
    const why = hostProblem(e);
    // With another DDM chosen, this computer's needn't be there.
    if (!(await remoteTarget())) await noteStatus(why);
    return { ok: false, why, error: String((e && e.message) || e) };
  }
}
