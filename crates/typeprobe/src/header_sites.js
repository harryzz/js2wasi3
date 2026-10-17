var __N, __T, __W = new WeakMap();
function __init(n) {
  __N = new Float64Array(n); __T = new Array(n);
  require('process').on('exit', function () {
    var o = [];
    for (var i = 0; i < n; i++) if (__N[i] > 0) {
      var t = {};
      __T[i].forEach(function (c, k) { t[k] = c; });
      o.push([i, __N[i], t]);
    }
    require('fs').writeFileSync(require('process').env.TYPEPROBE_OUT, JSON.stringify(o));
  });
}
// tag an object/function where it is made (a function's default prototype too)
function __a(i, v) {
  if ((typeof v === 'object' && v !== null) || typeof v === 'function') {
    if (!__W.has(v)) __W.set(v, 'site@' + i);
    if (typeof v === 'function') {
      var p = v.prototype;
      if (p && typeof p === 'object' && !__W.has(p)) __W.set(p, 'proto@' + i);
    }
  }
  return v;
}
function __ptag(p, d) {
  var t = __W.get(p);
  if (t !== undefined) return t;
  if (d > 4) return null;
  var q = Object.getPrototypeOf(p);
  if (q === null) return null;
  var u = __ptag(q, d + 1);
  return u === null ? null : 'inst@' + u;
}
function __tag(v) {
  switch (typeof v) {
    case 'number': return 'num';
    case 'object': if (v === null) return 'null';
    // fall through
    case 'function': {
      var t = __W.get(v);
      if (t !== undefined) return t;
      if (Array.isArray(v)) return 'array';
      if (typeof v === 'function') return 'other-fn';
      var q = Object.getPrototypeOf(v);
      var u = q === null ? null : __ptag(q, 0);
      return u === null ? 'other-obj' : 'inst@' + u;
    }
    default: return typeof v;
  }
}
function __count(i, k) {
  __N[i]++;
  var m = __T[i];
  if (m === undefined) m = __T[i] = new Map();
  m.set(k, (m.get(k) || 0) + 1);
}
function __r(i, v) { __count(i, __tag(v)); return v; }
function __b(i, a, b) { __count(i, __tag(a) + ',' + __tag(b)); }
