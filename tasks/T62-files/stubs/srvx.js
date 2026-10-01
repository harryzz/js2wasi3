"use strict";
// T62: srvx needs the Fetch API globals at load time (globalThis.Request/Response/Headers/URL); the
// frontend and onboarding web servers are off in the survey's configuration.
function stub(n) { return function () { throw new Error("T62 stub: srvx." + n); }; }
exports.NodeRequest = stub("NodeRequest");
exports.sendNodeResponse = stub("sendNodeResponse");
exports.staticMiddleware = stub("staticMiddleware");
