#!/bin/bash
# T62 survey: copy zigbee2mqtt to $S/zp/patched and replace what blocks the build, to see what's next.
# Level 1 (always): missing built-ins → $S/t62/stubs/<name>.js; ajv's new Function, the two unknown import()s.
set -e
S=$(cd "$(dirname "$0")/.." && pwd)
rm -rf $S/zp/patched; cp -r $S/zp/package $S/zp/patched; cp $S/t62/main.js $S/zp/patched/; cd $S/zp/patched
mods="child_process crypto url dns net tls dgram http https http2 zlib perf_hooks"
files=$(grep -rlE --include='*.js' --include='*.mjs' --include='*.cjs' "(require\(|from |import\()[\"'](node:)?($(echo $mods | tr ' ' '|'))[\"']" . || true)
for f in $files; do
  for m in $mods; do
    sed -i -E "s#(require\(|from |import\()([\"'])(node:)?$m([\"'])#\1\2$S/t62/stubs/$m.js\4#g" "$f"
  done
done
echo "stubbed built-ins in $(echo $files | wc -w) files"
sed -i 's#const makeValidate = new Function(`${names_1.default.self}`, `${names_1.default.scope}`, sourceCode);#const makeValidate = () => { throw new Error("T62 stub: ajv new Function"); };#' node_modules/ajv/dist/compile/index.js
grep -q "T62 stub: ajv" node_modules/ajv/dist/compile/index.js
sed -i 's#const mod = await import(node_os_1.default.platform() === "win32" ? `file:///${tmpFile.replaceAll("\\\\", "/")}` : tmpFile);#const mod = (() => { throw new Error("T62 stub: externalJS import"); })();#' dist/extension/externalJS.js
grep -q "T62 stub: externalJS" dist/extension/externalJS.js
sed -i 's#const packageJSON = (await import(`${depend}/package.json`, { with: { type: "json" } })).default;#const packageJSON = { version: "T62 stub" };#' dist/util/utils.js
grep -q "T62 stub\"" dist/util/utils.js
# Level 2 (T62_LEVEL=2): the lowering errors of level 1, to get a .wasm.
[ "${T62_LEVEL:-1}" -ge 2 ] || exit 0
# Globals js2wasi3 lacks: bound from web.js at the top of each file that names one (same line: line numbers kept).
for f in $(grep -lE "global \`" $S/t62/errors-p1.txt >/dev/null; grep -E "global \`" $S/t62/errors-p1.txt | cut -d: -f1 | sort -u); do
  names=$(grep -E "^$f:.*global \`" $S/t62/errors-p1.txt | sed -E 's/.*global `([A-Za-z]+)`.*/\1/' | sort -u)
  decl=""
  for n in $names; do
    case $f in *.mjs) decl="${decl}import { $n } from \"$S/t62/stubs/web.js\"; ";; *) decl="${decl}var $n = require(\"$S/t62/stubs/web.js\").$n; ";; esac
  done
  sed -i "1s#^#$decl#" "$f"
