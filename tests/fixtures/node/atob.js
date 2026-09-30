// `atob`/`btoa` (T56): node's DOMException for invalid input; `structuredClone`'s one shares it.
for (const f of [() => atob("a"), () => btoa("é€"), () => atob("a b=c"), () => atob(" aGk= "), () => btoa("é"), () => atob("aGk"), () => atob(), () => btoa(null), () => atob("YWJj\nZGVm")]) {
  try { console.log(JSON.stringify(f())); } catch (e) { console.log(e.name, e.code, e.message, e instanceof Error); }
}
const clone = () => structuredClone(() => 1);
try { clone(); } catch (e) { console.log(e.name, e.code, e.message); }
