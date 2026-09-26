
// Map and Set (T25 slice 2). Conventions: crates/lower/src/builtins.rs.
// The storage is native (`crates/backend/src/mapset.rs`, the `__intrinsic_map_*` operations): an
// ordered hash table keyed by SameValueZero whose iterators see entries added while iterating.
// `$$Map`/`$$Set` are `new Map(iterable)`/`new Set(iterable)` (only `new` calls them; `Map(…)`
// throws in the lowering). `Map.prototype[Symbol.iterator]` is `entries`, `Set.prototype.keys` and
// `[Symbol.iterator]` are `values`, `size` is a getter (installed by the backend). Iterator modes
// (`__intrinsic_map_iter_kind`): 0–2 a Map's keys/values/entries, 3–5 a Set's.
// Not implemented: subclassing, `WeakMap`/`WeakSet`. The constructors add entries directly, not through a user-replaced
// `set`/`add`.
// Uses array.js's `$$describe`, `$$typeDescribe`, `$$call3`; string.js's `$$String`.

// Throw unless `v` is a Map (kind 0) or a Set (kind 1).
function $$mapThis(v, kind, name) {
  if (__intrinsic_map_kind(v) !== kind) {
    throw new TypeError("Method " + name + " called on incompatible receiver " + $$describe(v));
  }
}

// The constructors' check (for-of would name the variable instead).
function $$mapIterable(v) {
  if (typeof v[Symbol.iterator] !== "function") {
    throw new TypeError(
      $$typeDescribe(v) + " is not iterable (cannot read property Symbol(Symbol.iterator))",
    );
  }
}

function $$Map(iterable) {
  const m = __intrinsic_map_new_map();
  if (iterable === undefined || iterable === null) {
    return m;
  }
  $$mapIterable(iterable);
  for (const e of iterable) {
    if ((typeof e !== "object" && typeof e !== "function") || e === null) {
      throw new TypeError("Iterator value " + $$String(e) + " is not an entry object");
    }
    __intrinsic_map_set(m, e[0], e[1]);
  }
  return m;
}

function $$Set(iterable) {
  const s = __intrinsic_map_new_set();
  if (iterable === undefined || iterable === null) {
    return s;
  }
  $$mapIterable(iterable);
  for (const v of iterable) {
    __intrinsic_map_set(s, v, undefined);
  }
  return s;
}

// `Map.groupBy(items, callbackfn)` (node 21+): groups in first-seen order, keys by SameValueZero.
function $$Map_groupBy(items, callbackfn) {
  if (items === undefined || items === null) {
    throw new TypeError("Map.groupBy called on null or undefined");
  }
  if (typeof callbackfn !== "function") {
    throw new TypeError($$describe(callbackfn) + " is not a function");
  }
  $$mapIterable(items);
  const m = __intrinsic_map_new_map();
  let k = 0;
  for (const v of items) {
    const key = callbackfn(v, k);
    let group = __intrinsic_map_get(m, key);
    if (group === undefined) {
      group = [];
      __intrinsic_map_set(m, key, group);
    }
    group.push(v);
    k++;
  }
  return m;
}

// ---- Map.prototype -------------------------------------------------------------------------

function $$Map_prototype_get(key) {
  $$mapThis(this, 0, "Map.prototype.get");
  return __intrinsic_map_get(this, key);
}

function $$Map_prototype_set(key, value) {
  $$mapThis(this, 0, "Map.prototype.set");
  __intrinsic_map_set(this, key, value);
  return this;
}

function $$Map_prototype_has(key) {
  $$mapThis(this, 0, "Map.prototype.has");
  return __intrinsic_map_has(this, key);
}

function $$Map_prototype_delete(key) {
  $$mapThis(this, 0, "Map.prototype.delete");
  return __intrinsic_map_delete(this, key);
}

function $$Map_prototype_clear() {
  $$mapThis(this, 0, "Map.prototype.clear");
  __intrinsic_map_clear(this);
}

function $$Map_prototype_size() {
  $$mapThis(this, 0, "get Map.prototype.size");
  return __intrinsic_map_size(this);
}

function $$Map_prototype_forEach(callbackfn, thisArg) {
  $$mapThis(this, 0, "Map.prototype.forEach");
  if (typeof callbackfn !== "function") {
    throw new TypeError($$typeDescribe(callbackfn) + " is not a function");
  }
  const it = __intrinsic_map_iter_entries(this);
  for (;;) {
    const i = __intrinsic_map_step(it);
    if (i < 0) {
      return;
    }
    $$call3(callbackfn, thisArg, __intrinsic_map_value(it, i), __intrinsic_map_key(it, i), this);
  }
}

