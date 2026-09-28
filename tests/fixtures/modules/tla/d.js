import "./b.js";
console.log("d start");
for await (const x of [1, 2]) console.log("d", x);
export const d = "d";
