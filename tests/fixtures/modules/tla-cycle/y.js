import { x } from "./x.js";
export function hello() {
  return "hello from y";
}
console.log("y start");
await Promise.resolve();
console.log("y after await");
export const y = "y";
