import { y, hello } from "./y.js";
console.log("x start", hello());
await 0;
console.log("x after await, y =", y);
export const x = "x";
