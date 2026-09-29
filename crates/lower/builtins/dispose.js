
// ---- DisposableStack, AsyncDisposableStack (T45) ------------------------------------------------
// A stack is an object of class `CLASS_STACK` whose data `[kind, disposed, resources]` is under a
// private key: `__intrinsic_stack_new(data)` / `__intrinsic_async_stack_new(data)`,
// `__intrinsic_stack_data(v)` (`undefined` for anything else). A resource is `[value, method,
// async]`, as in the `using` rewrite (`frontend::using`), disposed last first.

function $$stackThis(v, kind, name) {
  const d = __intrinsic_stack_data(v);
  if (d === undefined || d[0] !== kind) {
    throw new TypeError("Method " + name + " called on incompatible receiver " + $$describe(v));
  }
  return d;
}

function $$stackPending(d, name) {
  if (d[1]) {
    throw new ReferenceError("Cannot call " + name + " on an already-disposed DisposableStack");
  }
}

function $$stackCreate(kind, newTarget) {
  const data = [kind, false, []];
  const o = kind === 0 ? __intrinsic_stack_new(data) : __intrinsic_async_stack_new(data);
  // OrdinaryCreateFromConstructor: through the constructor as a value (`super()`,
  // `Reflect.construct`) the construct entry passes the new target as `this`.
  if (newTarget !== undefined) {
    const proto = newTarget.prototype;
    if ($$isObject(proto) && proto !== Object.getPrototypeOf(o)) {
      __intrinsic_set_proto(o, proto);
    }
  }
  return o;
}

// AddDisposableResource with a value: `[value, method, async]` (GetDisposeMethod).
function $$stackResource(v, async) {
  if (typeof v !== "object" && typeof v !== "function") {
    throw new TypeError("An object is expected with `using` declarations");
  }
  let m = undefined;
  if (async) {
    m = v[Symbol.asyncDispose];
    if (m === undefined) {
      const d = v[Symbol.dispose];
      if (typeof d !== "function") {
        throw new TypeError("Symbol(Symbol.dispose) is not a function");
      }
      m = async function () {
        d.call(this);
      };
    }
  } else {
    m = v[Symbol.dispose];
  }
  if (typeof m !== "function") {
    throw new TypeError(async ? "Symbol(Symbol.asyncDispose) is not a function" : "Symbol(Symbol.dispose) is not a function");
  }
  return [v, m, async];
}

function $$stackCallable(f) {
  if (typeof f !== "function") {
    throw new TypeError($$describe(f) + " is not a function");
  }
}

// DisposeResources, synchronous: every method, last first; errors chained in SuppressedErrors.
function $$stackDispose(stack) {
  let has = false;
  let error = undefined;
  for (let i = stack.length - 1; i >= 0; i--) {
    const r = stack[i];
    try {
      r[1].call(r[0]);
    } catch (e) {
      error = has ? new SuppressedError(e, error, "An error was suppressed during disposal") : e;
      has = true;
    }
  }
  if (has) {
    throw error;
  }
}

function $$DisposableStack() {
  return $$stackCreate(0, this);
}

function $$DisposableStack_prototype_disposed() {
  return $$stackThis(this, 0, "get DisposableStack.prototype.disposed")[1];
}

function $$DisposableStack_prototype_use(value) {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.use");
  $$stackPending(d, "DisposableStack.prototype.use");
  if (value !== null && value !== undefined) {
    d[2].push($$stackResource(value, false));
  }
  return value;
}

function $$DisposableStack_prototype_adopt(value, onDispose) {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.adopt");
  $$stackPending(d, "DisposableStack.prototype.adopt");
  $$stackCallable(onDispose);
  d[2].push([undefined, () => onDispose(value), false]);
  return value;
}

function $$DisposableStack_prototype_defer(onDispose) {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.defer");
  $$stackPending(d, "DisposableStack.prototype.defer");
  $$stackCallable(onDispose);
  d[2].push([undefined, onDispose, false]);
}

