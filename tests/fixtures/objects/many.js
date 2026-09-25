// Many properties: the hash index, deletes, re-adds, and objects used as maps.
const counts = {};
const text = "the quick brown fox jumps over the lazy dog the end";
let word = "";
for (let i = 0; i <= text.length; i++) {
  const c = i < text.length ? text[i] : " ";
  if (c === " ") {
    if (word !== "") counts[word] = (counts[word] ?? 0) + 1;
    word = "";
  } else {
    word += c;
  }
}
let s = "";
for (const w in counts) s += w + "=" + counts[w] + " ";
console.log(s);

const big = {};
for (let i = 0; i < 2000; i++) big["key" + i] = i * 2;
let sum = 0;
for (let i = 0; i < 2000; i++) sum += big["key" + i];
console.log(sum, big.key1999, big.key2000, "key500" in big);

for (let i = 0; i < 2000; i += 2) delete big["key" + i];
let n = 0;
let odd = 0;
for (const k in big) {
  n++;
  odd += big[k];
}
console.log(n, odd, "key500" in big, "key501" in big);

for (let i = 0; i < 2000; i += 2) big["key" + i] = -1;
let first = "";
let m = 0;
for (const k in big) {
  if (m < 3) first += k + " ";
  m++;
}
console.log(m, first);

// Integer keys come out ascending regardless of insertion order.
const nums = {};
for (let i = 20; i > 0; i--) nums[i * 7 % 23] = i;
let order = "";
for (const k in nums) order += k + " ";
console.log(order);
