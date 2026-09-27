// T20 (F11): an init that throws fails every export call with its error (a failed module
// evaluation), even exports that can't throw themselves.
const limits = JSON.parse("{bad");

export function one() {
  return 1;
}
export function limit() {
  return limits.max;
}