function $$DisposableStack_prototype_move() {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.move");
  $$stackPending(d, "DisposableStack.prototype.move");
  const o = $$stackCreate(0, undefined);
  __intrinsic_stack_data(o)[2] = d[2];
  d[2] = [];
  d[1] = true;
  return o;
}

function $$DisposableStack_prototype_dispose() {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.dispose");
  if (d[1]) {
    return;
  }
  d[1] = true;
  const stack = d[2];
  d[2] = [];
  $$stackDispose(stack);
}

// `DisposableStack.prototype[Symbol.dispose]` (node: the same function as `dispose`).
function $$DisposableStack_prototype_$dispose() {
  const d = $$stackThis(this, 0, "DisposableStack.prototype.dispose");
  if (d[1]) {
    return;
  }
  d[1] = true;
  const stack = d[2];
  d[2] = [];
  $$stackDispose(stack);
}

function $$AsyncDisposableStack() {
  return $$stackCreate(1, this);
}

function $$AsyncDisposableStack_prototype_disposed() {
  return $$stackThis(this, 1, "get AsyncDisposableStack.prototype.disposed")[1];
}

function $$AsyncDisposableStack_prototype_use(value) {
  const d = $$stackThis(this, 1, "AsyncDisposableStack.prototype.use");
  $$stackPending(d, "AsyncDisposableStack.prototype.use");
  if (value === null || value === undefined) {
    d[2].push([value, undefined, true]);
  } else {
    d[2].push($$stackResource(value, true));
  }
  return value;
}

function $$AsyncDisposableStack_prototype_adopt(value, onDisposeAsync) {
  const d = $$stackThis(this, 1, "AsyncDisposableStack.prototype.adopt");
  $$stackPending(d, "AsyncDisposableStack.prototype.adopt");
  $$stackCallable(onDisposeAsync);
  d[2].push([undefined, () => onDisposeAsync(value), true]);
  return value;
}

function $$AsyncDisposableStack_prototype_defer(onDisposeAsync) {
  const d = $$stackThis(this, 1, "AsyncDisposableStack.prototype.defer");
  $$stackPending(d, "AsyncDisposableStack.prototype.defer");
  $$stackCallable(onDisposeAsync);
  d[2].push([undefined, onDisposeAsync, true]);
}

function $$AsyncDisposableStack_prototype_move() {
  const d = $$stackThis(this, 1, "AsyncDisposableStack.prototype.move");
  $$stackPending(d, "AsyncDisposableStack.prototype.move");
  const o = $$stackCreate(1, undefined);
  __intrinsic_stack_data(o)[2] = d[2];
  d[2] = [];
  d[1] = true;
  return o;
}

// DisposeResources with the spec's awaits (see `frontend::using`).
async function $$stackDisposeAsync(self) {
  const d = __intrinsic_stack_data(self);
  if (d === undefined || d[0] !== 1) {
    throw new TypeError("Receiver is not an AsyncDisposableStack");
  }
  if (d[1]) {
    return;
  }
  d[1] = true;
  const stack = d[2];
  d[2] = [];
  let needsAwait = false;
  let hasAwaited = false;
  let has = false;
  let error = undefined;
  for (let i = stack.length - 1; i >= 0; i--) {
    const r = stack[i];
    if (!r[2] && needsAwait && !hasAwaited) {
      await undefined;
      needsAwait = false;
    }
    if (r[1] !== undefined) {
      try {
        const x = r[1].call(r[0]);
        if (r[2]) {
          await x;
          hasAwaited = true;
        }
      } catch (e) {
        error = has ? new SuppressedError(e, error, "An error was suppressed during disposal") : e;
        has = true;
      }
    } else {
      needsAwait = true;
    }
  }
  if (needsAwait && !hasAwaited) {
    await undefined;
  }
  if (has) {
    throw error;
  }
}

function $$AsyncDisposableStack_prototype_disposeAsync() {
  return $$stackDisposeAsync(this);
}

function $$AsyncDisposableStack_prototype_$asyncDispose() {
  return $$stackDisposeAsync(this);
}
