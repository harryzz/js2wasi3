class ValidationError extends Error {
  constructor(field, message) {
    super(message);
    this.name = "ValidationError";
    this.field = field;
  }
}
class NotFound extends ValidationError {
  constructor(what) {
    super(what, what + " not found");
  }
  get code() {
    return 404;
  }
}
try {
  throw new NotFound("user");
} catch (e) {
  console.log(e instanceof NotFound, e instanceof ValidationError, e instanceof Error);
  console.log(e.name, e.message, e.field, e.code, e.toString());
  console.log(`${e}`, Object.keys(e));
}
class Plain extends Error {}
const p = new Plain("boom");
console.log(p.message, p.name, p instanceof Plain, p instanceof Error, p.constructor === Plain);
const E = TypeError;
const t = new E("bad");
console.log(t instanceof TypeError, t.message, t.constructor === TypeError, E.name, E.length);
console.log(typeof Error, Error.prototype.constructor === Error, RangeError("r").message);
class WithCause extends Error {
  constructor(m, c) {
    super(m, { cause: c });
  }
}
console.log(new WithCause("x", 42).cause);
throw new NotFound("page");
