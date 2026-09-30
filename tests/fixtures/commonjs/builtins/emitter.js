// A CommonJS module using node's built-ins the old way: util.inherits over EventEmitter.
const EventEmitter = require("events");
const util = require("node:util");

function Counter() {
  EventEmitter.call(this);
  this.n = 0;
}
util.inherits(Counter, EventEmitter);
Counter.prototype.tick = function () {
  this.n++;
  this.emit("tick", this.n);
};
module.exports = Counter;

// node's `Buffer` global in a CommonJS module (T56).
Counter.prototype.bytes = function () {
  return Buffer.from(String(this.n)).toString("hex");
};
