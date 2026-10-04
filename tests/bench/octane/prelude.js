// T78: scripts/bench.sh puts this before base.js and replaces `Math.random` with `__random` in base.js and the
// benchmark. Octane's files are sloppy scripts; js2wasi3 compiles CommonJS as strict code (F232) and can't assign
// to a built-in's property (`Math.random = …`, base.js's deterministic generator; F331). These declarations make
// the files run in strict mode and change nothing for node, which runs the same file.
// The other names are assigned without a declaration (crypto.js, earley-boyer.js).
var alert, setupEngine, __random = Math.random;
var nValue, eValue, dValue, pValue, qValue, dmp1Value, dmq1Value, coeffValue, result;
