// BigInt (T40). The arithmetic is native (crates/backend/src/bigint.rs); this file has the
// conversions and the methods.

// StringToBigInt: a BigInt, or `undefined` if `s` isn't a StringIntegerLiteral (whitespace
// around; decimal with an optional sign, or 0x/0o/0b; "" is 0n). The runtime calls it to compare
// strings with BigInts.
function $$StringToBigInt(s) {
  let start = 0;
  let end = s.length;
  while (start < end && $$isSpace(__intrinsic_unit(s, start))) {
    start++;
  }
  while (end > start && $$isSpace(__intrinsic_unit(s, end - 1))) {
    end--;
  }
  if (start === end) {
    return 0n;
  }
  let radix = 10;
  let negative = false;
  const c = __intrinsic_unit(s, start);
  if (end - start > 2 && c === 48) {
    const p = __intrinsic_unit(s, start + 1);
    if (p === 120 || p === 88) {
      radix = 16;
    } else if (p === 111 || p === 79) {
      radix = 8;
    } else if (p === 98 || p === 66) {
      radix = 2;
    }
    if (radix !== 10) {
      start += 2;
    }
  } else if (c === 43 || c === 45) {
    negative = c === 45;
    start++;
    if (start === end) {
      return undefined;
    }
  }
  // Digits in chunks that stay exact as numbers.
  let r = 0n;
  let chunk = 0;
  let scale = 1;
  for (let i = start; i < end; i++) {
    const d = $$digitValue(__intrinsic_unit(s, i));
    if (d >= radix) {
      return undefined;
    }
    if (scale * radix > 9007199254740992) {
      r = r * __intrinsic_bigint_from_number(scale) + __intrinsic_bigint_from_number(chunk);
      chunk = 0;
      scale = 1;
    }
    chunk = chunk * radix + d;
    scale *= radix;
  }
  r = r * __intrinsic_bigint_from_number(scale) + __intrinsic_bigint_from_number(chunk);
  return negative ? -r : r;
}

// thisBigIntValue.
function $$thisBigInt(v, name) {
  v = __intrinsic_unwrap(v);
  if (typeof v !== "bigint") {
    throw new TypeError("BigInt.prototype." + name + " requires that 'this' be a BigInt");
  }
  return v;
}

// ToBigInt (after ToPrimitive with hint number, as `BigInt(…)` does it).
function $$toBigIntPrim(prim) {
  const t = typeof prim;
  if (t === "bigint") {
    return prim;
  }
  if (t === "boolean") {
    return prim ? 1n : 0n;
  }
  if (t === "string") {
    const r = $$StringToBigInt(prim);
    if (r === undefined) {
      throw new SyntaxError("Cannot convert " + prim + " to a BigInt");
    }
    return r;
  }
  if (t === "symbol") {
    throw new TypeError("Cannot convert " + $$String(prim) + " to a BigInt");
  }
  throw new TypeError("Cannot convert " + __intrinsic_to_string(prim) + " to a BigInt");
}

// ToBigInt.
function $$toBigInt(value) {
  return $$toBigIntPrim(__intrinsic_to_primitive(value, 1));
}

// NumberToBigInt.
function $$numberToBigInt(x) {
  if (x !== x || x - x !== 0 || __intrinsic_trunc(x) !== x) {
    throw new RangeError("The number " + __intrinsic_number_to_string(x) + " cannot be converted to a BigInt because it is not an integer");
  }
  return __intrinsic_bigint_from_number(x);
}

// `BigInt(value)` (`new BigInt` is a TypeError: not a constructor).
function $$BigInt(value) {
  const prim = __intrinsic_to_primitive(value, 1);
  if (typeof prim === "number") {
    return $$numberToBigInt(prim);
  }
  return $$toBigIntPrim(prim);
}

// ToIndex, with V8's message.
function $$bigIntBits(bits) {
  if (bits === undefined) {
    return 0;
  }
  const n = $$toInteger(bits);
  if (n < 0 || n > 9007199254740991) {
    throw new RangeError("Invalid value: not (convertible to) a safe integer");
  }
  return n;
}

// BigInt values have at most 2^30 bits (V8's limit): a wider `asIntN`/`asUintN` changes nothing
// (or, for a negative `asUintN`, can't be represented).
function $$BigInt_asUintN(bits, bigint) {
  const n = $$bigIntBits(bits);
  const x = $$toBigInt(bigint);
  if (n === 0) {
    return 0n;
  }
  if (n > 1073741824) {
    if (x < 0n) {
      throw new RangeError("Maximum BigInt size exceeded");
    }
    return x;
  }
  return x & ((1n << __intrinsic_bigint_from_number(n)) - 1n);
}

function $$BigInt_asIntN(bits, bigint) {
  const n = $$bigIntBits(bits);
  const x = $$toBigInt(bigint);
  if (n === 0) {
    return 0n;
  }
  if (n > 1073741824) {
    return x;
  }
  const m = 1n << __intrinsic_bigint_from_number(n);
  const r = x & (m - 1n);
  return r >= m >> 1n ? r - m : r;
}

// (`radix` is optional: `length` 0.)
function $$BigInt_prototype_toString(radix = undefined) {
  const x = $$thisBigInt(this, "toString");
  let r = 10;
  if (radix !== undefined) {
    r = $$toInteger(radix);
  }
  if (r < 2 || r > 36) {
    throw new RangeError("toString() radix argument must be between 2 and 36");
  }
  return __intrinsic_bigint_to_string(x, r);
}

function $$BigInt_prototype_valueOf() {
  return $$thisBigInt(this, "valueOf");
}

// node's `en-US` form (digits grouped by 3 with commas); other locales and options need `Intl`.
function $$BigInt_prototype_toLocaleString(locales, options) {
  const x = $$thisBigInt(this, "toLocaleString");
  if ((locales !== undefined && locales !== "en-US") || options !== undefined) {
    throw new RangeError("BigInt.prototype.toLocaleString with locales or options is not supported (js2wasi3)");
  }
  const s = __intrinsic_bigint_to_string(x < 0n ? -x : x, 10);
  const n = s.length;
  let out = x < 0n ? "-" : "";
  for (let i = 0; i < n; i++) {
    if (i > 0 && (n - i) % 3 === 0) {
      out += ",";
    }
    out += __intrinsic_sub(s, i, i + 1);
  }
  return out;
}
