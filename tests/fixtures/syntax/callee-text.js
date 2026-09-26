const t = (f) => { try { f() } catch (e) { console.log(e.message) } };
t(() => { for (const x of {}) ; });
t(() => { new ({ a: 1, m() {} }.m)(); });
t(() => { ({ a: 1 }).x(); });
t(() => { ({ m() {} }).x(); });
t(() => { ({}).x(); });
