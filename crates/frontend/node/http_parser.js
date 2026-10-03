// node's `internalBinding('http_parser')` (T71): `HTTPParser`, `ConnectionsList`, `methods`, `allMethods`,
// with the interface of node 24's `src/node_http_parser.cc` over llhttp, written in JS from RFC 9112 and
// checked against node's binding (`tasks/T71-files/parser-diff.js`): the same callbacks with the same
// arguments, the same results of `execute`/`finish`, llhttp's error codes and reasons. Requests only
// (`HTTPParser.RESPONSE` is for node's HTTP client, which js2wasi3 doesn't have); strict parsing (the lenient
// flags are accepted and ignored).
"use strict";

const { Buffer } = require("buffer");

const methods = [
  "DELETE", "GET", "HEAD", "POST", "PUT", "CONNECT", "OPTIONS", "TRACE", "COPY", "LOCK", "MKCOL", "MOVE",
  "PROPFIND", "PROPPATCH", "SEARCH", "UNLOCK", "BIND", "REBIND", "UNBIND", "ACL", "REPORT", "MKACTIVITY",
  "CHECKOUT", "MERGE", "M-SEARCH", "NOTIFY", "SUBSCRIBE", "UNSUBSCRIBE", "PATCH", "PURGE", "MKCALENDAR", "LINK",
  "UNLINK", "SOURCE", "QUERY",
];
// llhttp's method numbers are the indices here.
const allMethods = [
  "DELETE", "GET", "HEAD", "POST", "PUT", "CONNECT", "OPTIONS", "TRACE", "COPY", "LOCK", "MKCOL", "MOVE",
  "PROPFIND", "PROPPATCH", "SEARCH", "UNLOCK", "BIND", "REBIND", "UNBIND", "ACL", "REPORT", "MKACTIVITY",
  "CHECKOUT", "MERGE", "M-SEARCH", "NOTIFY", "SUBSCRIBE", "UNSUBSCRIBE", "PATCH", "PURGE", "MKCALENDAR", "LINK",
  "UNLINK", "SOURCE", "PRI", "DESCRIBE", "ANNOUNCE", "SETUP", "PLAY", "PAUSE", "TEARDOWN", "GET_PARAMETER",
  "SET_PARAMETER", "REDIRECT", "RECORD", "FLUSH", "QUERY",
];
const M_CONNECT = 5;
const M_SOURCE = 33;
const M_PRI = 34;
// Methods valid with `HTTP/`: llhttp's HTTP methods (RTSP's need `RTSP/`).
const isHttpMethod = (m) => m <= M_SOURCE || m === 46;

const kOnMessageBegin = 0;
const kOnHeaders = 1;
const kOnHeadersComplete = 2;
const kOnBody = 3;
const kOnMessageComplete = 4;
const kOnExecute = 5;
const kOnTimeout = 6;
const kMaxHeaderFieldsCount = 32;
const kDefaultMaxHeaderSize = 16384;

// Byte classes.
const TOKEN = new Uint8Array(256);
for (const c of "!#$%&'*+-.^_`|~") TOKEN[c.charCodeAt(0)] = 1;
for (let c = 48; c <= 57; c++) TOKEN[c] = 1;
for (let c = 65; c <= 90; c++) TOKEN[c] = TOKEN[c + 32] = 1;
const isValueChar = (c) => c === 9 || (c >= 32 && c !== 127);
const isUrlChar = (c) => c > 32 && c < 127;
const isAlpha = (c) => (c | 32) >= 97 && (c | 32) <= 122;
const isDigit = (c) => c >= 48 && c <= 57;
const hexValue = (c) => (isDigit(c) ? c - 48 : (c | 32) >= 97 && (c | 32) <= 102 ? (c | 32) - 87 : -1);

