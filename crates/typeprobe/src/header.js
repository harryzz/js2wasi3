var __N, __T, __R, __R2;
function __init(n) {
  __N = new Float64Array(n); __T = new Array(n); __R = new Array(n); __R2 = new Array(n);
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
function __tag(reps, v) {
  switch (typeof v) {
    case 'number': return (v | 0) === v && !(v === 0 && 1 / v < 0) ? 'int' : 'double';
    case 'object': if (v === null) return 'null';
    // fall through
    case 'function':
      for (var j = 0; j < reps.length; j++) if (%HaveSameMap(reps[j], v)) return 'obj' + j;
      if (reps.length < 8) { reps.push(v); return 'obj' + (reps.length - 1); }
      return 'obj+';
    default: return typeof v;
  }
}
function __count(i, k) {
  __N[i]++;
  var m = __T[i];
  if (m === undefined) m = __T[i] = new Map();
  m.set(k, (m.get(k) || 0) + 1);
}
function __r(i, v) {
  var r = __R[i]; if (r === undefined) r = __R[i] = [];
  __count(i, __tag(r, v));
  return v;
}
function __b(i, a, b) {
  var r = __R[i]; if (r === undefined) r = __R[i] = [];
  var r2 = __R2[i]; if (r2 === undefined) r2 = __R2[i] = [];
  __count(i, __tag(r, a) + ',' + __tag(r2, b));
}
