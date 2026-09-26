// Timeout ids through ToPrimitive in a program that never names Symbol.toPrimitive (F74).
const t = setTimeout(() => console.log("never"), 5);
console.log(Number.isInteger(+t), `${t}` === String(+t), t + 0 === +t);
clearTimeout(`${t}`);
const im = setImmediate(() => {});
console.log(Number.isNaN(+im), `${im}`);