// States.
const S_START = 0; // before a message: CR/LF skipped
const S_METHOD = 1;
const S_URL_START = 2; // spaces before the URL
const S_URL = 3;
const S_SCHEMA = 4; // `http` of an absolute URL
const S_SCHEMA_SLASH = 5; // `://`
const S_AFTER_URL = 6; // spaces before the protocol
const S_PROTOCOL = 7; // `HTTP/`, `RTSP/`, `ICE/`
const S_MAJOR = 8;
const S_DOT = 9;
const S_MINOR = 10;
const S_LINE_CR = 11; // CR after the version
const S_LINE_LF = 12;
const S_H09_LF = 13; // HTTP/0.9: LF after the URL
const S_HEADER_START = 14; // a header line, or the end of the headers
const S_FIELD = 15;
const S_VALUE_START = 16; // OWS before a value
const S_VALUE = 17;
const S_VALUE_LF = 18;
const S_HEADERS_LF = 19; // LF of the empty line
const S_BODY = 20; // Content-Length body
const S_CHUNK_SIZE_START = 21;
const S_CHUNK_SIZE = 22;
const S_CHUNK_EXT = 23;
const S_CHUNK_SIZE_LF = 24;
const S_CHUNK_DATA = 25;
const S_CHUNK_DATA_CR = 26;
const S_CHUNK_DATA_LF = 27;
const S_CLOSED = 28; // after a `Connection: close` message
const S_ERROR = 29;
const S_PAUSED_UPGRADE = 30;
const S_UPGRADE_AT_END = 31;

// Header kinds and flags (llhttp's).
const F_CONNECTION_KEEP_ALIVE = 1;
const F_CONNECTION_CLOSE = 2;
const F_CONNECTION_UPGRADE = 4;
const F_CHUNKED = 8;
const F_UPGRADE = 16;
const F_CONTENT_LENGTH = 32;
const F_SKIPBODY = 64;
const F_TRANSFER_ENCODING = 512;

function latin1(bytes, start, end) {
  let s = "";
  for (let i = start; i < end; i++) s += String.fromCharCode(bytes[i]);
  return s;
}

function trimEnd(s) {
  let n = s.length;
  while (n > 0 && (s.charCodeAt(n - 1) === 32 || s.charCodeAt(n - 1) === 9)) n--;
  return n === s.length ? s : s.slice(0, n);
}

const now = () => Date.now();

class ParseError {
  constructor(code, reason, pos) {
    this.code = code;
    this.reason = reason;
    this.pos = pos;
  }
}

class HTTPParser {
  constructor() {
    this._state = S_START;
    this._type = 1;
    this._maxHeaderSize = kDefaultMaxHeaderSize;
    this._list = null;
    this._paused = false;
    this._pendingPause = undefined;
    this._executing = false;
    this._current = null;
    this._error = null;
    this._lastMessageStart = 0;
    this._headersCompleted = false;
    this._haveFlushed = false;
    this._reset();
  }

  _reset() {
    this._method = -1;
    this._methodText = "";
    this._url = "";
    this._fields = [];
    this._values = [];
    this._field = "";
    this._value = "";
    this._headerSize = 0;
    this._major = 0;
    this._minor = 0;
    this._protocol = "";
    this._flags = 0;
    this._contentLength = 0;
    this._remaining = 0;
    this._chunkDigits = 0;
    this._upgrade = false;
    this._trailers = false;
    this._sawHeader = false;
    this._te = "";
  }

  initialize(type, resource, maxHeaderSize, lenientFlags, connectionsList) {
    if (type !== HTTPParser.REQUEST) {
      throw new Error("js2wasi3: HTTPParser.RESPONSE isn't supported (no HTTP client)");
    }
    this._type = type;
    this._maxHeaderSize = maxHeaderSize > 0 ? maxHeaderSize : kDefaultMaxHeaderSize;
    this._state = S_START;
    this._error = null;
    this._paused = false;
    this._pendingPause = undefined;
    // (node's binding never clears it again: after a flush, later messages' headers come through
    // `kOnHeaders` too.)
    this._haveFlushed = false;
    this._reset();
    this._lastMessageStart = now();
    this._headersCompleted = false;
    if (connectionsList instanceof ConnectionsList) {
      this._list = connectionsList;
      connectionsList._push(this);
      connectionsList._pushActive(this);
    } else {
      this._list = null;
    }
  }

