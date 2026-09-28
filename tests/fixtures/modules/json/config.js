import data from "./data.json" with { type: "json" };
export { default as settings } from "./settings.json" with { type: "json" };
export function bump() {
  return ++data.count;
}
