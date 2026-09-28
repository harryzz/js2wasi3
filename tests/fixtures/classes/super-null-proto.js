// super.x with a null or changed home prototype (T38, F168).
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name + ": " + e.message); } };
const o = { m() { return super.x; }, n() { super.y = 1; }, k(q) { return super[q]; } };
Object.setPrototypeOf(o, null);
t(() => o.m()); t(() => o.n()); t(() => o.k("z"));
class C { static m() { super.x = 1; } static g() { return super.x; } }
Object.setPrototypeOf(C, null);
t(() => C.m()); t(() => C.g());
// (`super[key] = v` reading the base before converting the key is tested by test262 only: node 24
// converts first.)
