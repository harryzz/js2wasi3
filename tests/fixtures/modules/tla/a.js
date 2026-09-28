console.log("a start");
export let a = "a0";
await 0;
a = "a1";
console.log("a mid");
await new Promise((r) => setTimeout(r, 5));
a = "a2";
console.log("a end");
