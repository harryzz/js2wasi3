// T87: a function declaration and a `var` of the same name in one function scope (the var declared first).
(function () {
  console.log(typeof __decl, __decl());
  var __decl = 1;
  console.log(__decl);
  function __decl() { return 1; }
})();
(function () {
  function g() { return "fn"; }
  var g;
  console.log(typeof g, g());
  var h = 2;
  function h() {}
  console.log(typeof h, h);
  console.log(typeof k);
  function k() {}
  var k;
})();
