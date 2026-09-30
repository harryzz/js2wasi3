// `util.inspect.custom` (T56): console.log and util.inspect call custom inspectors (depth, the
// inspect function), a non-string result is formatted, prototypes aren't.
import util from "node:util";
class P { constructor() { this.x = 1; } [util.inspect.custom](depth, opts, inspect) { return `P<${this.x}> d=${depth} ${inspect({ a: [1] })}`; } }
class Q { [Symbol.for("nodejs.util.inspect.custom")]() { return { replaced: true }; } }
console.log(new P(), [new P()], { q: new Q() }, util.inspect(new P()), P.prototype);
