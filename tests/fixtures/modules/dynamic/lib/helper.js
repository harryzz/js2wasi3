export let loads = 0;
loads++;
console.log("helper evaluated");
export function greet(who) {
  return `hello, ${who}`;
}
