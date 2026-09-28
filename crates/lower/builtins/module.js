// Module namespace objects (T41): `import * as ns`, `export * as ns from`. The module linker
// (`frontend::link`) makes one per module before any module runs, with the exported names in
// code-unit order and a getter reading each binding (so it's live, and throws in its TDZ). The
// object has a null prototype and class `CLASS_MODULE`, for which the runtime shows the getters
// as data properties (descriptors, printing) and rejects writes with node's messages.
function $$ModuleNamespace(keys, getters) {
  const ns = __intrinsic_module_new();
  for (let i = 0; i < keys.length; i++) {
    Object.defineProperty(ns, keys[i], { get: getters[i], enumerable: true });
  }
  Object.defineProperty(ns, Symbol.toStringTag, { value: "Module" });
  Object.preventExtensions(ns);
  return ns;
}
