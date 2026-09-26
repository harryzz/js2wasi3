// return() and throw() run finally blocks.
function* withFinally() {
  try {
    console.log("start");
    yield 1;
    yield 2;
    console.log("not reached");
  } finally {
    console.log("cleanup");
  }
}
const a = withFinally();
console.log(a.next());
console.log(a.return(42));
console.log(a.next());

const b = withFinally();
b.next();
try {
  b.throw(new Error("boom"));
} catch (e) {
  console.log("caught", e.message);
}
console.log(b.next());

// A finally that yields while returning, and one that overrides the return value.
function* yieldInFinally() {
  try {
    yield "a";
  } finally {
    yield "cleanup-1";
    console.log("after cleanup yield");
  }
}
const c = yieldInFinally();
console.log(c.next());
console.log(c.return("R"));
console.log(c.next());
console.log(c.next());

function* override() {
  try {
    yield 1;
  } finally {
    return "from finally";
  }
}
const d = override();
d.next();
console.log(d.return("ignored"));

// throw() caught inside the generator.
function* catcher() {
  while (true) {
    try {
      yield "waiting";
    } catch (e) {
      console.log("generator caught", e);
      yield "recovered";
    }
  }
}
const k = catcher();
console.log(k.next());
console.log(k.throw("oops"));
console.log(k.next());
console.log(k.throw("again"));
console.log(k.return("bye"));

// Nested try/finally, return through two finally blocks.
function* nested() {
  try {
    try {
      yield 1;
    } finally {
      console.log("inner finally");
    }
  } finally {
    console.log("outer finally");
  }
}
const n = nested();
n.next();
console.log(n.return(5));

// return()/throw() before the first next(): the body never runs.
const fresh = withFinally();
console.log(fresh.return("early"));
console.log(fresh.next());
const fresh2 = withFinally();
try {
  fresh2.throw(new TypeError("early throw"));
} catch (e) {
  console.log(e.name, e.message);
}

// Breaking out of for-of calls return().
for (const v of withFinally()) {
  console.log("got", v);
  break;
}

// Errors.
const running = (function* () {
  running.next();
})();
try {
  running.next();
} catch (e) {
  console.log(e.message);
}
try {
  withFinally().next.call({});
} catch (e) {
  console.log(e.message);
}
try {
  new withFinally();
} catch (e) {
  console.log(e.message);
}
