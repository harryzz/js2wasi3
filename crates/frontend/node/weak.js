// node:internal/weak (T61): `WeakMap` and `WeakSet`, node's globals bound by the linker
// (`node::GLOBALS`). Each holds its entries in a `Map` (keys by identity): weakness isn't observable
// without `WeakRef`/`FinalizationRegistry`, which js2wasi3 lacks, so entries just live as long as
// the collection. Differences from node: called without `new`, the class's own TypeError.
'use strict';

const inspectCustom = Symbol.for('nodejs.util.inspect.custom');

// CanBeHeldWeakly: an object, or a symbol not in the registry.
function canBeHeldWeakly(v) {
  return (typeof v === 'object' && v !== null) || typeof v === 'function' ||
    (typeof v === 'symbol' && Symbol.keyFor(v) === undefined);
}

function incompatible(method, receiver) {
  return new TypeError(`Method ${method} called on incompatible receiver ${$$describe(receiver)}`);
}

// node's `<Name> { <items unknown> }`, `Sub [WeakMap] { … }` for a subclass.
function show(o, base) {
  const name = o.constructor && o.constructor.name;
  return `${name && name !== base ? `${name} [${base}]` : base} { <items unknown> }`;
}

function notIterable(v) {
  return new TypeError(`${$$typeDescribe(v)} is not iterable (cannot read property Symbol(Symbol.iterator))`);
}

class WeakMap {
  #m = new Map();

  // `...args`: `length` 0, as node's.
  constructor(...args) {
    const iterable = args[0];
    if (iterable === undefined || iterable === null) return;
    const adder = this.set;
    if (typeof adder !== 'function') throw new TypeError(`'${String(adder)}' returned for property 'set' of object '#<WeakMap>' is not a function`);
    if (typeof iterable[Symbol.iterator] !== 'function') throw notIterable(iterable);
    for (const e of iterable) {
      if ((typeof e !== 'object' && typeof e !== 'function') || e === null) {
        throw new TypeError(`Iterator value ${String(e)} is not an entry object`);
      }
      adder.call(this, e[0], e[1]);
    }
  }

  delete(key) {
    if (!isWeakMap(this)) throw incompatible('WeakMap.prototype.delete', this);
    return this.#m.delete(key);
  }

  get(key) {
    if (!isWeakMap(this)) throw incompatible('WeakMap.prototype.get', this);
    return this.#m.get(key);
  }

  set(key, value) {
    if (!isWeakMap(this)) throw incompatible('WeakMap.prototype.set', this);
    if (!canBeHeldWeakly(key)) throw new TypeError('Invalid value used as weak map key');
    this.#m.set(key, value);
    return this;
  }

  has(key) {
    if (!isWeakMap(this)) throw incompatible('WeakMap.prototype.has', this);
    return this.#m.has(key);
  }

  static is(v) {
    return (typeof v === 'object' && v !== null) && #m in v;
  }
}

const isWeakMap = WeakMap.is;

class WeakSet {
  #s = new Map();

  // `...args`: `length` 0, as node's.
  constructor(...args) {
    const iterable = args[0];
    if (iterable === undefined || iterable === null) return;
    const adder = this.add;
    if (typeof adder !== 'function') throw new TypeError(`'${String(adder)}' returned for property 'add' of object '#<WeakSet>' is not a function`);
    if (typeof iterable[Symbol.iterator] !== 'function') throw notIterable(iterable);
    for (const v of iterable) adder.call(this, v);
  }

  delete(value) {
    if (!isWeakSet(this)) throw incompatible('WeakSet.prototype.delete', this);
    return this.#s.delete(value);
  }

  has(value) {
    if (!isWeakSet(this)) throw incompatible('WeakSet.prototype.has', this);
    return this.#s.has(value);
  }

  add(value) {
    if (!isWeakSet(this)) throw incompatible('WeakSet.prototype.add', this);
    if (!canBeHeldWeakly(value)) throw new TypeError('Invalid value used in weak set');
    this.#s.set(value, true);
    return this;
  }

  static is(v) {
    return (typeof v === 'object' && v !== null) && #s in v;
  }
}

const isWeakSet = WeakSet.is;

// Helpers, not node's: off the classes.
delete WeakMap.is;
delete WeakSet.is;

for (const [C, name] of [[WeakMap, 'WeakMap'], [WeakSet, 'WeakSet']]) {
  Object.defineProperty(C.prototype, Symbol.toStringTag, { value: name, writable: false, enumerable: false, configurable: true });
  Object.defineProperty(C.prototype, inspectCustom, {
    value: function () {
      return show(this, name);
    },
    writable: true,
    enumerable: false,
    configurable: true,
  });
}

module.exports = { WeakMap, WeakSet, isWeakMap, isWeakSet };