  // The parsed byte count, or an `Error` like node's binding.
  execute(data) {
    this._current = data;
    this._executing = true;
    let r;
    try {
      r = this._execute(data);
    } finally {
      this._executing = false;
      this._current = null;
    }
    if (this._pendingPause !== undefined) {
      this._paused = this._pendingPause;
      this._pendingPause = undefined;
    }
    return r;
  }

  finish() {
    if (this._error !== null) return this._makeError(this._error, 0);
    const s = this._state;
    if (s === S_START || s === S_CLOSED || s === S_PAUSED_UPGRADE || s === S_UPGRADE_AT_END) return undefined;
    return this._makeError(new ParseError("HPE_INVALID_EOF_STATE", "Invalid EOF state", 0), 0);
  }

  _makeError(e, pos) {
    const err = new Error("Parse Error");
    err.bytesParsed = pos;
    err.code = e.code;
    err.reason = e.reason;
    return err;
  }

  _execute(data) {
    if (this._error !== null) return this._makeError(this._error, 0);
    if (this._paused) return this._makeError(new ParseError("HPE_PAUSED", "Paused", 0), 0);
    if (this._state === S_PAUSED_UPGRADE) {
      // llhttp_resume_after_upgrade: the next bytes are a new message.
      this._state = S_START;
    } else if (this._state === S_UPGRADE_AT_END) {
      // An upgrade that ended its buffer pauses llhttp on the next byte, after the next message began.
      this._state = S_PAUSED_UPGRADE;
      if (data.length > 0) this._messageBegin();
      return 0;
    }
    const n = data.length;
    let i = 0;
    try {
      i = this._run(data, 0, n);
    } catch (e) {
      if (!(e instanceof ParseError)) throw e;
      this._error = e;
      this._state = S_ERROR;
      return this._makeError(e, e.pos);
    }
    return i;
  }

  _fail(code, reason, pos) {
    throw new ParseError(code, reason, pos);
  }

  _track(len, pos) {
    this._headerSize += len;
    if (this._headerSize >= this._maxHeaderSize) this._fail("HPE_HEADER_OVERFLOW", "Header overflow", pos);
  }

  _messageBegin() {
    this._reset();
    if (this._list !== null) {
      this._list._pop(this);
      this._list._popActive(this);
      this._lastMessageStart = now();
      this._list._push(this);
      this._list._pushActive(this);
    } else {
      this._lastMessageStart = now();
    }
    this._headersCompleted = false;
    const cb = this[kOnMessageBegin];
    if (typeof cb === "function") cb.call(this);
  }

  _headerPairs() {
    const out = [];
    for (let k = 0; k < this._values.length; k++) out.push(this._fields[k], this._values[k]);
    return out;
  }

  _flush() {
    const cb = this[kOnHeaders];
    const headers = this._headerPairs();
    const url = this._url;
    this._fields = [];
    this._values = [];
    this._url = "";
    this._haveFlushed = true;
    if (typeof cb === "function") cb.call(this, headers, url);
  }

  // A header name is complete.
  _fieldDone() {
    if (this._fields.length + 1 === kMaxHeaderFieldsCount) this._flush();
    this._fields.push(this._field);
  }

  // A header value is complete (`pos`: its CR).
  _valueDone(pos) {
    const value = trimEnd(this._value);
    this._values.push(value);
    if (!this._trailers) this._special(this._fields[this._fields.length - 1], value, pos);
  }

