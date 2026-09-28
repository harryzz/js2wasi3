export function helper() {
  return 'one helper';
}
export class Thing {}
export const value = 'one';
let x = 'one x';
export function getX() {
  const { length } = x;
  return `${x} (${length})`;
}
