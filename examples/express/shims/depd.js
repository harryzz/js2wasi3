'use strict'
// js2wasi3 shim for `depd` (Express's deprecation-warning helper).
//
// The real depd reads V8's call-site API (`Error.prepareStackTrace`, `CallSite` objects) and builds functions with
// `new Function` from a computed string, at load time. js2wasi3 has neither (it compiles ahead of time and keeps no
// structured stack frames), so this keeps depd's interface and prints no deprecation warnings.
module.exports = depd

function depd (namespace) {
  if (!namespace) throw new TypeError('argument namespace is required')
  function deprecate (message) {}
  deprecate._file = undefined
  deprecate._ignored = true
  deprecate._namespace = namespace
  deprecate._traced = false
  deprecate._warned = Object.create(null)
  deprecate.function = function (fn, message) { return fn }
  deprecate.property = function (obj, prop, message) {}
  return deprecate
}
