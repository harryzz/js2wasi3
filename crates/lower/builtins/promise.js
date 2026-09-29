// The `Promise` statics (T50): the spec's steps for any constructor `this` (NewPromiseCapability,
// GetPromiseResolve, the iterator protocol with IteratorClose). Promises themselves, `then`, the
// resolving functions and the job queue are native (crates/backend/src/promise.rs). The backend
// installs these on the `Promise` value; `Promise.all(…)` calls them directly with `this` = `Promise`.
// Uses array.js's `$$typeDescribe`, `$$describe`, `$$append`.

function $$promiseIsObject(v) {
  return (typeof v === "object" && v !== null) || typeof v === "function";
}

// NewPromiseCapability(C): [promise, resolve, reject].
function $$promiseCapability(c) {
  if (!__intrinsic_is_constructor(c)) {
    // V8 prints a function's source; a built-in's is `function f() { [native code] }` (user
    // functions' source text isn't kept unless the code names `toString`, F104).
    const what =
      typeof c === "function" ? "function " + c.name + "() { [native code] }" : $$describe(c);
    throw new TypeError(what + " is not a constructor");
  }
  const cap = [undefined, undefined, undefined];
  // GetCapabilitiesExecutor: `length` 2, `name` "".
  const executor = (resolve, reject) => {
    if (cap[1] !== undefined || cap[2] !== undefined) {
      throw new TypeError("Promise executor has already been invoked with non-undefined arguments");
    }
    cap[1] = resolve;
    cap[2] = reject;
  };
  cap[0] = new c(executor);
  if (typeof cap[1] !== "function" || typeof cap[2] !== "function") {
    throw new TypeError("Promise resolve or reject function is not callable");
  }
  return cap;
}

// GetIterator(v, sync) into `r`: [iterator, next method, done, current value].
function $$promiseIterator(v, r) {
  const m = v === undefined || v === null ? undefined : v[Symbol.iterator];
  if (typeof m !== "function") {
    throw new TypeError(
      $$typeDescribe(v) + " is not iterable (cannot read property Symbol(Symbol.iterator))",
    );
  }
  const it = __intrinsic_call_list(m, v, []);
  if (!$$promiseIsObject(it)) {
    throw new TypeError("Result of the Symbol.iterator method is not an object");
  }
  r[0] = it;
  r[1] = it.next;
  r[2] = false;
}

// IteratorStepValue: `true` with the value in `r[3]`, `false` at the end. An exception marks the
// iterator done (it isn't closed then).
function $$promiseStep(r) {
  try {
    const next = r[1];
    if (typeof next !== "function") {
      throw new TypeError($$typeDescribe(next) + " is not a function");
    }
    const res = __intrinsic_call_list(next, r[0], []);
    if (!$$promiseIsObject(res)) {
      throw new TypeError("Iterator result " + $$String(res) + " is not an object");
    }
    if (res.done) {
      r[2] = true;
      return false;
    }
    r[3] = res.value;
    return true;
  } catch (e) {
    r[2] = true;
    throw e;
  }
}

// IteratorClose for a throw completion: what `return` does or throws doesn't matter.
function $$promiseClose(r) {
  try {
    const ret = r[0].return;
    if (typeof ret === "function") {
      __intrinsic_call_list(ret, r[0], []);
    }
  } catch (e) {}
}

// A copy of `values` (CreateArrayFromList).
function $$promiseList(values) {
  const a = [];
  for (let k = 0; k < values.length; k++) {
    $$append(a, values[k]);
  }
  return a;
}

// `Promise.any`'s rejection: an AggregateError of the reasons whose `stack` is its header, as
// node's (no frames: `console.log` prints it as `[AggregateError: …] { [errors]: … }`).
function $$promiseAnyError(errors) {
  const error = new AggregateError($$promiseList(errors), "All promises were rejected");
  Object.defineProperty(error, "stack", {
    value: "AggregateError: All promises were rejected",
    writable: true,
    enumerable: false,
    configurable: true,
  });
  return error;
}

