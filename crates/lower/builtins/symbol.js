// Symbols (T25 slice 5). Conventions: crates/lower/src/builtins.rs.
// A symbol is a `$symbol`: a `$string` subtype holding its description, compared by identity, so
// `__intrinsic_sub(s, 0, n)` copies the description. `Symbol()` and `Symbol("")` are the same here:
// `description` is `undefined` for both (F53).
// Uses string.js's `$$String`, array.js's `$$describe`.

// The global registry of `Symbol.for`: key → symbol and symbol → key (maps, made on first use).
let $$symbolFor = null;
let $$symbolKey = null;

function $$thisSymbol(v, name) {
  v = __intrinsic_unwrap(v);
  if (typeof v !== "symbol") {
    throw new TypeError("Symbol.prototype." + name + " requires that 'this' be a Symbol");
  }
  return v;
}

// `Symbol` as a value (T49): its call entry (`new Symbol()` throws natively). Calls of `Symbol(…)` by
// name don't come here: they are `Intrinsic::NewSymbol`.
function $$Symbol(description) {
  return Symbol(description);
}

function $$Symbol_for(key) {
  const k = __intrinsic_to_string(key);
  if ($$symbolFor === null) {
    $$symbolFor = __intrinsic_map_new_map();
    $$symbolKey = __intrinsic_map_new_map();
  }
  let s = __intrinsic_map_get($$symbolFor, k);
  if (s === undefined) {
    s = Symbol(k);
    __intrinsic_map_set($$symbolFor, k, s);
    __intrinsic_map_set($$symbolKey, s, k);
  }
  return s;
}

function $$Symbol_keyFor(sym) {
  if (typeof sym !== "symbol") {
    throw new TypeError($$describe(sym) + " is not a symbol");
  }
  if ($$symbolKey === null) {
    return undefined;
  }
  return __intrinsic_map_get($$symbolKey, sym);
}

function $$Symbol_prototype_toString() {
  const s = $$thisSymbol(this, "toString");
  return "Symbol(" + __intrinsic_sub(s, 0, __intrinsic_str_len(s)) + ")";
}

function $$Symbol_prototype_valueOf() {
  return $$thisSymbol(this, "valueOf");
}

// A getter (installed by the backend).
function $$Symbol_prototype_description() {
  const s = $$thisSymbol(this, "description");
  const n = __intrinsic_str_len(s);
  if (n === 0) {
    return undefined;
  }
  return __intrinsic_sub(s, 0, n);
}

// `Symbol.prototype[Symbol.toPrimitive](hint)` (T49), linked by naming `toPrimitive`.
function $$Symbol_prototype_$toPrimitive(hint) {
  const v = __intrinsic_unwrap(this);
  if (typeof v !== "symbol") {
    throw new TypeError("Symbol.prototype [ @@toPrimitive ] requires that 'this' be a Symbol");
  }
  return v;
}