function $$Map_prototype_keys() {
  $$mapThis(this, 0, "Map.prototype.keys");
  return __intrinsic_map_iter_keys(this);
}

function $$Map_prototype_values() {
  $$mapThis(this, 0, "Map.prototype.values");
  return __intrinsic_map_iter_values(this);
}

function $$Map_prototype_entries() {
  $$mapThis(this, 0, "Map.prototype.entries");
  return __intrinsic_map_iter_entries(this);
}

// ---- Set.prototype -------------------------------------------------------------------------

function $$Set_prototype_add(value) {
  $$mapThis(this, 1, "Set.prototype.add");
  __intrinsic_map_set(this, value, undefined);
  return this;
}

function $$Set_prototype_has(value) {
  $$mapThis(this, 1, "Set.prototype.has");
  return __intrinsic_map_has(this, value);
}

function $$Set_prototype_delete(value) {
  $$mapThis(this, 1, "Set.prototype.delete");
  return __intrinsic_map_delete(this, value);
}

function $$Set_prototype_clear() {
  $$mapThis(this, 1, "Set.prototype.clear");
  __intrinsic_map_clear(this);
}

function $$Set_prototype_size() {
  $$mapThis(this, 1, "get Set.prototype.size");
  return __intrinsic_map_size(this);
}

function $$Set_prototype_forEach(callbackfn, thisArg) {
  $$mapThis(this, 1, "Set.prototype.forEach");
  if (typeof callbackfn !== "function") {
    throw new TypeError($$typeDescribe(callbackfn) + " is not a function");
  }
  const it = __intrinsic_map_iter_values(this);
  for (;;) {
    const i = __intrinsic_map_step(it);
    if (i < 0) {
      return;
    }
    const k = __intrinsic_map_key(it, i);
    $$call3(callbackfn, thisArg, k, k, this);
  }
}

function $$Set_prototype_values() {
  $$mapThis(this, 1, "Set.prototype.values");
  return __intrinsic_map_iter_values(this);
}

function $$Set_prototype_entries() {
  $$mapThis(this, 1, "Set.prototype.entries");
  return __intrinsic_map_iter_entries(this);
}

// The ES2025 set methods over a set-like `other` (GetSetRecord: `size`, `has`, `keys`).
function $$setRecord(other, name) {
  if (typeof other !== "object" || other === null) {
    if (typeof other !== "function") {
      throw new TypeError("Set.prototype." + name + " argument must be an object");
    }
  }
  const rawSize = other.size;
  const numSize = +rawSize;
  if (numSize !== numSize) {
    throw new TypeError("The .size property is NaN");
  }
  const size = $$toInteger(numSize);
  if (size < 0) {
    throw new RangeError("'" + $$String(size) + "' is an invalid size");
  }
  const has = other.has;
  if (typeof has !== "function") {
    throw new TypeError('string "has" is not a function');
  }
  const keys = other.keys;
  if (typeof keys !== "function") {
    throw new TypeError('string "keys" is not a function');
  }
  return [size, has, keys];
}

// The iterator `other.keys()` returned, checked.
function $$setKeys(other, keys) {
  const it = keys.call(other);
  if (typeof it !== "object" || it === null) {
    throw new TypeError($$describe(it) + " is not an object");
  }
  return it;
}

function $$setStep(it) {
  const r = it.next();
  if (typeof r !== "object" || r === null) {
    throw new TypeError("Iterator result " + $$describe(r) + " is not an object");
  }
  return r;
}

function $$setCopy(s) {
  const out = __intrinsic_map_new_set();
  for (const v of s) {
    __intrinsic_map_set(out, v, undefined);
  }
  return out;
}

function $$setClose(it) {
  const ret = it.return;
  if (ret !== undefined && ret !== null) {
    ret.call(it);
  }
}

function $$Set_prototype_union(other) {
  $$mapThis(this, 1, "Set.prototype.union");
  const rec = $$setRecord(other, "union");
  const it = $$setKeys(other, rec[2]);
  const out = $$setCopy(this);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return out;
    }
    __intrinsic_map_set(out, r.value, undefined);
  }
  return out;
}

