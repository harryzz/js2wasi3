// node's `tty` (T55): WASI programs have no terminal file descriptors js2wasi3 can query yet, so
// `isatty` is false (as node's under pipes: packages print without colours). `ReadStream` and
// `WriteStream` throw.
"use strict";

function isatty(fd) {
  return false;
}

function ReadStream(fd, options) {
  throw new Error("tty.ReadStream isn't implemented in js2wasi3 (planned: F243)");
}

function WriteStream(fd) {
  throw new Error("tty.WriteStream isn't implemented in js2wasi3 (planned: F243)");
}

module.exports = { isatty, ReadStream, WriteStream };
