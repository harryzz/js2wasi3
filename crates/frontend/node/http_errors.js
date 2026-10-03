// node 24's `internal/errors` for the `_http_*` modules (T71): `node:internal/errors` (T60's) plus the HTTP
// codes and `ConnResetException`, here so programs without `http` keep their bytes.
"use strict";

const errors = require("node:internal/errors");
const { format } = require("node:internal/util/inspect");

const codes = { ...errors.codes };

// node's `format` for these messages' `%s`: primitives as `String` gives them.
function fill(message, args) {
  let i = 0;
  return message.replace(/%s/g, () => {
    const a = args[i++];
    return typeof a === "object" && a !== null ? format("%s", a) : String(a);
  });
}

function E(code, message, Base) {
  class NodeError extends Base {
    constructor(...args) {
      super(typeof message === "function" ? message(...args) : fill(message, args));
    }

    toString() {
      return `${this.name} [${code}]: ${this.message}`;
    }
  }
  Object.defineProperties(NodeError.prototype, {
    name: { value: Base.name, writable: true, enumerable: false, configurable: true },
    toString: {
      value() {
        return `${this.name} [${code}]: ${this.message}`;
      },
      writable: true,
      enumerable: false,
      configurable: true,
    },
  });
  NodeError.prototype.code = code;
  // (node's variant without the stack frames: the same here.)
  NodeError.HideStackFramesError = NodeError;
  codes[code] = NodeError;
}

E("ERR_HTTP_BODY_NOT_ALLOWED", "Adding content for this request method or response status is not allowed.", Error);
E("ERR_HTTP_CONTENT_LENGTH_MISMATCH",
  "Response body's content-length of %s byte(s) does not match the content-length of %s byte(s) set in header", Error);
E("ERR_HTTP_HEADERS_SENT", "Cannot %s headers after they are sent to the client", Error);
E("ERR_HTTP_INVALID_HEADER_VALUE", 'Invalid value "%s" for header "%s"', TypeError);
E("ERR_HTTP_INVALID_STATUS_CODE", "Invalid status code: %s", RangeError);
E("ERR_HTTP_REQUEST_TIMEOUT", "Request timeout", Error);
E("ERR_HTTP_SOCKET_ASSIGNED", "ServerResponse has an already assigned socket", Error);
E("ERR_HTTP_SOCKET_ENCODING", "Changing the socket encoding is not allowed per RFC7230 Section 3.", Error);
E("ERR_HTTP_TRAILER_INVALID", "Trailers are invalid with this transfer encoding", Error);
E("ERR_INVALID_CHAR", (name, field = undefined) => {
  let msg = `Invalid character in ${name}`;
  if (field !== undefined) msg += ` ["${field}"]`;
  return msg;
}, TypeError);
E("ERR_INVALID_HTTP_TOKEN", '%s must be a valid HTTP token ["%s"]', TypeError);
E("ERR_PROXY_INVALID_CONFIG", "%s", Error);

class ConnResetException extends Error {
  constructor(msg) {
    super(msg);
    this.code = "ECONNRESET";
  }

  get ["constructor"]() {
    return Error;
  }
}

module.exports = { ...errors, codes, ConnResetException };
