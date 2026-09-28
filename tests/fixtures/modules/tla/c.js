import { a } from "./a.js";
import { shared } from "./shared.js";
console.log("c sees", a, shared());
export const c = "c" + a;
