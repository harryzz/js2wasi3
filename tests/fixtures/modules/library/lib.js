// A library whose exports come from several modules (T41): a dependency's function is re-exported,
// and its top-level code runs before the first call (library init).
import { scale, table } from './util.js';
import config from './config.json' with { type: 'json' };
export { double } from './util.js';

/** @param {number} x */
export function area(x) {
  return scale * x * x;
}

export function square(i) {
  return table[i];
}

/** @returns {string} */
export function describe() {
  return `${config.name} ${config.version} from ${import.meta.url.slice(import.meta.url.lastIndexOf('/') + 1)}`;
}
