"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.counter = void 0;
exports.default = greet;
exports.counter = { count: 0 };
function greet(name) {
    exports.counter.count++;
    return `hello, ${name}`;
}