  // llhttp's checks of Content-Length, Transfer-Encoding, Connection and Upgrade.
  _special(name, value, pos) {
    const lower = name.toLowerCase();
    if (lower === "content-length") {
      if (this._flags & F_CONTENT_LENGTH) {
        this._fail("HPE_UNEXPECTED_CONTENT_LENGTH", "Duplicate Content-Length", pos);
      }
      if (this._flags & F_TRANSFER_ENCODING) {
        this._fail("HPE_INVALID_CONTENT_LENGTH", "Content-Length can't be present with Transfer-Encoding", pos);
      }
      if (value === "") this._fail("HPE_INVALID_CONTENT_LENGTH", "Empty Content-Length", pos);
      let v = 0;
      let digits = 0;
      for (let k = 0; k < value.length; k++) {
        const c = value.charCodeAt(k);
        if (!isDigit(c)) this._fail("HPE_INVALID_CONTENT_LENGTH", "Invalid character in Content-Length", pos);
        if (digits > 0 || c !== 48) digits++;
        v = v * 10 + (c - 48);
        if (digits > 20 || v > 18446744073709551615) {
          this._fail("HPE_INVALID_CONTENT_LENGTH", "Content-Length overflow", pos);
        }
      }
      this._flags |= F_CONTENT_LENGTH;
      this._contentLength = v;
    } else if (lower === "transfer-encoding") {
      if (this._flags & F_CONTENT_LENGTH) {
        this._fail("HPE_INVALID_TRANSFER_ENCODING", "Transfer-Encoding can't be present with Content-Length", pos);
      }
      if (value === "") this._fail("HPE_INVALID_TRANSFER_ENCODING", "Empty Transfer-Encoding", pos);
      const tokens = value.split(",");
      for (let k = 0; k < tokens.length; k++) {
        const t = tokens[k].trim().toLowerCase();
        if (this._flags & F_CHUNKED) {
          this._fail("HPE_INVALID_TRANSFER_ENCODING", "Invalid `Transfer-Encoding` header value", pos);
        }
        if (t === "chunked") this._flags |= F_CHUNKED;
      }
      this._flags |= F_TRANSFER_ENCODING;
    } else if (lower === "connection") {
      for (const token of value.split(",")) {
        const t = token.trim().toLowerCase();
        if (t === "close") this._flags |= F_CONNECTION_CLOSE;
        else if (t === "keep-alive") this._flags |= F_CONNECTION_KEEP_ALIVE;
        else if (t === "upgrade") this._flags |= F_CONNECTION_UPGRADE;
      }
    } else if (lower === "upgrade") {
      this._flags |= F_UPGRADE;
    }
  }

  _shouldKeepAlive() {
    if (this._major > 0 && this._minor > 0) return (this._flags & F_CONNECTION_CLOSE) === 0;
    return (this._flags & F_CONNECTION_KEEP_ALIVE) !== 0;
  }

  // The empty line ending the headers was read; `next` is the offset after it. Returns the state to go on in.
  _headersComplete(next) {
    if ((this._flags & F_UPGRADE) && (this._flags & F_CONNECTION_UPGRADE)) this._upgrade = true;
    else this._upgrade = this._method === M_CONNECT;
    this._headersCompleted = true;
    this._headerSize = 0;
    const cb = this[kOnHeadersComplete];
    let headers;
    let url;
    if (this._haveFlushed) {
      this._flush();
    } else {
      headers = this._headerPairs();
      url = this._url;
    }
    let r = 0;
    if (typeof cb === "function") {
      r = cb.call(this, this._major, this._minor, headers, this._method, url, undefined, undefined, this._upgrade,
        this._shouldKeepAlive());
      r = Number(r) | 0;
    }
    if (r === 2) {
      this._upgrade = true;
      this._flags |= F_SKIPBODY;
    } else if (r === 1) {
      this._flags |= F_SKIPBODY;
    }
    const hasBody = (this._flags & F_CHUNKED) !== 0 || this._contentLength > 0;
    if (this._upgrade && (this._method === M_CONNECT || (this._flags & F_SKIPBODY) || !hasBody)) {
      return this._messageComplete(next);
    }
    if (this._flags & F_SKIPBODY) return this._messageComplete(next);
    if (this._flags & F_CHUNKED) return S_CHUNK_SIZE_START;
    if (this._flags & F_TRANSFER_ENCODING) {
      this._fail("HPE_INVALID_TRANSFER_ENCODING", "Request has invalid `Transfer-Encoding`", next);
    }
    if (this._contentLength > 0) {
      this._remaining = this._contentLength;
      return S_BODY;
    }
    return this._messageComplete(next);
  }

