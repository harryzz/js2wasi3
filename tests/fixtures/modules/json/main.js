// JSON modules (T41 slice 3): `with { type: "json" }` gives the parsed file as the default export;
// the value is made once (every import sees the same object) and it's an ordinary mutable object.
import data from "./data.json" with { type: "json" };
import * as ns from "./data.json" with { type: "json" };
import { settings, bump } from "./config.js";

// (Printing an own `__proto__` key differs from node: F198.)
const { odd: own, ...rest } = data;
console.log(rest);
console.log(Object.keys(ns), ns.default === data, Object.keys(data));
console.log(Object.keys(own), Object.getPrototypeOf(own) === Object.prototype, own.__proto__ === Object.prototype);
console.log(Object.hasOwn(own, "__proto__"), Object.getOwnPropertyDescriptor(own, "__proto__").value);
console.log(settings.levels.length, bump(), data.count);
