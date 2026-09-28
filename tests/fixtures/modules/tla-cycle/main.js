// Top-level `await` inside an import cycle (T41 slice 3): the cycle's modules finish together,
// and a module outside it that imports one of them waits for the whole cycle.
import { x } from "./x.js";
import { z } from "./z.js";
import "./w.js";
console.log("main", x, z);