  _messageComplete(next) {
    // Trailers.
    if (this._trailers && this._values.length > 0) this._flush();
    if (this._list !== null) {
      this._list._pop(this);
      this._list._popActive(this);
      this._lastMessageStart = 0;
      this._list._push(this);
    } else {
      this._lastMessageStart = 0;
    }
    const cb = this[kOnMessageComplete];
    if (typeof cb === "function") cb.call(this);
    if (this._upgrade) return S_PAUSED_UPGRADE;
    return this._shouldKeepAlive() ? S_START : S_CLOSED;
  }

  _body(data, start, end) {
    const cb = this[kOnBody];
    if (typeof cb === "function") cb.call(this, Buffer.from(data.subarray(start, end)));
  }

  _run(data, i, n) {
    // Where the current span started in this buffer (URL, name, value).
    let mark = -1;
    let state = this._state;
    while (i < n) {
      const c = data[i];
      switch (state) {
        case S_START:
          if (c === 13 || c === 10) {
            i++;
            break;
          }
          this._messageBegin();
          state = S_METHOD;
          this._methodStart = i;
          break;
        case S_METHOD: {
          if (c === 32 && this._methodText !== "") {
            const m = allMethods.indexOf(this._methodText);
            if (m < 0) this._fail("HPE_INVALID_METHOD", "Invalid method encountered", this._methodStart);
            this._method = m;
            state = S_URL_START;
            i++;
            break;
          }
          const text = this._methodText + String.fromCharCode(c);
          if (!allMethods.some((m) => m.startsWith(text))) {
            if (allMethods.includes(this._methodText)) {
              this._fail("HPE_INVALID_METHOD", "Expected space after method", i);
            }
            this._fail("HPE_INVALID_METHOD", "Invalid method encountered", this._methodStart);
          }
          this._methodText = text;
          i++;
          break;
        }
        case S_URL_START:
          if (c === 32) {
            i++;
            break;
          }
          if (this._method === M_CONNECT) {
            if (!isUrlChar(c)) this._fail("HPE_INVALID_URL", "Unexpected start char in url", i);
            state = S_URL;
          } else if (c === 47 || c === 42) {
            state = S_URL;
          } else if (isAlpha(c)) {
            state = S_SCHEMA;
          } else {
            this._fail("HPE_INVALID_URL", "Unexpected start char in url", i);
          }
          mark = i;
          i++;
          break;
        case S_SCHEMA:
          if (mark < 0) mark = i;
          if (isAlpha(c)) {
            i++;
          } else if (c === 58) {
            this._slashes = 0;
            state = S_SCHEMA_SLASH;
            i++;
          } else {
            this._fail("HPE_INVALID_URL", "Invalid characters in url", i);
          }
          break;
        case S_SCHEMA_SLASH:
          if (mark < 0) mark = i;
          if (c !== 47) this._fail("HPE_INVALID_URL", "Invalid characters in url", i);
          if (++this._slashes === 2) state = S_URL;
          i++;
          break;
        case S_URL: {
          if (mark < 0) mark = i;
          if (isUrlChar(c)) {
            i++;
            break;
          }
          this._url += latin1(data, mark, i);
          const len = i - mark;
          mark = -1;
          if (c === 32 || c === 13 || c === 10) this._track(len, i);
          if (c === 32) {
            state = S_AFTER_URL;
            i++;
          } else if (c === 13 || c === 10) {
            // HTTP/0.9: `GET /` and the line end.
            this._major = 0;
            this._minor = 9;
            state = c === 13 ? S_H09_LF : S_LINE_LF;
            if (c === 10) continue;
            i++;
          } else {
            const url = this._url;
            const where = url.includes("#") ? "fragment" : url.includes("?") ? "query" : "path";
            this._fail("HPE_INVALID_URL", `Invalid char in url ${where}`, i);
          }
          break;
        }
        case S_AFTER_URL:
          if (c === 32) {
            i++;
            break;
          }
          this._protocol = "";
          state = S_PROTOCOL;
          break;
        case S_PROTOCOL: {
          const p = this._protocol + String.fromCharCode(c);
          if (!["HTTP/", "RTSP/", "ICE/"].some((x) => x.startsWith(p))) {
            this._fail("HPE_INVALID_CONSTANT", "Expected HTTP/, RTSP/ or ICE/", i);
          }
          this._protocol = p;
          i++;
          if (p === "HTTP/" || p === "RTSP/" || p === "ICE/") {
            if (p === "ICE/" && this._method !== M_SOURCE) {
              this._fail("HPE_INVALID_CONSTANT", "Expected SOURCE method for ICE/x.x request", i);
            }
            if (p === "HTTP/" && !isHttpMethod(this._method) && this._method !== M_PRI) {
              this._fail("HPE_INVALID_CONSTANT", "Invalid method for HTTP/x.x request", i - 1);
            }
            state = S_MAJOR;
          }
          break;
        }
        case S_MAJOR:
          if (!isDigit(c)) this._fail("HPE_INVALID_VERSION", "Invalid major version", i);
          this._major = c - 48;
          state = S_DOT;
          i++;
          break;
        case S_DOT:
          if (c !== 46) this._fail("HPE_INVALID_VERSION", "Expected dot", i);
          state = S_MINOR;
          i++;
          break;
        case S_MINOR: {
          if (!isDigit(c)) this._fail("HPE_INVALID_VERSION", "Invalid minor version", i);
          this._minor = c - 48;
          const v = this._major * 10 + this._minor;
          if (v !== 9 && v !== 10 && v !== 11 && v !== 20) {
            this._fail("HPE_INVALID_VERSION", "Invalid HTTP version", i);
          }
          state = S_LINE_CR;
          i++;
          break;
        }
        case S_LINE_CR:
          if (c !== 13) this._fail("HPE_INVALID_VERSION", "Expected CRLF after version", i);
          state = S_LINE_LF;
          i++;
          break;
        case S_LINE_LF:
          if (c !== 10) this._fail("HPE_STRICT", "Expected CRLF after version", i);
          i++;
          if (this._method === M_PRI) {
            this._fail("HPE_PAUSED_H2_UPGRADE", "Pause on PRI/Upgrade", i);
          }
          if (this._major === 0 && this._minor === 9 && this._protocol === "") {
            // `GET /` (no version): no headers.
            state = this._headersComplete(i);
            break;
          }
          state = S_HEADER_START;
          break;
        case S_H09_LF:
          if (c !== 10) this._fail("HPE_STRICT", "Expected LF after CR", i);
          state = this._headersComplete(i + 1);
          i++;
          break;
        case S_HEADER_START:
          if (c === 13) {
            state = S_HEADERS_LF;
            i++;
          } else if (c === 32 || c === 9) {
            if (!this._sawHeader) this._fail("HPE_UNEXPECTED_SPACE", "Unexpected space after start line", i);
            this._fail("HPE_INVALID_HEADER_TOKEN", "Unexpected whitespace after header value", i);
          } else if (TOKEN[c]) {
            this._field = "";
            mark = i;
            state = S_FIELD;
          } else if (c === 10) {
            this._fail("HPE_INVALID_HEADER_TOKEN", "Invalid header field char", i);
          } else {
            this._fail("HPE_INVALID_HEADER_TOKEN", "Invalid header token", i);
          }
          break;
        case S_FIELD:
          if (mark < 0) mark = i;
          if (TOKEN[c]) {
            i++;
          } else if (c === 58) {
            this._field += latin1(data, mark, i);
            this._track(i - mark, i);
            mark = -1;
            if (this._field === "") this._fail("HPE_INVALID_HEADER_TOKEN", "Invalid header token", i);
            this._fieldDone();
            this._value = "";
            state = S_VALUE_START;
            i++;
          } else {
            this._fail("HPE_INVALID_HEADER_TOKEN", "Invalid header token", i);
          }
          break;
        case S_VALUE_START:
          if (c === 32 || c === 9) {
            i++;
            break;
          }
          state = S_VALUE;
          mark = i;
          break;
        case S_VALUE:
          if (mark < 0) mark = i;
          if (c === 13) {
            this._value += latin1(data, mark, i);
            this._track(i - mark, i + 1);
            mark = -1;
            this._valueDone(i);
            state = S_VALUE_LF;
            i++;
          } else if (c === 10) {
            this._fail("HPE_CR_EXPECTED", "Missing expected CR after header value", i);
          } else if (isValueChar(c)) {
            i++;
          } else {
            this._fail("HPE_INVALID_HEADER_TOKEN", "Invalid header value char", i);
          }
          break;
        case S_VALUE_LF:
          if (c !== 10) this._fail("HPE_LF_EXPECTED", "Missing expected LF after header value", i);
          this._sawHeader = true;
          state = S_HEADER_START;
          i++;
          break;
        case S_HEADERS_LF:
          if (c !== 10) this._fail("HPE_STRICT", "Expected LF after headers", i);
          i++;
          if (this._trailers) state = this._messageComplete(i);
          else state = this._headersComplete(i);
          break;
        case S_BODY: {
          const take = Math.min(this._remaining, n - i);
          this._body(data, i, i + take);
          i += take;
          this._remaining -= take;
          if (this._remaining === 0) state = this._messageComplete(i);
          break;
        }
        case S_CHUNK_SIZE_START:
          if (hexValue(c) < 0) this._fail("HPE_INVALID_CHUNK_SIZE", "Invalid character in chunk size", i);
          this._remaining = 0;
          this._chunkDigits = 0;
          state = S_CHUNK_SIZE;
          break;
        case S_CHUNK_SIZE: {
          const h = hexValue(c);
          if (h >= 0) {
            if (this._chunkDigits > 0 || h !== 0) this._chunkDigits++;
            if (this._chunkDigits > 16) this._fail("HPE_INVALID_CHUNK_SIZE", "Chunk size overflow", i);
            this._remaining = this._remaining * 16 + h;
            i++;
          } else if (c === 59) {
            state = S_CHUNK_EXT;
            i++;
          } else if (c === 13) {
            state = S_CHUNK_SIZE_LF;
            i++;
          } else if (c === 10) {
            this._fail("HPE_CR_EXPECTED", "Missing expected CR after chunk size", i);
          } else {
            this._fail("HPE_INVALID_CHUNK_SIZE", "Invalid character in chunk size", i);
          }
          break;
        }
        case S_CHUNK_EXT:
          if (c === 13) state = S_CHUNK_SIZE_LF;
          else if (c === 10) this._fail("HPE_CR_EXPECTED", "Missing expected CR after chunk extension name", i);
          else if (!isValueChar(c)) this._fail("HPE_STRICT", "Invalid character in chunk extensions", i);
          i++;
          break;
        case S_CHUNK_SIZE_LF:
          if (c !== 10) this._fail("HPE_STRICT", "Expected LF after chunk size", i);
          i++;
          if (this._remaining === 0) {
            this._trailers = true;
            this._fields = [];
            this._values = [];
            this._sawHeader = true;
            state = S_HEADER_START;
          } else {
            state = S_CHUNK_DATA;
          }
          break;
        case S_CHUNK_DATA: {
          const take = Math.min(this._remaining, n - i);
          this._body(data, i, i + take);
          i += take;
          this._remaining -= take;
          if (this._remaining === 0) state = S_CHUNK_DATA_CR;
          break;
        }
        case S_CHUNK_DATA_CR:
          if (c === 10) this._fail("HPE_CR_EXPECTED", "Missing expected CR after chunk data", i);
          if (c !== 13) this._fail("HPE_STRICT", "Expected LF after chunk data", i);
          state = S_CHUNK_DATA_LF;
          i++;
          break;
        case S_CHUNK_DATA_LF:
          if (c !== 10) this._fail("HPE_STRICT", "Expected LF after chunk data", i);
          state = S_CHUNK_SIZE_START;
          i++;
          break;
        case S_CLOSED:
          if (c === 13 || c === 10) {
            i++;
            break;
          }
          this._fail("HPE_CLOSED_CONNECTION", "Data after `Connection: close`", i);
          break;
        case S_PAUSED_UPGRADE:
          // The rest is another protocol's: stop here.
          this._state = state;
          return i;
        default:
          this._fail("HPE_INTERNAL", "Invalid parser state", i);
      }
      if (state === S_PAUSED_UPGRADE) {
        this._state = i === n ? S_UPGRADE_AT_END : state;
        return i;
      }
      this._state = state;
    }
    // The buffer ended inside a span: keep what it has.
    if (mark >= 0) {
      const part = latin1(data, mark, n);
      if (state === S_URL || state === S_SCHEMA || state === S_SCHEMA_SLASH) this._url += part;
      else if (state === S_FIELD) this._field += part;
      else if (state === S_VALUE) this._value += part;
      this._track(n - mark, n);
    }
    this._state = state;
    return n;
  }