function $$Set_prototype_intersection(other) {
  $$mapThis(this, 1, "Set.prototype.intersection");
  const rec = $$setRecord(other, "intersection");
  const out = __intrinsic_map_new_set();
  if (__intrinsic_map_size(this) <= rec[0]) {
    for (const e of $$setCopy(this)) {
      if (rec[1].call(other, e)) {
        __intrinsic_map_set(out, e, undefined);
      }
    }
    return out;
  }
  const it = $$setKeys(other, rec[2]);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return out;
    }
    if (__intrinsic_map_has(this, r.value)) {
      __intrinsic_map_set(out, r.value, undefined);
    }
  }
  return out;
}

function $$Set_prototype_difference(other) {
  $$mapThis(this, 1, "Set.prototype.difference");
  const rec = $$setRecord(other, "difference");
  const out = $$setCopy(this);
  if (__intrinsic_map_size(this) <= rec[0]) {
    for (const e of $$setCopy(this)) {
      if (rec[1].call(other, e)) {
        __intrinsic_map_delete(out, e);
      }
    }
    return out;
  }
  const it = $$setKeys(other, rec[2]);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return out;
    }
    __intrinsic_map_delete(out, r.value);
  }
  return out;
}

function $$Set_prototype_symmetricDifference(other) {
  $$mapThis(this, 1, "Set.prototype.symmetricDifference");
  const rec = $$setRecord(other, "symmetricDifference");
  const it = $$setKeys(other, rec[2]);
  const out = $$setCopy(this);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return out;
    }
    if (__intrinsic_map_has(this, r.value)) {
      __intrinsic_map_delete(out, r.value);
    } else {
      __intrinsic_map_set(out, r.value, undefined);
    }
  }
  return out;
}

function $$Set_prototype_isSubsetOf(other) {
  $$mapThis(this, 1, "Set.prototype.isSubsetOf");
  const rec = $$setRecord(other, "isSubsetOf");
  if (__intrinsic_map_size(this) > rec[0]) {
    return false;
  }
  for (const e of $$setCopy(this)) {
    if (!rec[1].call(other, e)) {
      return false;
    }
  }
  return true;
}

function $$Set_prototype_isSupersetOf(other) {
  $$mapThis(this, 1, "Set.prototype.isSupersetOf");
  const rec = $$setRecord(other, "isSupersetOf");
  if (__intrinsic_map_size(this) < rec[0]) {
    return false;
  }
  const it = $$setKeys(other, rec[2]);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return true;
    }
    if (!__intrinsic_map_has(this, r.value)) {
      $$setClose(it);
      return false;
    }
  }
  return true;
}

function $$Set_prototype_isDisjointFrom(other) {
  $$mapThis(this, 1, "Set.prototype.isDisjointFrom");
  const rec = $$setRecord(other, "isDisjointFrom");
  if (__intrinsic_map_size(this) <= rec[0]) {
    for (const e of $$setCopy(this)) {
      if (rec[1].call(other, e)) {
        return false;
      }
    }
    return true;
  }
  const it = $$setKeys(other, rec[2]);
  for (;;) {
    const r = $$setStep(it);
    if (r.done) {
      return true;
    }
    if (__intrinsic_map_has(this, r.value)) {
      $$setClose(it);
      return false;
    }
  }
  return true;
}

// ---- iterators -----------------------------------------------------------------------------

function $$MapIterator_prototype_next() {
  const mode = __intrinsic_map_iter_kind(this);
  if (mode < 0 || mode > 2) {
    throw new TypeError(
      "Method Map Iterator.prototype.next called on incompatible receiver " + $$describe(this),
    );
  }
  return $$mapNext(this, mode);
}

function $$SetIterator_prototype_next() {
  const mode = __intrinsic_map_iter_kind(this);
  if (mode < 3) {
    throw new TypeError(
      "Method Set Iterator.prototype.next called on incompatible receiver " + $$describe(this),
    );
  }
  return $$mapNext(this, mode);
}

// The next iterator result of a map/set iterator in `mode`.
function $$mapNext(it, mode) {
  const i = __intrinsic_map_step(it);
  if (i < 0) {
    return { value: undefined, done: true };
  }
  const k = __intrinsic_map_key(it, i);
  if (mode === 1) {
    return { value: __intrinsic_map_value(it, i), done: false };
  }
  if (mode === 2) {
    return { value: [k, __intrinsic_map_value(it, i)], done: false };
  }
  if (mode === 5) {
    return { value: [k, k], done: false };
  }
  return { value: k, done: false };
}
