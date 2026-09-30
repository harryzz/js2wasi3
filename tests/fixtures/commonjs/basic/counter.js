let n = 0;
exports.next = () => ++n;
exports.value = function () { return n; };