  getCurrentBuffer() {
    return this._current === null ? Buffer.alloc(0) : Buffer.from(this._current);
  }

  pause() {
    if (this._executing) this._pendingPause = true;
    else this._paused = true;
  }

  resume() {
    if (this._executing) this._pendingPause = false;
    else this._paused = false;
  }

  remove() {
    if (this._list !== null) {
      this._list._pop(this);
      this._list._popActive(this);
    }
  }

  free() {}

  close() {}

  consume() {}

  unconsume() {}
}

HTTPParser.REQUEST = 1;
HTTPParser.RESPONSE = 2;
HTTPParser.kOnMessageBegin = kOnMessageBegin;
HTTPParser.kOnHeaders = kOnHeaders;
HTTPParser.kOnHeadersComplete = kOnHeadersComplete;
HTTPParser.kOnBody = kOnBody;
HTTPParser.kOnMessageComplete = kOnMessageComplete;
HTTPParser.kOnExecute = kOnExecute;
HTTPParser.kOnTimeout = kOnTimeout;
HTTPParser.kLenientNone = 0;
HTTPParser.kLenientHeaders = 1;
HTTPParser.kLenientChunkedLength = 2;
HTTPParser.kLenientKeepAlive = 4;
HTTPParser.kLenientTransferEncoding = 8;
HTTPParser.kLenientVersion = 16;
HTTPParser.kLenientDataAfterClose = 32;
HTTPParser.kLenientOptionalLFAfterCR = 64;
HTTPParser.kLenientOptionalCRLFAfterChunk = 128;
HTTPParser.kLenientOptionalCRBeforeLF = 256;
HTTPParser.kLenientSpacesAfterChunkSize = 512;
HTTPParser.kLenientHeaderValueRelaxed = 1024;
HTTPParser.kLenientAll = 1023;

