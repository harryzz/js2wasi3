// node's `assert` (T55): passing assertions of every kind, then the AssertionErrors node generates
// for primitives.
import assert, { strictEqual, deepStrictEqual, throws, AssertionError } from "node:assert";
import strict from "assert/strict";
assert(true); assert.ok(1); strictEqual(1, 1); deepStrictEqual({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] });
assert.deepEqual({ a: 1 }, { a: "1" }); assert.equal(1, "1"); assert.notStrictEqual(1, "1"); assert.match("abc", /b/);
throws(() => { throw new TypeError("x"); }, TypeError); throws(() => { throw new Error("abc"); }, /bc/);
throws(() => { throw new Error("m"); }, { message: "m" }); assert.doesNotThrow(() => 1); assert.ifError(null);
await assert.rejects(Promise.reject(new Error("r")), Error); await assert.doesNotReject(async () => 1);
console.log("passing ok", strict.equal === assert.strictEqual, assert.strict === strict, Object.keys(assert).join(" "));
const checks = [() => strictEqual(1, 2), () => strictEqual("a", "b", "custom"), () => assert.notStrictEqual(1, 1), () => assert.equal(1, 2),
  () => assert.fail(), () => assert.fail("boom"), () => throws(() => {}), () => assert.match("abc", /x/), () => assert.ok(0, "m"),
  () => assert.ifError(new Error("e")), () => strict.equal(1, "1"), () => throws(() => {}, TypeError), () => assert.doesNotThrow(() => { throw new Error("bad"); }),
  () => assert.ok(), () => strict(0, "s")];
for (const f of checks) { try { f(); console.log("no throw"); } catch (e) { console.log(JSON.stringify([e.name, e.code, e.message, e.operator, e.generatedMessage]), e instanceof AssertionError); } }
