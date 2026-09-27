// `yield*` in an async generator (T29). Conventions: crates/lower/src/builtins.rs.
// The lowering (`Lowerer::async_yield_star`) calls these; every `Await` and `yield` of the
// delegation is in the generator itself. The delegation record is `[iterator, next]`.
// Uses array.js's `$$typeDescribe`, string.js's `$$String`.

function $$isObjectLike(v) {
  return (typeof v === "object" && v !== null) || typeof v === "function";
}

// GetIterator(v, async): the `Symbol.asyncIterator` iterator, else the sync one wrapped by
// CreateAsyncFromSyncIterator. TypeErrors with node's messages.
function $$asyncDelegateOpen(v) {
  const method = v[Symbol.asyncIterator];
  if (method === undefined || method === null) {
    const syncMethod = v[Symbol.iterator];
    if (typeof syncMethod !== "function") {
      throw new TypeError($$typeDescribe(syncMethod) + " is not a function");
    }
    const it = syncMethod.call(v);
    if (!$$isObjectLike(it)) {
      throw new TypeError("Result of the Symbol.iterator method is not an object");
    }
    const wrapper = $$asyncFromSync(it, it.next);
    return [wrapper, wrapper.next];
  }
  if (typeof method !== "function") {
    throw new TypeError($$typeDescribe(method) + " is not a function");
  }
  const it = method.call(v);
  if (!$$isObjectLike(it)) {
    throw new TypeError("Result of the Symbol.asyncIterator method is not an object");
  }
  return [it, it.next];
}

// Call(next, iterator, «received»): the result to await.
function $$asyncDelegateNext(rec, received) {
  const next = rec[1];
  if (typeof next !== "function") {
    throw new TypeError($$typeDescribe(next) + " is not a function");
  }
  return next.call(rec[0], received);
}

// GetMethod(iterator, name): `undefined` when there is none.
function $$asyncDelegateMethod(rec, name) {
  const m = rec[0][name];
  if (m === undefined || m === null) {
    return undefined;
  }
  if (typeof m !== "function") {
    throw new TypeError($$typeDescribe(m) + " is not a function");
  }
  return m;
}

function $$asyncDelegateCall(rec, m, received) {
  return m.call(rec[0], received);
}

// `return()` without arguments (AsyncIteratorClose).
function $$asyncDelegateClose(rec, m) {
  return m.call(rec[0]);
}

// An awaited inner result must be an object.
function $$asyncDelegateResult(r) {
  if (!$$isObjectLike(r)) {
    throw new TypeError("Iterator result " + $$String(r) + " is not an object");
  }
}

function $$asyncDelegateNoThrow() {
  throw new TypeError("The iterator does not provide a 'throw' method.");
}

// IteratorClose(sync, completion) for a throw completion: `return()` runs, its result and errors
// are ignored.
function $$syncCloseQuietly(it) {
  try {
    const ret = it.return;
    if (ret !== undefined && ret !== null) {
      ret.call(it);
    }
  } catch (e) {
    // The original error wins.
  }
}

// CreateAsyncFromSyncIterator (ES2025 27.1.6): each result's value is awaited; a rejected value
// closes the sync iterator unless it's done (`closeOnRejection`).
function $$asyncFromSync(it, next) {
  const continuation = (result, closeOnRejection) => {
    const done = !!result.done;
    const value = result.value;
    let wrapper;
    try {
      wrapper = Promise.resolve(value);
    } catch (e) {
      if (!done && closeOnRejection) {
        $$syncCloseQuietly(it);
      }
      throw e;
    }
    const onRejected =
      done || !closeOnRejection
        ? undefined
        : (e) => {
            $$syncCloseQuietly(it);
            throw e;
          };
    return wrapper.then((v) => ({ value: v, done: done }), onRejected);
  };
  // Abrupt completions reject the returned promise (IfAbruptRejectPromise).
  const settle = (f) => {
    try {
      return f();
    } catch (e) {
      return Promise.reject(e);
    }
  };
  return {
    next(value) {
      return settle(() => {
        const result = next.call(it, value);
        if (!$$isObjectLike(result)) {
          throw new TypeError("Iterator result " + $$String(result) + " is not an object");
        }
        return continuation(result, true);
      });
    },
    return(value) {
      return settle(() => {
        const ret = it.return;
        if (ret === undefined || ret === null) {
          return Promise.resolve({ value: value, done: true });
        }
        const result = ret.call(it, value);
        if (!$$isObjectLike(result)) {
          throw new TypeError("Iterator result " + $$String(result) + " is not an object");
        }
        return continuation(result, false);
      });
    },
    throw(value) {
      return settle(() => {
        const thr = it.throw;
        if (thr === undefined || thr === null) {
          // Close the sync iterator (errors from `return()` win), then a TypeError.
          const ret = it.return;
          if (ret !== undefined && ret !== null) {
            const r = ret.call(it);
            if (!$$isObjectLike(r)) {
              throw new TypeError("Iterator result " + $$String(r) + " is not an object");
            }
          }
          throw new TypeError("The iterator does not provide a 'throw' method");
        }
        const result = thr.call(it, value);
        if (!$$isObjectLike(result)) {
          throw new TypeError("Iterator result " + $$String(result) + " is not an object");
        }
        return continuation(result, true);
      });
    },
  };
}