// node's `ConnectionsList`: the server's parsers (all of them, and those with a request in progress).
class ConnectionsList {
  constructor() {
    this._all = new Set();
    this._active = new Set();
  }

  _push(p) {
    this._all.add(p);
  }

  _pop(p) {
    this._all.delete(p);
  }

  _pushActive(p) {
    this._active.add(p);
  }

  _popActive(p) {
    this._active.delete(p);
  }

  all() {
    return [...this._all];
  }

  idle() {
    return [...this._all].filter((p) => p._lastMessageStart === 0);
  }

  active() {
    return [...this._active];
  }

  expired(headersTimeout, requestTimeout) {
    const t = now();
    const headersDeadline = headersTimeout > 0 ? t - headersTimeout : 0;
    const requestDeadline = requestTimeout > 0 ? t - requestTimeout : 0;
    const out = [];
    if (headersDeadline === 0 && requestDeadline === 0) return out;
    for (const p of [...this._active]) {
      if ((!p._headersCompleted && headersDeadline > 0 && p._lastMessageStart < headersDeadline) ||
          (requestDeadline > 0 && p._lastMessageStart < requestDeadline)) {
        out.push(p);
        this._active.delete(p);
      }
    }
    return out;
  }
}

module.exports = { HTTPParser, ConnectionsList, methods, allMethods };
