console.log('math runs');
export function add(a, b) {
  return a + b;
}
const PI = 3.14;
export { PI };
export class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
  sum() {
    return add(this.x, this.y);
  }
}