done
# Object.getOwnPropertyDescriptors (T21 gap)
grep -rlE --include='*.js' --include='*.cjs' --include='*.mjs' "Object\.getOwnPropertyDescriptors" node_modules dist | xargs sed -i 's#Object\.getOwnPropertyDescriptors#((o) => { const r = {}; for (const k of Reflect.ownKeys(o)) r[k] = Object.getOwnPropertyDescriptor(o, k); return r; })#g'
# tsc's __decorate: typeof Reflect.decorate
grep -rl 'typeof Reflect.decorate === "function"' dist | xargs sed -i 's#typeof Reflect === "object" \&\& typeof Reflect.decorate === "function"#false#'
# winston: console._stdout/_stderr, a rest parameter with arguments
sed -i 's#console\._stderr#process.stderr#g; s#console\._stdout#process.stdout#g' node_modules/winston/lib/winston/transports/console.js
sed -i 's#^  log(level, msg, ...splat) {#  log(level, msg) { const splat = Array.prototype.slice.call(arguments, 2);#' node_modules/winston/lib/winston/logger.js
# delete of an optional chain
sed -i 's#    delete settings.devices?.\[device.ID\];#    if (settings.devices) delete settings.devices[device.ID];#' dist/util/settings.js
# babel regenerator: var read before its declaration in a loop
sed -i 's#^      function d(r, n) {$#      function d(r, n) { var o;#' node_modules/@babel/runtime/helpers/regenerator.js
grep -rl 'r = Reflect.decorate(decorators, target, key, desc)' dist | xargs sed -i 's#r = Reflect.decorate(decorators, target, key, desc)#r = undefined#'
sed -i "1s#^#import { Request } from \"$S/t62/stubs/web.js\"; #" node_modules/srvx/dist/body-limit.mjs
# Native addons (run time): no binding
sed -i "s#^exports.binding = (0, node_gyp_build_1.default)((0, path_1.join)(__dirname, '../'));#exports.binding = {}; // T62 stub: native addon#" node_modules/@serialport/bindings-cpp/dist/serialport-bindings.js
for f in node_modules/unix-dgram/lib/unix_dgram.js node_modules/winston-syslog/node_modules/unix-dgram/lib/unix_dgram.js; do
  sed -i "s#^var binding = require('bindings')('unix_dgram.node');#var binding = {}; // T62 stub: native addon#" $f
done
# ajv standalone (F191): the four settings validators generated at build time by t62/ajv-standalone.cjs
python3 - "$S" <<'PY'
import sys; S = sys.argv[1]; p = "dist/util/settings.js"; s = open(p).read()
start = s.index("const ajvSetting = new ajv_1.default"); end = s.index("exports.defaults = {")
new = "".join(f'const {v} = require("{S}/t62/ajv/{f}.js");\n' for v, f in [("ajvSetting", "setting"), ("ajvRestartRequired", "restart"), ("ajvRestartRequiredDeviceOptions", "device"), ("ajvRestartRequiredGroupOptions", "group")])
# keep the line count (diagnostics' line numbers)
new += "\n" * (s[start:end].count("\n") - new.count("\n"))
open(p, "w").write(s[:start] + new + s[end:])
PY
# srvx (web server; needs Fetch API globals at load): stubbed where zigbee2mqtt requires it
sed -i "s#require(\"srvx/node\")#require(\"$S/t62/stubs/srvx.js\")#; s#require(\"srvx/static\")#require(\"$S/t62/stubs/srvx.js\")#" dist/util/staticFileServer.js
# %TypedArray%.prototype[Symbol.toStringTag] getter missing (safe-stable-stringify, via logform/winston): same lines
python3 - <<'PY'
p = "node_modules/safe-stable-stringify/index.js"; s = open(p).read()
a = s.index("const typedArrayPrototypeGetSymbolToStringTag =\n"); b = s.index(").get\n", a) + len(").get\n")
new = "const typedArrayPrototypeGetSymbolToStringTag = function () { return ArrayBuffer.isView(this) && 'BYTES_PER_ELEMENT' in this ? this.constructor.name : undefined }\n"
open(p, "w").write(s[:a] + new + "\n" * (s[a:b].count("\n") - 1) + s[b:])
PY
# Error.prototype.stack traps (builtin sentinel): log the error's string instead where zigbee2mqtt logs a start failure
sed -i 's#^                logger_1.default.error(error.stack);#                logger_1.default.error(String(error));#' dist/controller.js
# device modules (built with --include): zemismart.js names TextEncoder/TextDecoder
sed -i "1s#^#var TextEncoder = require(\"$S/t62/stubs/web.js\").TextEncoder; var TextDecoder = require(\"$S/t62/stubs/web.js\").TextDecoder; #" node_modules/zigbee-herdsman-converters/dist/devices/zemismart.js