// `Promise.all` (0), `allSettled` (1), `any` (2), `race` (3) on constructor `c`.
function $$promiseCombine(c, iterable, kind, name) {
  if (!$$promiseIsObject(c)) {
    throw new TypeError("Promise." + name + " called on non-object");
  }
  const cap = $$promiseCapability(c);
  // Made before the `try` and filled in it: wasmtime 49 loses GC references assigned to a local
  // inside a `try` and read in its `catch` when a collection runs in between (F224). Done until
  // GetIterator succeeds.
  const r = [undefined, undefined, true, undefined];
  try {
    // GetPromiseResolve(C).
    const resolve = c.resolve;
    if (typeof resolve !== "function") {
      throw new TypeError("resolve is not a function");
    }
    $$promiseIterator(iterable, r);
    if (kind === 3) {
      while ($$promiseStep(r)) {
        const next = __intrinsic_call_list(resolve, c, [r[3]]);
        next.then(cap[1], cap[2]);
      }
      return cap[0];
    }
    const values = [];
    // The remaining elements count.
    const remaining = [1];
    let index = 0;
    while ($$promiseStep(r)) {
      $$append(values, undefined);
      const next = __intrinsic_call_list(resolve, c, [r[3]]);
      const i = index;
      // Shared by an `allSettled` element's pair.
      const called = [false];
      const settle = (v) => {
        values[i] = v;
        remaining[0]--;
        if (remaining[0] === 0) {
          if (kind === 2) {
            __intrinsic_call_list(cap[2], undefined, [$$promiseAnyError(values)]);
          } else {
            __intrinsic_call_list(cap[1], undefined, [$$promiseList(values)]);
          }
        }
      };
      remaining[0]++;
      if (kind === 0) {
        next.then((x) => {
          if (called[0]) {
            return undefined;
          }
          called[0] = true;
          settle(x);
          return undefined;
        }, cap[2]);
      } else if (kind === 1) {
        next.then(
          (x) => {
            if (called[0]) {
              return undefined;
            }
            called[0] = true;
            settle({ status: "fulfilled", value: x });
            return undefined;
          },
          (x) => {
            if (called[0]) {
              return undefined;
            }
            called[0] = true;
            settle({ status: "rejected", reason: x });
            return undefined;
          },
        );
      } else {
        next.then(cap[1], (x) => {
          if (called[0]) {
            return undefined;
          }
          called[0] = true;
          settle(x);
          return undefined;
        });
      }
      index++;
    }
    remaining[0]--;
    if (remaining[0] === 0) {
      if (kind === 2) {
        __intrinsic_call_list(cap[2], undefined, [$$promiseAnyError(values)]);
      } else {
        __intrinsic_call_list(cap[1], undefined, [$$promiseList(values)]);
      }
    }
    return cap[0];
  } catch (e) {
    // IfAbruptRejectPromise, after IteratorClose when the iterator isn't done.
    if (!r[2]) {
      $$promiseClose(r);
    }
    __intrinsic_call_list(cap[2], undefined, [e]);
    return cap[0];
  }
}

function $$Promise_all(iterable) {
  return $$promiseCombine(this, iterable, 0, "all");
}

function $$Promise_allSettled(iterable) {
  return $$promiseCombine(this, iterable, 1, "allSettled");
}

function $$Promise_any(iterable) {
  return $$promiseCombine(this, iterable, 2, "any");
}

function $$Promise_race(iterable) {
  return $$promiseCombine(this, iterable, 3, "race");
}

// PromiseResolve(C, x).
function $$promiseResolve(c, x) {
  if (__intrinsic_is_promise(x) && x.constructor === c) {
    return x;
  }
  const cap = $$promiseCapability(c);
  __intrinsic_call_list(cap[1], undefined, [x]);
  return cap[0];
}

function $$Promise_resolve(x) {
  const c = this;
  if (!$$promiseIsObject(c)) {
    throw new TypeError("PromiseResolve called on non-object");
  }
  return $$promiseResolve(c, x);
}

function $$Promise_reject(r) {
  const c = this;
  if (!$$promiseIsObject(c)) {
    throw new TypeError("PromiseReject called on non-object");
  }
  const cap = $$promiseCapability(c);
  __intrinsic_call_list(cap[2], undefined, [r]);
  return cap[0];
}

function $$Promise_withResolvers() {
  const c = this;
  if (!$$promiseIsObject(c)) {
    throw new TypeError("Promise.withResolvers called on non-object");
  }
  const cap = $$promiseCapability(c);
  return { promise: cap[0], resolve: cap[1], reject: cap[2] };
}

function $$Promise_try(f, ...args) {
  const c = this;
  if (!$$promiseIsObject(c)) {
    throw new TypeError("Promise.try called on non-object");
  }
  const cap = $$promiseCapability(c);
  try {
    if (typeof f !== "function") {
      throw new TypeError($$typeDescribe(f) + " is not a function");
    }
    const v = __intrinsic_call_list(f, undefined, args);
    __intrinsic_call_list(cap[1], undefined, [v]);
  } catch (e) {
    __intrinsic_call_list(cap[2], undefined, [e]);
  }
  return cap[0];
}
