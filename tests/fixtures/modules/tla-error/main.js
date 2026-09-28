// A dependency's top-level `await` rejects (T41 slice 3): its importers never run, the
// modules that don't import it do, and the program ends with the error.
import "./ok.js";
import "./fails.js";
import "./after.js";
console.log("main never runs");
