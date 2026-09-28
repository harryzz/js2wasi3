// Top-level `await` in dependencies (T41 slice 3): modules that don't wait for an async one run
// while it's suspended; its importers run one job after its body settles.
import "./ticks.js";
import { a } from "./a.js";
import "./b.js";
import { c } from "./c.js";
import * as d from "./d.js";
import { e } from "./e.js";

console.log("main", a, c, d.d, e);
console.log(d);
