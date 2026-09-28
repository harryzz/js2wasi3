export const sum = (a, b) => a + b;
export class Point {
  constructor(x) {
    this.x = x;
  }
}
// Not re-exported by `export *`.
export default 'tools default';
