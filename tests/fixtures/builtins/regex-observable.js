// T81: RegExp Symbol.match/replace/split/search/matchAll when user code can observe the spec steps
// (modified exec, flags, flag getters, lastIndex, Symbol methods, subclasses), and the fast paths
// for plain RegExps around them: same results and the same observable order as node.
const log = [];
const show = (label, f) => {
  log.length = 0;
  let r;
  try {
    r = JSON.stringify(f());
  } catch (e) {
    r = 'throws ' + e.name;
  }
  console.log(label, r, log.join(' '));
};
const P = RegExp.prototype;

// Plain RegExps (the fast paths).
const plain = () => {
  const s = 'a1b22c333 d😀e';
  const g = /\d+/g;
  show('match g', () => [s.match(g), g.lastIndex]);
  show('match', () => s.match(/(\d)(?<two>\d)/));
  show('match empty g', () => 'a😀b'.match(/(?:)/g));
  show('match empty gu', () => 'a😀b'.match(/(?:)/gu));
  show('match y', () => { const y = /\d/y; y.lastIndex = 1; return [s.match(y), y.lastIndex]; });
  show('match none', () => s.match(/x/g));
  show('replace fn', () => s.replace(/(\d)(\d)?/g, (m, a, b, i) => `<${m},${a},${b},${i}>`));
  show('replace named', () => s.replace(/(?<d>\d)/g, '[$<d>$1$&]'));
  show('replace once', () => s.replace(/\d+/, '#'));
  show('replace lastIndex', () => { const r = /\d/g; r.lastIndex = 5; s.replace(r, '-'); return r.lastIndex; });
  show('replace y', () => { const r = /\d/y; r.lastIndex = 1; return [s.replace(r, '$&$&'), r.lastIndex]; });
  show('split', () => s.split(/(\d)/));
  show('split limit', () => s.split(/\d/, 3));
  show('split empty', () => ''.split(/x/));
  show('split empty match', () => ''.split(/(?:)/));
  show('split u', () => 'a😀b'.split(/(?:)/u));
  show('split y', () => 'a1b2c'.split(/\d/y));
  show('search', () => { const r = /\d/g; r.lastIndex = 3; return [s.search(r), r.lastIndex]; });
  show('search none', () => s.search(/x/));
  show('matchAll', () => [...s.matchAll(/\d(\d)?/g)].map((m) => [m[0], m[1], m.index]));
  show('replaceAll', () => s.replaceAll(/\d/g, '_'));
  show('flags order', () => [...'aXbx'.matchAll(/x/gi)].map((m) => m.index));
};
plain();

// An own `exec` on the RegExp object.
const own = /\d/g;
own.exec = function (s) {
  log.push('exec@' + this.lastIndex);
  return RegExp.prototype.exec.call(this, s);
};
show('own exec match', () => 'a1b2'.match(own));
show('own exec replace', () => 'a1b2'.replace(own, (m) => m + m));
show('own exec search', () => 'a1b2'.search(own));
show('own exec test', () => own.test('x9'));

// A modified RegExp.prototype.exec (then restored with the same descriptor).
const exec = P.exec;
P.exec = function (s) {
  log.push('pexec:' + this.source + '@' + this.lastIndex);
  return exec.call(this, s);
};
show('proto exec match', () => 'a1b2'.match(/\d/g));
show('proto exec replace', () => 'a1b2'.replace(/\d/, 'x'));
show('proto exec split', () => 'a1b'.split(/\d/));
show('proto exec search', () => 'a1b2'.search(/\d/));
show('proto exec matchAll', () => [...'a1b2'.matchAll(/\d/g)].length);
P.exec = exec;

// A `flags` getter that logs.
const flagsDesc = Object.getOwnPropertyDescriptor(P, 'flags');
Object.defineProperty(P, 'flags', {
  get() { log.push('flags'); return flagsDesc.get.call(this); }, configurable: true,
});
show('flags match', () => 'a1b2'.match(/\d/g));
// (Not `replace`: node reads `global` and `unicode` there, the spec and js2wasi3 `flags`: F356.)
show('flags split', () => 'a1b'.split(/\d/));
show('flags matchAll', () => [...'a1b2'.matchAll(/\d/g)].length);
Object.defineProperty(P, 'flags', flagsDesc);

// A flag getter that lies (`global` false): `flags` reads it.
const globalDesc = Object.getOwnPropertyDescriptor(P, 'global');
Object.defineProperty(P, 'global', { get() { log.push('global'); return false; }, configurable: true });
show('global lies match', () => 'a1b2'.match(/\d/g));
Object.defineProperty(P, 'global', globalDesc);

// lastIndex: an object (ToLength calls valueOf), non-writable, a getter.
show('lastIndex object', () => {
  const r = /\d/y;
  r.lastIndex = { valueOf() { log.push('valueOf'); return 2; } };
  return ['a1b2'.replace(r, '_'), r.lastIndex];
});
show('lastIndex object search', () => {
  const r = /\d/;
  const o = { valueOf() { log.push('valueOf'); return 0; } };
  r.lastIndex = o;
  return ['a1'.search(r), r.lastIndex === o];
});
show('lastIndex read-only g', () => {
  const r = /\d/g;
  Object.defineProperty(r, 'lastIndex', { writable: false, value: 0 });
  return 'a1'.replace(r, '_');
});
show('lastIndex read-only match', () => {
  const r = /\d/g;
  Object.defineProperty(r, 'lastIndex', { writable: false, value: 0 });
  return 'a1'.match(r);
});
show('lastIndex read-only non-global', () => {
  const r = /\d/;
  Object.defineProperty(r, 'lastIndex', { writable: false, value: 0 });
  return ['a1'.replace(r, '_'), 'a1'.match(r)[0], 'a1'.search(r)];
});

// ToString(string) runs first and can change the RegExp.
show('toString first', () => {
  const r = /\d/g;
  const str = { toString() { log.push('toString'); r.lastIndex = 7; return 'a1b2'; } };
  return [r[Symbol.replace](str, '_'), r.lastIndex];
});
show('limit valueOf', () => {
  const r = /\d/;
  const lim = { valueOf() { log.push('limit'); r.lastIndex = 3; return 2; } };
  return ['a1b2c3'.split(r, lim), r.lastIndex];
});

// The replacement function runs after all matches and can change things.
show('fn changes exec', () => 'a1b2c3'.replace(/\d/g, (m) => {
  P.exec = function () { log.push('late exec'); return null; };
  return '<' + m + '>';
}));
P.exec = exec;
show('fn changes lastIndex', () => {
  const r = /\d/g;
  return ['a1b2'.replace(r, (m) => { r.lastIndex = 9; return m + m; }), r.lastIndex];
});

// Replaced Symbol methods. (Subclasses: F355, `class extends RegExp` instances get RegExp.prototype.)
const symReplace = P[Symbol.replace];
P[Symbol.replace] = function (s, v) { log.push('my replace'); return symReplace.call(this, s, v); };
show('Symbol.replace replaced', () => 'a1'.replace(/\d/, '_'));
P[Symbol.replace] = symReplace;

// A RegExp whose prototype isn't RegExp.prototype.
show('other proto', () => {
  const r = /\d/g;
  Object.setPrototypeOf(r, Object.create(P, { exec: { value(s) { log.push('sub exec'); return P.exec.call(this, s); } } }));
  return 'a1'.replace(r, '_');
});

// Plain again after the restores.
plain();
