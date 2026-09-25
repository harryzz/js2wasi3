// Strict-mode TypeErrors from property operations (node's messages).
function setOn(v, k) {
  try {
    v[k] = 1;
    console.log("set", k, "ok");
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
}
function getFrom(v, k) {
  try {
    console.log("get", k, v[k]);
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
}
function del(v, k) {
  try {
    console.log("delete", k, delete v[k]);
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
}
function has(v, k) {
  try {
    console.log("in", k, k in v);
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
}
function create(p) {
  try {
    console.log("create", typeof Object.create(p));
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
}

setOn("abc", "x");
setOn("abc", 0);
setOn("abc", "length");
setOn("abc", 7);
setOn(5, "x");
setOn(true, "x");
setOn(null, "x");
setOn(undefined, 3);
setOn({}, "x");

getFrom(null, "x");
getFrom(undefined, 0);
getFrom("abc", 1);

del("abc", 0);
del("abc", "length");
del("abc", "x");
del(5, "x");
del(null, "x");
del({ x: 1 }, "x");

has("abc", "a");
has(5, 1);
has(null, "a");
has(undefined, "a");
has({ a: 1 }, "a");

create(5);
create("x");
create(undefined);
create(null);
create({});

try {
  Object.getPrototypeOf(null);
} catch (e) {
  console.log(e.message);
}
try {
  Object.hasOwn(undefined, "a");
} catch (e) {
  console.log(e.message);
}
