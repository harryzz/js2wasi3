// A require the build can't work out needs --include (main.flags): the included modules are
// looked up by path when it runs.
const list = JSON.parse('["./plugins/alpha.js", "./plugins/beta.mjs"]');
for (const p of list) {
  const m = require(p);
  console.log(p, m.name ?? m.default);
}
