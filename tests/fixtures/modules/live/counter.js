export let count = 0;
export var name = 'counter';
export function increment() {
  count++;
  name = `counter@${count}`;
}
export function reset() {
  count = 0;
}
