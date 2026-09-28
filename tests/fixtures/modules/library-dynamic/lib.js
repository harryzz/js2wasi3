// A library that loads modules with `import()` (T41 slice 4): the modules are in the build, and
// each loads on the first call that needs it.
/**
 * @param {string} name
 * @returns {Promise<number>}
 */
export async function run(name, x) {
  const plugin = await import(`./plugins/${name}.js`);
  return plugin.apply(x);
}
