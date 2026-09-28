// RegExp (T27). Conventions: crates/lower/src/builtins.rs.
// A RegExp is a native object (`CLASS_REGEXP`, prototype `RegExp.prototype`, an own `lastIndex`)
// holding its data `[source, flags, code]` out of reach (`source` as the getter shows it, flags in
// canonical order): `__intrinsic_regexp_new(data)` makes one,
// `__intrinsic_regexp_data(v)` reads it back (`null` for `RegExp.prototype`, `undefined` for
// anything else). `code` is the compiled pattern, shared by every RegExp of the same source and
// flags: `[prog, sets, groups, names, flagBits, regs, source, flags, caps, regs]`: `prog`, `caps`
// and `regs` are `$i32s` arrays (`__intrinsic_i32_*`: typed, unboxed); `sets` holds two slots per
// class (`negate`, its ranges as `$i32s`) or backreference (its groups, `null`); the escaped source
// and the canonical flags; `caps`/`regs` are reused by every match (no user code runs while
// matching).
// - The pattern is parsed into an AST of arrays (`$$reParse*`, node types `$$RE_*` below), then
//   compiled into `prog`, an array of numeric instructions (`$$reEmit`, opcodes `$$OP_*`).
// - `$$reRun` runs `prog` at one position with an explicit backtrack stack (never JS recursion,
//   so a catastrophic pattern is slow, as in V8, but can't overflow the Wasm stack). Entries are
//   three numbers: a choice `[pc, pos, 0]`, an undo of a capture slot `[-1, slot, old]` or a
//   register `[-2, reg, old]`, a lookaround barrier `[-3 (positive) | -4 (negative), pos, pc]`.
// - A quantifier keeps its iteration count and the position its iteration started in two
//   registers (undo-logged, so backtracking restores them) and fails an iteration past the
//   minimum that matched the empty string (the spec's RepeatMatcher).
// - Lookarounds are atomic: when the body matches, the choices above the barrier are dropped and
//   the undo entries kept. A lookbehind body is compiled backwards (`dir` 1).
// - Flag bits: d 1, g 2, i 4, m 8, s 16, u 32, v 64, y 128.
// Not implemented yet: `v`'s
// class set operations, subclassing (`new.target`).
// Uses unicode-case.js's tables, string.js's `$$toInteger`, array.js's `$$append`
// and `$$describe`.

// ---- AST node types ------------------------------------------------------------------------
// [1, c] a character (a code unit, a code point in Unicode mode)
// [2] `.`
// [3, negate, ranges] a class: sorted, merged `[lo, hi, lo, hi, …]`
// [4, items] a sequence
// [5, alternatives]
// [6, group, body] a capturing group
// [7, min, max, greedy, body, firstGroup, endGroup] a quantifier (max -1: no limit); the groups
//   inside are firstGroup + 1 … endGroup
// [8] `^`, [9] `$`, [10, negate] `\b` / `\B`
// [11, groups] a backreference (several groups for a name used more than once)
// [12, ahead, negate, body] a lookaround

// ---- parser state ---------------------------------------------------------------------------

let $$reSrc = null;
let $$reFlagText = null;
let $$reAt = 0;
let $$reLen = 0;
let $$reUnicode = false;
// The `v` flag (class set syntax) and `i` while parsing.
let $$reVMode = false;
let $$reNamed = false;
let $$reTotal = 0;
let $$reGroups = 0;
let $$reDisj = 0;
let $$reNames = null;
let $$reRefs = null;
let $$rePath = null;

function $$reError(msg) {
  throw new SyntaxError("Invalid regular expression: /" + $$reSrc + "/" + $$reFlagText + ": " + msg);
}

function $$rePeek(k) {
  const i = $$reAt + k;
  return i < $$reLen ? __intrinsic_unit($$reSrc, i) : -1;
}

function $$reIsDigit(c) {
  return c >= 48 && c <= 57;
}

function $$reHex(c) {
  if (c >= 48 && c <= 57) {
    return c - 48;
  }
  if (c >= 65 && c <= 70) {
    return c - 55;
  }
  if (c >= 97 && c <= 102) {
    return c - 87;
  }
  return -1;
}

function $$reIsLead(c) {
  return c >= 0xd800 && c <= 0xdbff;
}

function $$reIsTrail(c) {
  return c >= 0xdc00 && c <= 0xdfff;
}

// The next pattern character (a code point in Unicode mode), consumed.
function $$reNextChar() {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  $$reAt++;
  if ($$reUnicode && $$reIsLead(c) && $$reAt < $$reLen) {
    const d = __intrinsic_unit($$reSrc, $$reAt);
    if ($$reIsTrail(d)) {
      $$reAt++;
      return (c - 0xd800) * 0x400 + (d - 0xdc00) + 0x10000;
    }
  }
  return c;
}

// Count the capturing groups and see whether any is named (backreferences and `\k` depend on it).
function $$reScan() {
  let n = 0;
  let named = false;
  let inClass = false;
  for (let i = 0; i < $$reLen; i++) {
    const c = __intrinsic_unit($$reSrc, i);
    if (c === 92) {
      i++;
    } else if (inClass) {
      if (c === 93) {
        inClass = false;
      }
    } else if (c === 91) {
      inClass = true;
    } else if (c === 40) {
      if (i + 1 < $$reLen && __intrinsic_unit($$reSrc, i + 1) === 63) {
        if (i + 2 < $$reLen && __intrinsic_unit($$reSrc, i + 2) === 60) {
          const d = i + 3 < $$reLen ? __intrinsic_unit($$reSrc, i + 3) : -1;
          if (d !== 61 && d !== 33) {
            n++;
            named = true;
          }
        }
      } else {
        n++;
      }
    }
  }
  $$reTotal = n;
  $$reNamed = named;
}

// Pattern → AST. Also fills `$$reNames` (`[name, group, path]`) and resolves `\k<name>`.
function $$reParse(src, flagText, unicode) {
  $$reSrc = src;
  $$reFlagText = flagText;
  $$reAt = 0;
  $$reLen = src.length;
  $$reUnicode = unicode;
  $$reGroups = 0;
  $$reDisj = 0;
  $$reNames = [];
  $$reRefs = [];
  $$rePath = [];
  $$reScan();
  const ast = $$reDisjunction();
  if ($$reAt < $$reLen) {
    $$reError("Unmatched ')'");
  }
  for (let i = 0; i < $$reRefs.length; i++) {
    const ref = $$reRefs[i];
    const name = ref[2];
    for (let j = 0; j < $$reNames.length; j++) {
      if ($$reNames[j][0] === name) {
        $$append(ref[1], $$reNames[j][1]);
      }
    }
    if (ref[1].length === 0) {
      $$reError("Invalid named capture referenced");
    }
  }
  return ast;
}

function $$reDisjunction() {
  $$reDisj++;
  const entry = [$$reDisj, 0];
  $$append($$rePath, entry);
  const alts = [];
  $$append(alts, $$reAlternative());
  while ($$rePeek(0) === 124) {
    $$reAt++;
    entry[1] = entry[1] + 1;
    $$append(alts, $$reAlternative());
  }
  $$rePath.length = $$rePath.length - 1;
  return alts.length === 1 ? alts[0] : [5, alts];
}

function $$reAlternative() {
  const items = [];
  while ($$reAt < $$reLen) {
    const c = __intrinsic_unit($$reSrc, $$reAt);
    if (c === 124 || c === 41) {
      break;
    }
    $$reTerm(items);
  }
  return [4, items];
}

// Parse `{n}`, `{n,}`, `{n,m}` at `$$reAt` (a `{`): `[min, max]` (max -1: none), consumed, or
// null (nothing consumed).
function $$reBraces() {
  let i = $$reAt + 1;
  const start = i;
  let min = 0;
  while (i < $$reLen && $$reIsDigit(__intrinsic_unit($$reSrc, i))) {
    min = min * 10 + (__intrinsic_unit($$reSrc, i) - 48);
    i++;
  }
  if (i === start) {
    return null;
  }
  let max = min;
  if (i < $$reLen && __intrinsic_unit($$reSrc, i) === 44) {
    i++;
    const s2 = i;
    max = 0;
    while (i < $$reLen && $$reIsDigit(__intrinsic_unit($$reSrc, i))) {
      max = max * 10 + (__intrinsic_unit($$reSrc, i) - 48);
      i++;
    }
    if (i === s2) {
      max = -1;
    }
  }
  if (i >= $$reLen || __intrinsic_unit($$reSrc, i) !== 125) {
    return null;
  }
  $$reAt = i + 1;
  // V8 caps counts at 2^31 - 1.
  if (min > 2147483647) {
    min = 2147483647;
  }
  if (max > 2147483647) {
    max = 2147483647;
  }
  return [min, max];
}

function $$reTerm(items) {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  const groupsBefore = $$reGroups;
  let atom = null;
  // Assertions and lookarounds.
  if (c === 94) {
    $$reAt++;
    $$append(items, [8]);
    $$reNoQuantifier("Nothing to repeat");
    return;
  }
  if (c === 36) {
    $$reAt++;
    $$append(items, [9]);
    $$reNoQuantifier("Nothing to repeat");
    return;
  }
  if (c === 92 && ($$rePeek(1) === 98 || $$rePeek(1) === 66)) {
    $$append(items, [10, $$rePeek(1) === 66]);
    $$reAt += 2;
    $$reNoQuantifier("Nothing to repeat");
    return;
  }
  if (c === 40 && $$rePeek(1) === 63) {
    const d = $$rePeek(2);
    const behind = d === 60 && ($$rePeek(3) === 61 || $$rePeek(3) === 33);
    if (d === 61 || d === 33 || behind) {
      const negate = behind ? $$rePeek(3) === 33 : d === 33;
      $$reAt += behind ? 4 : 3;
      const body = $$reDisjunction();
      if ($$rePeek(0) !== 41) {
        $$reError("Unterminated group");
      }
      $$reAt++;
      atom = [12, !behind, negate, body];
      if (behind || $$reUnicode) {
        $$append(items, atom);
        $$reNoQuantifier("Invalid quantifier");
        return;
      }
    }
  }
  if (atom === null) {
    atom = $$reAtom();
  }
  // A quantifier.
  let min = -1;
  let max = -1;
  const q = $$rePeek(0);
  if (q === 42) {
    min = 0;
    max = -1;
    $$reAt++;
  } else if (q === 43) {
    min = 1;
    max = -1;
    $$reAt++;
  } else if (q === 63) {
    min = 0;
    max = 1;
    $$reAt++;
  } else if (q === 123) {
    const b = $$reBraces();
    if (b !== null) {
      min = b[0];
      max = b[1];
      if (max !== -1 && max < min) {
        $$reError("numbers out of order in {} quantifier");
      }
    } else if ($$reUnicode) {
      $$reError("Incomplete quantifier");
    }
  }
  if (min < 0) {
    $$append(items, atom);
    return;
  }
  let greedy = true;
  if ($$rePeek(0) === 63) {
    greedy = false;
    $$reAt++;
  }
  $$append(items, [7, min, max, greedy, atom, groupsBefore, $$reGroups]);
}

// After an assertion: a quantifier is an error.
function $$reNoQuantifier(msg) {
  const q = $$rePeek(0);
  if (q === 42 || q === 43 || q === 63) {
    $$reError(msg);
  }
  if (q === 123) {
    const save = $$reAt;
    if ($$reBraces() !== null) {
      $$reAt = save;
      $$reError(msg);
    }
    $$reAt = save;
  }
}

function $$reAtom() {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if (c === 46) {
    $$reAt++;
    return [2];
  }
  if (c === 40) {
    return $$reGroup();
  }
  if (c === 91) {
    return $$reVMode ? $$reClassV() : $$reClass();
  }
  if (c === 42 || c === 43 || c === 63) {
    $$reError("Nothing to repeat");
  }
  if (c === 123) {
    const save = $$reAt;
    if ($$reBraces() !== null) {
      $$reAt = save;
      $$reError("Nothing to repeat");
    }
    if ($$reUnicode) {
      $$reError("Lone quantifier brackets");
    }
    $$reAt++;
    return [1, c];
  }
  if ((c === 125 || c === 93) && $$reUnicode) {
    $$reError("Lone quantifier brackets");
  }
  if (c === 92) {
    return $$reAtomEscape();
  }
  return [1, $$reNextChar()];
}

// Does the current alternative path allow another group named like one on `other`: are they in
// different alternatives of some disjunction?
function $$reApart(path, other) {
  const n = path.length < other.length ? path.length : other.length;
  for (let i = 0; i < n; i++) {
    if (path[i][0] !== other[i][0]) {
      return false;
    }
    if (path[i][1] !== other[i][1]) {
      return true;
    }
  }
  return false;
}

function $$reGroup() {
  $$reAt++;
  let group = 0;
  if ($$rePeek(0) === 63) {
    const d = $$rePeek(1);
    if (d === 58) {
      $$reAt += 2;
    } else if (d === 60) {
      $$reAt += 2;
      const name = $$reGroupName();
      $$reGroups++;
      group = $$reGroups;
      const path = [];
      for (let i = 0; i < $$rePath.length; i++) {
        $$append(path, [$$rePath[i][0], $$rePath[i][1]]);
      }
      for (let i = 0; i < $$reNames.length; i++) {
        if ($$reNames[i][0] === name && !$$reApart(path, $$reNames[i][2])) {
          $$reError("Duplicate capture group name");
        }
      }
      $$append($$reNames, [name, group, path]);
    } else {
      $$reError("Invalid group");
    }
  } else {
    $$reGroups++;
    group = $$reGroups;
  }
  const body = $$reDisjunction();
  if ($$rePeek(0) !== 41) {
    $$reError("Unterminated group");
  }
  $$reAt++;
  return group === 0 ? body : [6, group, body];
}

function $$reIdStart(c) {
  return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c === 36 || c === 95 || c >= 128;
}

// `name>` after `(?<` or `\k<`: the name, consumed.
function $$reGroupName() {
  const start = $$reAt;
  while ($$reAt < $$reLen) {
    const c = __intrinsic_unit($$reSrc, $$reAt);
    if (c === 62) {
      break;
    }
    const ok = $$reAt === start ? $$reIdStart(c) : $$reIdStart(c) || $$reIsDigit(c);
    if (!ok) {
      $$reError("Invalid capture group name");
    }
    $$reAt++;
  }
  if ($$reAt >= $$reLen || $$reAt === start) {
    $$reError("Invalid capture group name");
  }
  const name = __intrinsic_sub($$reSrc, start, $$reAt);
  $$reAt++;
  return name;
}

// The ranges of `\d`, `\s`, `\w` (and their negations: `negate`).
function $$reClassEscape(c) {
  let r = null;
  const lower = c | 32;
  if (lower === 100) {
    r = [48, 57];
  } else if (lower === 119) {
    r = [48, 57, 65, 90, 95, 95, 97, 122];
    if (c === 87 && $$reIcase && $$reUnicode) {
      // With `u` and `i`, ſ and K (Kelvin) are word characters (WordCharacters): not in `\W`.
      r = [48, 57, 65, 90, 95, 95, 97, 122, 0x17f, 0x17f, 0x212a, 0x212a];
    }
  } else {
    r = [9, 13, 32, 32, 0xa0, 0xa0, 0x1680, 0x1680, 0x2000, 0x200a, 0x2028, 0x2029, 0x202f, 0x202f,
      0x205f, 0x205f, 0x3000, 0x3000, 0xfeff, 0xfeff];
  }
  if ($$reVMode && $$reIcase) {
    // `v` + `i`: complement of the folded set (MaybeSimpleCaseFolding).
    r = $$reFold(r);
  }
  return c === lower ? r : $$reComplement(r);
}

function $$reMaxChar() {
  return $$reUnicode ? 0x10ffff : 0xffff;
}

function $$reComplement(r) {
  const out = [];
  let next = 0;
  for (let i = 0; i < r.length; i += 2) {
    if (r[i] > next) {
      $$append(out, next);
      $$append(out, r[i] - 1);
    }
    next = r[i + 1] + 1;
  }
  const max = $$reMaxChar();
  if (next <= max) {
    $$append(out, next);
    $$append(out, max);
  }
  return out;
}

// Sort and merge `[lo, hi, …]` pairs.
function $$reNormalize(r) {
  const n = r.length / 2;
  // Insertion sort of the pairs by `lo` (classes are small; big sets, from `\p{…}`, arrive sorted).
  let sorted = true;
  for (let i = 1; i < n && sorted; i++) {
    sorted = r[2 * i - 2] <= r[2 * i];
  }
  for (let i = 1; i < n && !sorted; i++) {
    const lo = r[2 * i];
    const hi = r[2 * i + 1];
    let j = i - 1;
    while (j >= 0 && r[2 * j] > lo) {
      r[2 * j + 2] = r[2 * j];
      r[2 * j + 3] = r[2 * j + 1];
      j--;
    }
    r[2 * j + 2] = lo;
    r[2 * j + 3] = hi;
  }
  const out = [];
  for (let i = 0; i < n; i++) {
    const lo = r[2 * i];
    const hi = r[2 * i + 1];
    const k = out.length;
    if (k > 0 && lo <= out[k - 1] + 1) {
      if (hi > out[k - 1]) {
        out[k - 1] = hi;
      }
    } else {
      $$append(out, lo);
      $$append(out, hi);
    }
  }
  return out;
}

// `\c` + a control letter at `$$reAt` (after the `c`): its value, consumed, or -1.
function $$reControl(inClass) {
  const d = $$rePeek(0);
  if ((d >= 65 && d <= 90) || (d >= 97 && d <= 122)) {
    $$reAt++;
    return d % 32;
  }
  // Annex B: `[\c0]`, `[\c_]`.
  if (inClass && !$$reUnicode && ($$reIsDigit(d) || d === 95)) {
    $$reAt++;
    return d % 32;
  }
  return -1;
}

// `\u` escapes after the `u`: the character, consumed, or -1.
function $$reUnicodeEscape() {
  if ($$reUnicode && $$rePeek(0) === 123) {
    let i = $$reAt + 1;
    let v = 0;
    const start = i;
    while (i < $$reLen && $$reHex(__intrinsic_unit($$reSrc, i)) >= 0) {
      v = v * 16 + $$reHex(__intrinsic_unit($$reSrc, i));
      if (v > 0x10ffff) {
        $$reError("Invalid Unicode escape");
      }
      i++;
    }
    if (i === start || i >= $$reLen || __intrinsic_unit($$reSrc, i) !== 125) {
      $$reError("Invalid Unicode escape");
    }
    $$reAt = i + 1;
    return v;
  }
  const v = $$reHex4($$reAt);
  if (v < 0) {
    return -1;
  }
  $$reAt += 4;
  if ($$reUnicode && $$reIsLead(v) && $$rePeek(0) === 92 && $$rePeek(1) === 117) {
    const w = $$reHex4($$reAt + 2);
    if ($$reIsTrail(w)) {
      $$reAt += 6;
      return (v - 0xd800) * 0x400 + (w - 0xdc00) + 0x10000;
    }
  }
  return v;
}

function $$reHex4(i) {
  if (i + 4 > $$reLen) {
    return -1;
  }
  let v = 0;
  for (let k = 0; k < 4; k++) {
    const h = $$reHex(__intrinsic_unit($$reSrc, i + k));
    if (h < 0) {
      return -1;
    }
    v = v * 16 + h;
  }
  return v;
}

// A legacy octal escape (Annex B) from `$$reAt`: up to three octal digits, at most 0o377.
function $$reOctal() {
  let v = __intrinsic_unit($$reSrc, $$reAt) - 48;
  $$reAt++;
  const d = $$rePeek(0);
  if (d >= 48 && d <= 55) {
    v = v * 8 + (d - 48);
    $$reAt++;
    const e = $$rePeek(0);
    if (v < 32 && e >= 48 && e <= 55) {
      v = v * 8 + (e - 48);
      $$reAt++;
    }
  }
  return v;
}

// A character escape shared by atoms and classes (after the `\`): the character, consumed.
// Class escapes (`\d`, …) and backreferences are handled by the callers.
function $$reCharEscape(inClass) {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if (c === 102) {
    $$reAt++;
    return 12;
  }
  if (c === 110) {
    $$reAt++;
    return 10;
  }
  if (c === 114) {
    $$reAt++;
    return 13;
  }
  if (c === 116) {
    $$reAt++;
    return 9;
  }
  if (c === 118) {
    $$reAt++;
    return 11;
  }
  if (c === 99) {
    $$reAt++;
    const v = $$reControl(inClass);
    if (v >= 0) {
      return v;
    }
    if ($$reUnicode) {
      $$reError("Invalid Unicode escape");
    }
    // `\c` without a letter is a backslash (the `c` is read next).
    $$reAt--;
    return 92;
  }
  if (c === 48 && !$$reIsDigit($$rePeek(1))) {
    $$reAt++;
    return 0;
  }
  if ($$reIsDigit(c)) {
    if ($$reUnicode) {
      $$reError("Invalid decimal escape");
    }
    if (c >= 56) {
      $$reAt++;
      return c;
    }
    return $$reOctal();
  }
  if (c === 120) {
    $$reAt++;
    const h1 = $$reHex($$rePeek(0));
    const h2 = $$reHex($$rePeek(1));
    if (h1 >= 0 && h2 >= 0) {
      $$reAt += 2;
      return h1 * 16 + h2;
    }
    if ($$reUnicode) {
      $$reError("Invalid escape");
    }
    return 120;
  }
  if (c === 117) {
    $$reAt++;
    const v = $$reUnicodeEscape();
    if (v >= 0) {
      return v;
    }
    if ($$reUnicode) {
      $$reError("Invalid Unicode escape");
    }
    return 117;
  }
  if ($$reUnicode) {
    // Only syntax characters, `/` and (in a class) `-` can be escaped.
    const ok =
      c === 94 || c === 36 || c === 92 || c === 46 || c === 42 || c === 43 || c === 63 ||
      c === 40 || c === 41 || c === 91 || c === 93 || c === 123 || c === 125 || c === 124 ||
      c === 47 || (inClass && c === 45);
    if (!ok) {
      $$reError("Invalid escape");
    }
  }
  return $$reNextChar();
}

// `\p{…}` / `\P{…}` (Unicode mode) at `$$reAt` (the `p`): its ranges, consumed. The sets come from
// `$$reUnicodeSet`, generated for the texts the program's source names (builtins.rs): a text
// built at run time finds none and is an invalid name here.
function $$reProperty(inClass) {
  const negate = __intrinsic_unit($$reSrc, $$reAt) === 80;
  $$reAt++;
  const msg = inClass ? "Invalid property name in character class" : "Invalid property name";
  if ($$rePeek(0) !== 123) {
    $$reError(msg);
  }
  const start = $$reAt + 1;
  let end = start;
  while (end < $$reLen && __intrinsic_unit($$reSrc, end) !== 125) {
    end++;
  }
  if (end >= $$reLen) {
    $$reError(msg);
  }
  const enc = $$reUnicodeSet(__intrinsic_sub($$reSrc, start, end));
  if (enc === null) {
    $$reError(msg);
  }
  $$reAt = end + 1;
  const nums = $$reDecode(enc);
  const r = [];
  let prev = 0;
  for (let i = 0; i < __intrinsic_i32_len(nums); i += 2) {
    const lo = prev + __intrinsic_i32_get(nums, i);
    const hi = lo + __intrinsic_i32_get(nums, i + 1);
    $$append(r, lo);
    $$append(r, hi);
    prev = hi;
  }
  const set = $$reVMode && $$reIcase ? $$reFold(r) : r;
  return negate ? $$reComplement(set) : set;
}

function $$reAtomEscape() {
  $$reAt++;
  if ($$reAt >= $$reLen) {
    $$reError("\\ at end of pattern");
  }
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if ($$reUnicode && (c === 112 || c === 80)) {
    return [3, false, $$reProperty(false)];
  }
  const lower = c | 32;
  if (lower === 100 || lower === 115 || lower === 119) {
    $$reAt++;
    return [3, false, $$reClassEscape(c)];
  }
  if (c >= 49 && c <= 57) {
    // A backreference if there are that many groups; else (Annex B) an octal escape or `8`/`9`.
    const save = $$reAt;
    let n = 0;
    while ($$reAt < $$reLen && $$reIsDigit(__intrinsic_unit($$reSrc, $$reAt))) {
      n = n * 10 + (__intrinsic_unit($$reSrc, $$reAt) - 48);
      $$reAt++;
    }
    if (n <= $$reTotal) {
      return [11, [n]];
    }
    if ($$reUnicode) {
      $$reError("Invalid escape");
    }
    $$reAt = save;
  }
  if (c === 107 && ($$reNamed || $$reUnicode)) {
    $$reAt++;
    if ($$rePeek(0) !== 60) {
      $$reError("Invalid named reference");
    }
    $$reAt++;
    const name = $$reGroupName();
    const ref = [11, [], name];
    $$append($$reRefs, ref);
    return ref;
  }
  return [1, $$reCharEscape(false)];
}

// One class atom: a character (>= 0) or, for `\d` etc., -1 with the ranges in `$$reClassSet`.
let $$reClassSet = null;

function $$reClassAtom() {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if (c !== 92) {
    return $$reNextChar();
  }
  $$reAt++;
  if ($$reAt >= $$reLen) {
    $$reError("\\ at end of pattern");
  }
  const d = __intrinsic_unit($$reSrc, $$reAt);
  const lower = d | 32;
  if (lower === 100 || lower === 115 || lower === 119) {
    $$reAt++;
    $$reClassSet = $$reClassEscape(d);
    return -1;
  }
  if ($$reUnicode && (d === 112 || d === 80)) {
    $$reClassSet = $$reProperty(true);
    return -1;
  }
  if (d === 98) {
    $$reAt++;
    return 8;
  }
  if (d === 45 && $$reUnicode) {
    $$reAt++;
    return 45;
  }
  if (d === 107 && !$$reUnicode) {
    $$reAt++;
    return 107;
  }
  return $$reCharEscape(true);
}

// ---- `v` mode classes (ClassSetExpression) --------------------------------------------------
// A set is `[ranges, strings]`: normalized ranges and the strings of other lengths than one code
// point (from `\q{…}`), as JS strings. With `i`, operands are folded (simple case folding) first.

function $$reIntersect(a, b) {
  const out = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    const lo = a[i] > b[j] ? a[i] : b[j];
    const hi = a[i + 1] < b[j + 1] ? a[i + 1] : b[j + 1];
    if (lo <= hi) {
      $$append(out, lo);
      $$append(out, hi);
    }
    if (a[i + 1] < b[j + 1]) {
      i += 2;
    } else {
      j += 2;
    }
  }
  return out;
}

function $$reUnion(a, b) {
  const out = [];
  for (let i = 0; i < a.length; i++) {
    $$append(out, a[i]);
  }
  for (let i = 0; i < b.length; i++) {
    $$append(out, b[i]);
  }
  return $$reNormalize(out);
}

// Simple case folding of a set of code points: the folded characters replaced by their folds.
function $$reFold(r) {
  const set = $$reI32(r);
  const added = [];
  const removed = [];
  const runs = $$reCaseRuns(true);
  const n = __intrinsic_i32_len(runs);
  for (let j = 0; j < n; j += 4) {
    const start = __intrinsic_i32_get(runs, j);
    const count = __intrinsic_i32_get(runs, j + 1);
    const delta = __intrinsic_i32_get(runs, j + 2);
    const stride = __intrinsic_i32_get(runs, j + 3);
    for (let k = 0; k < count; k++) {
      const c = start + k * stride;
      if ($$reInRanges(set, c)) {
        $$append(removed, c);
        $$append(removed, c);
        $$append(added, c + delta);
        $$append(added, c + delta);
      }
    }
  }
  const kept = $$reIntersect(r, $$reComplement($$reNormalize(removed)));
  return $$reUnion(kept, added);
}

function $$reFoldString(str) {
  let out = "";
  let i = 0;
  while (i < str.length) {
    const c = str.codePointAt(i);
    out = out + String.fromCodePoint($$reCanon(c, true));
    i += c > 0xffff ? 2 : 1;
  }
  return out;
}

function $$reHasString(list, str) {
  for (let i = 0; i < list.length; i++) {
    if (list[i] === str) {
      return true;
    }
  }
  return false;
}

// ClassSetCharacter / escapes after `[` or in `\q{…}`: a code point, consumed.
function $$reVChar() {
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if (c === 92) {
    $$reAt++;
    if ($$reAt >= $$reLen) {
      $$reError("\\ at end of pattern");
    }
    const d = __intrinsic_unit($$reSrc, $$reAt);
    if (d === 98) {
      $$reAt++;
      return 8;
    }
    // ClassSetReservedPunctuator: & - ! # % , : ; < = > @ ` ~
    if (d === 38 || d === 45 || d === 33 || d === 35 || d === 37 || d === 44 || d === 58 ||
      d === 59 || d === 60 || d === 61 || d === 62 || d === 64 || d === 96 || d === 126) {
      $$reAt++;
      return d;
    }
    return $$reCharEscape(true);
  }
  // ClassSetSyntaxCharacter: ( ) [ ] { } / - \ |
  if (c === 40 || c === 41 || c === 91 || c === 93 || c === 123 || c === 125 || c === 47 ||
    c === 45 || c === 124) {
    $$reError("Invalid character in character class");
  }
  // ClassSetReservedDoublePunctuator.
  if ($$rePeek(1) === c && (c === 38 || c === 33 || c === 35 || c === 36 || c === 37 || c === 42 ||
    c === 43 || c === 44 || c === 46 || c === 58 || c === 59 || c === 60 || c === 61 || c === 62 ||
    c === 63 || c === 64 || c === 94 || c === 96 || c === 126)) {
    $$reError("Invalid set operation in character class");
  }
  return $$reNextChar();
}

// A ClassSetOperand or the start of a range: `[set, char]` (char: the code point of a lone
// character, else -1).
function $$reVOperand() {
  if ($$reAt >= $$reLen) {
    $$reError("Unterminated character class");
  }
  const c = __intrinsic_unit($$reSrc, $$reAt);
  if (c === 91) {
    return [$$reClassSetV(), -1];
  }
  if (c === 92) {
    const d = $$rePeek(1);
    const lower = d | 32;
    if (lower === 100 || lower === 115 || lower === 119) {
      $$reAt += 2;
      return [[$$reClassEscape(d), []], -1];
    }
    if (d === 112 || d === 80) {
      $$reAt++;
      return [[$$reProperty(true), []], -1];
    }
    if (d === 113) {
      // `\q{a|bc|…}`
      $$reAt += 2;
      if ($$rePeek(0) !== 123) {
        $$reError("Invalid escape");
      }
      $$reAt++;
      let ranges = [];
      const strings = [];
      let str = "";
      let n = 0;
      while (true) {
        if ($$reAt >= $$reLen) {
          $$reError("Invalid character in character class");
        }
        const e = __intrinsic_unit($$reSrc, $$reAt);
        if (e === 124 || e === 125) {
          $$reAt++;
          if (n === 1) {
            const cp = $$reIcase ? $$reCanon(str.codePointAt(0), true) : str.codePointAt(0);
            ranges = $$reUnion(ranges, [cp, cp]);
          } else {
            const s2 = $$reIcase ? $$reFoldString(str) : str;
            if (!$$reHasString(strings, s2)) {
              $$append(strings, s2);
            }
          }
          str = "";
          n = 0;
          if (e === 125) {
            break;
          }
          continue;
        }
        str = str + String.fromCodePoint($$reVChar());
        n++;
      }
      return [[ranges, strings], -1];
    }
  }
  const ch = $$reVChar();
  const k = $$reIcase ? $$reCanon(ch, true) : ch;
  return [[[k, k], []], ch];
}

function $$reSetUnion(a, b) {
  const strings = [];
  for (let i = 0; i < a[1].length; i++) {
    $$append(strings, a[1][i]);
  }
  for (let i = 0; i < b[1].length; i++) {
    if (!$$reHasString(strings, b[1][i])) {
      $$append(strings, b[1][i]);
    }
  }
  return [$$reUnion(a[0], b[0]), strings];
}

// `[…]` in `v` mode at `$$reAt` (the `[`): the set (complemented if negated), consumed.
function $$reClassSetV() {
  $$reAt++;
  let negate = false;
  if ($$rePeek(0) === 94) {
    negate = true;
    $$reAt++;
  }
  let set = [[], []];
  let op = 0;
  let count = 0;
  // Was the only item so far a range (not an operand of `&&`/`--`)?
  let range = false;
  while (true) {
    if ($$reAt >= $$reLen) {
      $$reError("Unterminated character class");
    }
    const c = __intrinsic_unit($$reSrc, $$reAt);
    if (c === 93) {
      $$reAt++;
      break;
    }
    // An operator between operands: `&&` or `--` (and no mixing, no ranges with them).
    const d = $$rePeek(1);
    if ((c === 38 && d === 38) || (c === 45 && d === 45)) {
      const thisOp = c === 38 ? 1 : 2;
      if (count === 0 && thisOp === 2) {
        $$reError("Invalid character in character class");
      }
      if (count === 0 || range || (op !== 0 && op !== thisOp) || (op === 0 && count > 1)) {
        $$reError("Invalid set operation in character class");
      }
      op = thisOp;
      $$reAt += 2;
      const e = $$rePeek(0);
      if (e === 93 || e === -1 || (thisOp === 1 && e === 38)) {
        $$reError("Invalid character in character class");
      }
      const [b] = $$reVOperand();
      if (op === 1) {
        const strings = [];
        for (let i = 0; i < set[1].length; i++) {
          if ($$reHasString(b[1], set[1][i])) {
            $$append(strings, set[1][i]);
          }
        }
        set = [$$reIntersect(set[0], b[0]), strings];
      } else {
        const strings = [];
        for (let i = 0; i < set[1].length; i++) {
          if (!$$reHasString(b[1], set[1][i])) {
            $$append(strings, set[1][i]);
          }
        }
        set = [$$reIntersect(set[0], $$reComplement(b[0])), strings];
      }
      count++;
      continue;
    }
    if (op !== 0) {
      $$reError("Invalid set operation in character class");
    }
    const [a, ch] = $$reVOperand();
    let item = a;
    if (ch >= 0 && $$rePeek(0) === 45 && $$rePeek(1) !== 45) {
      // A range.
      $$reAt++;
      if ($$rePeek(0) === 93 || $$rePeek(0) === -1) {
        $$reError("Invalid character in character class");
      }
      const [, hi] = $$reVOperand();
      if (hi < 0) {
        $$reError("Invalid character class");
      }
      if (ch > hi) {
        $$reError("Range out of order in character class");
      }
      item = [$$reIcase ? $$reFold([ch, hi]) : [ch, hi], []];
      range = true;
    } else if (ch < 0 && $$rePeek(0) === 45 && $$rePeek(1) !== 45) {
      $$reError("Invalid character class");
    }
    set = count === 0 ? item : $$reSetUnion(set, item);
    count++;
  }
  if (negate) {
    if (set[1].length > 0) {
      $$reError("Negated character class may contain strings");
    }
    return [$$reComplement(set[0]), []];
  }
  return set;
}

// A `v` mode class as an AST node: a class, or with strings an alternation, longest first.
function $$reClassV() {
  const set = $$reClassSetV();
  const strings = set[1];
  if (strings.length === 0) {
    return [3, false, set[0]];
  }
  // Sort by length (code units; longer strings first), the empty string last.
  const sorted = [];
  for (let i = 0; i < strings.length; i++) {
    let j = sorted.length;
    $$append(sorted, strings[i]);
    while (j > 0 && sorted[j - 1].length < strings[i].length) {
      sorted[j] = sorted[j - 1];
      j--;
    }
    sorted[j] = strings[i];
  }
  const alts = [];
  let empty = false;
  for (let i = 0; i < sorted.length; i++) {
    const str = sorted[i];
    if (str.length === 0) {
      empty = true;
      continue;
    }
    const items = [];
    let k = 0;
    while (k < str.length) {
      const cp = str.codePointAt(k);
      $$append(items, [1, cp]);
      k += cp > 0xffff ? 2 : 1;
    }
    $$append(alts, [4, items]);
  }
  $$append(alts, [3, false, set[0]]);
  if (empty) {
    $$append(alts, [4, []]);
  }
  return [5, alts];
}

function $$reClass() {
  $$reAt++;
  let negate = false;
  if ($$rePeek(0) === 94) {
    negate = true;
    $$reAt++;
  }
  const r = [];
  while (true) {
    if ($$reAt >= $$reLen) {
      $$reError("Unterminated character class");
    }
    if (__intrinsic_unit($$reSrc, $$reAt) === 93) {
      $$reAt++;
      break;
    }
    const a = $$reClassAtom();
    const aSet = $$reClassSet;
    if ($$rePeek(0) === 45 && $$rePeek(1) !== 93 && $$rePeek(1) !== -1) {
      $$reAt++;
      const b = $$reClassAtom();
      const bSet = $$reClassSet;
      if (a < 0 || b < 0) {
        if ($$reUnicode) {
          $$reError("Invalid character class");
        }
        // Annex B: `[\d-z]` is `\d`, `-` and `z`.
        $$reAddAtom(r, a, aSet);
        $$append(r, 45);
        $$append(r, 45);
        $$reAddAtom(r, b, bSet);
        continue;
      }
      if (a > b) {
        $$reError("Range out of order in character class");
      }
      $$append(r, a);
      $$append(r, b);
      continue;
    }
    $$reAddAtom(r, a, aSet);
  }
  return [3, negate, $$reNormalize(r)];
}

function $$reAddAtom(r, a, set) {
  if (a >= 0) {
    $$append(r, a);
    $$append(r, a);
    return;
  }
  for (let i = 0; i < set.length; i++) {
    $$append(r, set[i]);
  }
}

// ---- compiler ---------------------------------------------------------------------------------
// Instructions (operands follow the opcode):
// 0 MATCH
// 1 CHAR c dir              one character equal to `c` (canonicalized with `i`)
// 2 ANY dir                 `.`
// 3 CLASS set dir           a character in class `set` (`sets[2 * set]`: negate, `+ 1`: ranges)
// 4 SPLIT alt               push a choice at `alt`, go on
// 5 GOTO to
// 6 SAVE slot               capture slot = position
// 7 RESET from to           capture slots [from, to) = -1
// 8 BOL, 9 EOL, 10 WORDB negate
// 11 BACKREF groups dir     `sets[groups]`: a list of groups
// 12 LOOK negate end        push a barrier; the body follows, then LOOK_END, then `end`
// 13 LOOK_END
// 14 REPEAT_INIT reg        registers reg (count) and reg + 1 (iteration start)
// 15 REPEAT_LOOP reg min max greedy exit   the body follows
// 16 REPEAT_START reg       iteration start = position
// 17 REPEAT_END reg min loop
// 18 STAR min max next      a greedy loop over the one-character instruction that follows (CHAR,
//                           ANY or CLASS, forwards, without `u`): consume all it can, then give
//                           back one character per backtrack (entry `[-16 - next, pos, lowest]`)

let $$reProg = null;
let $$reSets = null;
let $$reRegs = 0;
let $$reIcase = false;

function $$reOp(v) {
  $$append($$reProg, v);
}

function $$reEmit(node, dir) {
  const t = node[0];
  if (t === 1) {
    $$reOp(1);
    $$reOp($$reIcase ? $$reCanon(node[1], $$reUnicode) : node[1]);
    $$reOp(dir);
  } else if (t === 2) {
    $$reOp(2);
    $$reOp(dir);
  } else if (t === 3) {
    $$reOp(3);
    $$reOp($$reSets.length >> 1);
    $$append($$reSets, node[1]);
    // (With `v`, the parser already folded the sets.)
    const closed = $$reIcase && !$$reVMode;
    $$append($$reSets, $$reI32(closed ? $$reCaseClose(node[2], $$reUnicode) : node[2]));
    $$reOp(dir);
  } else if (t === 4) {
    const items = node[1];
    const n = items.length;
    for (let i = 0; i < n; i++) {
      $$reEmit(items[dir === 0 ? i : n - 1 - i], dir);
    }
  } else if (t === 5) {
    const alts = node[1];
    const jumps = [];
    for (let i = 0; i < alts.length; i++) {
      if (i + 1 < alts.length) {
        $$reOp(4);
        const split = $$reProg.length;
        $$reOp(0);
        $$reEmit(alts[i], dir);
        $$reOp(5);
        $$append(jumps, $$reProg.length);
        $$reOp(0);
        $$reProg[split] = $$reProg.length;
      } else {
        $$reEmit(alts[i], dir);
      }
    }
    for (let i = 0; i < jumps.length; i++) {
      $$reProg[jumps[i]] = $$reProg.length;
    }
  } else if (t === 6) {
    const g = node[1];
    $$reOp(6);
    $$reOp(dir === 0 ? 2 * g : 2 * g + 1);
    $$reEmit(node[2], dir);
    $$reOp(6);
    $$reOp(dir === 0 ? 2 * g + 1 : 2 * g);
  } else if (t === 7) {
    $$reEmitRepeat(node, dir);
  } else if (t === 8 || t === 9) {
    $$reOp(t);
  } else if (t === 10) {
    $$reOp(10);
    $$reOp(node[1] ? 1 : 0);
  } else if (t === 11) {
    $$reOp(11);
    $$reOp($$reSets.length);
    $$append($$reSets, node[1]);
    $$append($$reSets, null);
    $$reOp(dir);
  } else if (t === 12) {
    $$reOp(12);
    $$reOp(node[2] ? 1 : 0);
    const end = $$reProg.length;
    $$reOp(0);
    $$reEmit(node[3], node[1] ? 0 : 1);
    $$reOp(13);
    $$reProg[end] = $$reProg.length;
  }
}

// With `i`: the ranges plus the canonical form of each of their characters, so a character
// matches when its canonical form is in the result (CharacterSetMatcher: some member canonicalizes
// like it; canonical forms are their own canonical forms).
function $$reCaseClose(r, unicode) {
  const set = $$reI32(r);
  const out = [];
  for (let i = 0; i < r.length; i++) {
    $$append(out, r[i]);
  }
  const runs = $$reCaseRuns(unicode);
  const n = __intrinsic_i32_len(runs);
  for (let j = 0; j < n; j += 4) {
    const start = __intrinsic_i32_get(runs, j);
    const count = __intrinsic_i32_get(runs, j + 1);
    const delta = __intrinsic_i32_get(runs, j + 2);
    const stride = __intrinsic_i32_get(runs, j + 3);
    for (let k = 0; k < count; k++) {
      const c = start + k * stride;
      if ($$reInRanges(set, c)) {
        $$append(out, c + delta);
        $$append(out, c + delta);
      }
    }
  }
  return $$reNormalize(out);
}

function $$reEmitRepeat(node, dir) {
  const min = node[1];
  const max = node[2];
  const body = node[4];
  if (max === 0) {
    return;
  }
  if (min === 1 && max === 1) {
    $$reEmit(body, dir);
    return;
  }
  const t = body[0];
  if (node[3] && dir === 0 && !$$reUnicode && (t === 1 || t === 2 || t === 3)) {
    $$reOp(18);
    $$reOp(min);
    $$reOp(max);
    const next = $$reProg.length;
    $$reOp(0);
    $$reEmit(body, dir);
    $$reProg[next] = $$reProg.length;
    return;
  }
  const reg = $$reRegs;
  $$reRegs += 2;
  $$reOp(14);
  $$reOp(reg);
  const loop = $$reProg.length;
  $$reOp(15);
  $$reOp(reg);
  $$reOp(min);
  $$reOp(max);
  $$reOp(node[3] ? 1 : 0);
  const exit = $$reProg.length;
  $$reOp(0);
  $$reOp(16);
  $$reOp(reg);
  if (node[6] > node[5]) {
    $$reOp(7);
    $$reOp(2 * node[5] + 2);
    $$reOp(2 * node[6] + 2);
  }
  $$reEmit(body, dir);
  $$reOp(17);
  $$reOp(reg);
  $$reOp(min);
  $$reOp(loop);
  $$reProg[exit] = $$reProg.length;
}

// ---- flags and compiled code ------------------------------------------------------------------

// Flag text → bits, or -1 if invalid (unknown, repeated, or both `u` and `v`).
function $$reFlagBits(f) {
  let bits = 0;
  for (let i = 0; i < f.length; i++) {
    const c = __intrinsic_unit(f, i);
    let b = 0;
    if (c === 100) {
      b = 1;
    } else if (c === 103) {
      b = 2;
    } else if (c === 105) {
      b = 4;
    } else if (c === 109) {
      b = 8;
    } else if (c === 115) {
      b = 16;
    } else if (c === 117) {
      b = 32;
    } else if (c === 118) {
      b = 64;
    } else if (c === 121) {
      b = 128;
    }
    if (b === 0 || (bits & b) !== 0) {
      return -1;
    }
    bits = bits | b;
  }
  if ((bits & 96) === 96) {
    return -1;
  }
  return bits;
}

let $$reCache = null;
let $$reCacheSize = 0;

// A JS array of integers as an `$i32s` array.
function $$reI32(a) {
  const n = a.length;
  const b = __intrinsic_i32_new(n);
  for (let i = 0; i < n; i++) {
    __intrinsic_i32_set(b, i, +a[i]);
  }
  return b;
}

// The compiled code of a pattern with valid flags (cached).
function $$reCompile(src, flagText, bits) {
  const key = flagText + "/" + src;
  if ($$reCache === null) {
    $$reCache = Object.create(null);
  }
  const hit = $$reCache[key];
  if (hit !== undefined) {
    return hit;
  }
  $$reIcase = (bits & 4) !== 0;
  $$reVMode = (bits & 64) !== 0;
  const ast = $$reParse(src, flagText, (bits & 96) !== 0);
  $$reProg = [];
  $$reSets = [];
  $$reRegs = 0;
  $$reIcase = (bits & 4) !== 0;
  $$reEmit(ast, 0);
  $$reOp(0);
  const names = [];
  for (let i = 0; i < $$reNames.length; i++) {
    $$append(names, [$$reNames[i][0], $$reNames[i][1]]);
  }
  let f = "";
  const all = "dgimsuvy";
  for (let i = 0; i < 8; i++) {
    if ((bits & (1 << i)) !== 0) {
      f = f + __intrinsic_sub(all, i, i + 1);
    }
  }
  const code = [$$reI32($$reProg), $$reSets, $$reGroups + 1, names, bits, $$reRegs,
    $$reEscapeSource(src), f, __intrinsic_i32_new(2 * ($$reGroups + 1)), __intrinsic_i32_new($$reRegs)];
  $$reProg = null;
  $$reSets = null;
  $$reNames = null;
  $$reRefs = null;
  $$rePath = null;
  if ($$reCacheSize >= 1000) {
    $$reCache = Object.create(null);
    $$reCacheSize = 0;
  }
  $$reCache[key] = code;
  $$reCacheSize++;
  return code;
}

// A new RegExp (RegExpInitialize): SyntaxError for invalid flags or patterns.
function $$reCreate(src, flagText) {
  const bits = $$reFlagBits(flagText);
  if (bits < 0) {
    throw new SyntaxError("Invalid flags supplied to RegExp constructor '" + flagText + "'");
  }
  const code = $$reCompile(src, flagText, bits);
  return __intrinsic_regexp_new([code[6], code[7], code]);
}

// A regex literal (the lowering calls it; oxc checked the pattern).
function $$RegExpCreate(src, flagText) {
  return $$reCreate(src, flagText);
}

// `RegExp(pattern, flags)` and `new RegExp(pattern, flags)`.
function $$RegExp(pattern, flags) {
  const data = __intrinsic_regexp_data(pattern);
  let p = "";
  let f = "";
  if (data) {
    p = data[0];
    f = flags === undefined ? data[1] : __intrinsic_to_string(flags);
  } else {
    p = pattern === undefined ? "" : __intrinsic_to_string(pattern);
    f = flags === undefined ? "" : __intrinsic_to_string(flags);
  }
  return $$reCreate(p, f);
}

// ---- matching ---------------------------------------------------------------------------------

// Canonicalize (ES2025 22.2.7.3): without `u`, toUpperCase of one code unit unless the result
// isn't one code unit or maps a non-ASCII unit to ASCII; with `u`, simple case folding. Both from
// unicode-case.js's runs (generated from node's Unicode data).
function $$reCanon(c, unicode) {
  if (c < 128) {
    if (unicode) {
      return c >= 65 && c <= 90 ? c + 32 : c;
    }
    return c >= 97 && c <= 122 ? c - 32 : c;
  }
  const runs = $$reCaseRuns(unicode);
  // The last run starting at or before `c`.
  let lo = 0;
  let hi = __intrinsic_i32_len(runs) >> 2;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (__intrinsic_i32_get(runs, 4 * mid) <= c) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  if (lo === 0) {
    return c;
  }
  const j = 4 * (lo - 1);
  const d = c - __intrinsic_i32_get(runs, j);
  const stride = __intrinsic_i32_get(runs, j + 3);
  if (d % stride !== 0 || d / stride >= __intrinsic_i32_get(runs, j + 1)) {
    return c;
  }
  return c + __intrinsic_i32_get(runs, j + 2);
}

let $$reRunsNonUnicode = null;
let $$reRunsUnicode = null;

// The case runs (`[start, count, delta, stride]` as `$i32s`), decoded on first use.
function $$reCaseRuns(unicode) {
  if (unicode) {
    if ($$reRunsUnicode === null) {
      $$reRunsUnicode = $$reDecode($$reCaseRunsUnicode());
    }
    return $$reRunsUnicode;
  }
  if ($$reRunsNonUnicode === null) {
    $$reRunsNonUnicode = $$reDecode($$reCaseRunsNonUnicode());
  }
  return $$reRunsNonUnicode;
}

// unicode-case.js's (and `\p{…}` tables') encoding: zigzag numbers in base-0x4000 code units,
// continuation units (0x4000–0x7FFF) first.
function $$reDecode(s) {
  const out = [];
  let v = 0;
  for (let i = 0; i < s.length; i++) {
    const u = __intrinsic_unit(s, i);
    v = v * 16384 + (u & 16383);
    if (u < 16384) {
      $$append(out, v % 2 === 1 ? -(v + 1) / 2 : v / 2);
      v = 0;
    }
  }
  return $$reI32(out);
}

function $$reInRanges(r, c) {
  let lo = 0;
  let hi = __intrinsic_i32_len(r) >> 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (c < __intrinsic_i32_get(r, 2 * mid)) {
      hi = mid;
    } else if (c > __intrinsic_i32_get(r, 2 * mid + 1)) {
      lo = mid + 1;
    } else {
      return true;
    }
  }
  return false;
}



function $$reIsWord(c) {
  return (c >= 97 && c <= 122) || (c >= 65 && c <= 90) || (c >= 48 && c <= 57) || c === 95;
}

function $$reIsLineTerminator(c) {
  return c === 10 || c === 13 || c === 0x2028 || c === 0x2029;
}

// Read the character after (dir 0) or before (dir 1) `pos`; sets `$$reWidth` (0: none).
let $$reWidth = 0;

function $$reRead(s, pos, len, dir, unicode) {
  if (dir === 0) {
    if (pos >= len) {
      $$reWidth = 0;
      return -1;
    }
    const c = __intrinsic_unit(s, pos);
    if (unicode && $$reIsLead(c) && pos + 1 < len) {
      const d = __intrinsic_unit(s, pos + 1);
      if ($$reIsTrail(d)) {
        $$reWidth = 2;
        return (c - 0xd800) * 0x400 + (d - 0xdc00) + 0x10000;
      }
    }
    $$reWidth = 1;
    return c;
  }
  if (pos <= 0) {
    $$reWidth = 0;
    return -1;
  }
  const c = __intrinsic_unit(s, pos - 1);
  if (unicode && $$reIsTrail(c) && pos >= 2) {
    const d = __intrinsic_unit(s, pos - 2);
    if ($$reIsLead(d)) {
      $$reWidth = 2;
      return (d - 0xd800) * 0x400 + (c - 0xdc00) + 0x10000;
    }
  }
  $$reWidth = 1;
  return c;
}

// The backtrack stack (shared: matching never runs user code, so it is never reentered).
let $$reStack = null;

// A copy of an `$i32s` array with twice the room.
function $$reGrow(a) {
  const n = __intrinsic_i32_len(a);
  const b = __intrinsic_i32_new(2 * n);
  for (let i = 0; i < n; i++) {
    __intrinsic_i32_set(b, i, __intrinsic_i32_get(a, i));
  }
  return b;
}

// Run `code` on `s` from `start`: the end position (captures in `code[8]`), or -1.
function $$reRun(code, s, start) {
  const prog = code[0];
  const sets = code[1];
  const bits = +code[4];
  const caps = code[8];
  const regs = code[9];
  // Flags as numbers, not booleans (boolean locals are boxed, F14).
  const icase = bits & 4;
  const multiline = bits & 8;
  const dotAll = bits & 16;
  const unicode = bits & 96;
  const len = s.length;
  const ncaps = __intrinsic_i32_len(caps);
  for (let i = 0; i < ncaps; i++) {
    __intrinsic_i32_set(caps, i, -1);
  }
  __intrinsic_i32_set(caps, 0, start);
  let stack = $$reStack;
  if (stack === null) {
    stack = __intrinsic_i32_new(768);
    $$reStack = stack;
  }
  let cap = __intrinsic_i32_len(stack);
  let pc = 0;
  let pos = start;
  let sp = 0;
  while (true) {
    const op = __intrinsic_i32_get(prog, pc);
    let ok = 1;
    // Room for the (at most one) entry this instruction pushes (RESET grows on its own).
    if (sp + 3 > cap) {
      stack = $$reGrow(stack);
      $$reStack = stack;
      cap = __intrinsic_i32_len(stack);
    }
    if (op === 1) {
      const dir = __intrinsic_i32_get(prog, pc + 2);
      let c = 0;
      let w = 1;
      if (unicode === 0) {
        if (dir === 0 ? pos >= len : pos <= 0) {
          w = 0;
        } else {
          c = __intrinsic_unit(s, dir === 0 ? pos : pos - 1);
        }
      } else {
        c = $$reRead(s, pos, len, dir, unicode !== 0);
        w = $$reWidth;
      }
      if (w === 0) {
        ok = 0;
      } else {
        if (icase !== 0) {
          c = c < 128 && unicode === 0 ? (c >= 97 && c <= 122 ? c - 32 : c) : $$reCanon(c, unicode !== 0);
        }
        if (c !== __intrinsic_i32_get(prog, pc + 1)) {
          ok = 0;
        } else {
          pos = dir === 0 ? pos + w : pos - w;
          pc += 3;
        }
      }
    } else if (op === 18) {
      const min = __intrinsic_i32_get(prog, pc + 1);
      const max = __intrinsic_i32_get(prog, pc + 2);
      const next = __intrinsic_i32_get(prog, pc + 3);
      const apc = pc + 4;
      let limit = len - pos;
      if (max >= 0 && max < limit) {
        limit = max;
      }
      let n = 0;
      while (n < limit && $$reOne(prog, apc, sets, __intrinsic_unit(s, pos + n), icase, dotAll) !== 0) {
        n++;
      }
      if (n < min) {
        ok = 0;
      } else {
        if (n > min) {
          __intrinsic_i32_set(stack, sp, -16 - next);
          __intrinsic_i32_set(stack, sp + 1, pos + n);
          __intrinsic_i32_set(stack, sp + 2, pos + min);
          sp += 3;
        }
        pos += n;
        pc = next;
      }
    } else if (op === 3) {
      const dir = __intrinsic_i32_get(prog, pc + 2);
      const c = $$reRead(s, pos, len, dir, unicode !== 0);
      if ($$reWidth === 0) {
        ok = 0;
      } else {
        const k = __intrinsic_i32_get(prog, pc + 1);
        const r = sets[2 * k + 1];
        const found = $$reInRanges(r, icase !== 0 ? $$reCanon(c, unicode !== 0) : c);
        if (found === sets[2 * k]) {
          ok = 0;
        } else {
          pos = dir === 0 ? pos + $$reWidth : pos - $$reWidth;
          pc += 3;
        }
      }
    } else if (op === 2) {
      const dir = __intrinsic_i32_get(prog, pc + 1);
      const c = $$reRead(s, pos, len, dir, unicode !== 0);
      if ($$reWidth === 0 || (dotAll === 0 && $$reIsLineTerminator(c))) {
        ok = 0;
      } else {
        pos = dir === 0 ? pos + $$reWidth : pos - $$reWidth;
        pc += 2;
      }
    } else if (op === 4) {
      __intrinsic_i32_set(stack, sp, __intrinsic_i32_get(prog, pc + 1));
      __intrinsic_i32_set(stack, sp + 1, pos);
      sp += 3;
      pc += 2;
    } else if (op === 5) {
      pc = __intrinsic_i32_get(prog, pc + 1);
    } else if (op === 6) {
      const slot = __intrinsic_i32_get(prog, pc + 1);
      __intrinsic_i32_set(stack, sp, -1);
      __intrinsic_i32_set(stack, sp + 1, slot);
      __intrinsic_i32_set(stack, sp + 2, __intrinsic_i32_get(caps, slot));
      sp += 3;
      __intrinsic_i32_set(caps, slot, pos);
      pc += 2;
    } else if (op === 15) {
      const reg = __intrinsic_i32_get(prog, pc + 1);
      const count = __intrinsic_i32_get(regs, reg);
      const min = __intrinsic_i32_get(prog, pc + 2);
      const max = __intrinsic_i32_get(prog, pc + 3);
      if (count < min) {
        pc += 6;
      } else if (max >= 0 && count >= max) {
        pc = __intrinsic_i32_get(prog, pc + 5);
      } else if (__intrinsic_i32_get(prog, pc + 4) === 1) {
        __intrinsic_i32_set(stack, sp, __intrinsic_i32_get(prog, pc + 5));
        __intrinsic_i32_set(stack, sp + 1, pos);
        sp += 3;
        pc += 6;
      } else {
        __intrinsic_i32_set(stack, sp, pc + 6);
        __intrinsic_i32_set(stack, sp + 1, pos);
        sp += 3;
        pc = __intrinsic_i32_get(prog, pc + 5);
      }
    } else if (op === 16) {
      const reg = __intrinsic_i32_get(prog, pc + 1) + 1;
      __intrinsic_i32_set(stack, sp, -2);
      __intrinsic_i32_set(stack, sp + 1, reg);
      __intrinsic_i32_set(stack, sp + 2, __intrinsic_i32_get(regs, reg));
      sp += 3;
      __intrinsic_i32_set(regs, reg, pos);
      pc += 2;
    } else if (op === 17) {
      const reg = __intrinsic_i32_get(prog, pc + 1);
      const count = __intrinsic_i32_get(regs, reg);
      if (count >= __intrinsic_i32_get(prog, pc + 2) && pos === __intrinsic_i32_get(regs, reg + 1)) {
        ok = 0;
      } else {
        __intrinsic_i32_set(stack, sp, -2);
        __intrinsic_i32_set(stack, sp + 1, reg);
        __intrinsic_i32_set(stack, sp + 2, count);
        sp += 3;
        __intrinsic_i32_set(regs, reg, count + 1);
        pc = __intrinsic_i32_get(prog, pc + 3);
      }
    } else if (op === 14) {
      const reg = __intrinsic_i32_get(prog, pc + 1);
      __intrinsic_i32_set(stack, sp, -2);
      __intrinsic_i32_set(stack, sp + 1, reg);
      __intrinsic_i32_set(stack, sp + 2, __intrinsic_i32_get(regs, reg));
      sp += 3;
      __intrinsic_i32_set(regs, reg, 0);
      pc += 2;
    } else if (op === 7) {
      const to = __intrinsic_i32_get(prog, pc + 2);
      for (let slot = __intrinsic_i32_get(prog, pc + 1); slot < to; slot++) {
        const old = __intrinsic_i32_get(caps, slot);
        if (old !== -1) {
          if (sp + 3 > cap) {
            stack = $$reGrow(stack);
            $$reStack = stack;
            cap = __intrinsic_i32_len(stack);
          }
          __intrinsic_i32_set(stack, sp, -1);
          __intrinsic_i32_set(stack, sp + 1, slot);
          __intrinsic_i32_set(stack, sp + 2, old);
          sp += 3;
          __intrinsic_i32_set(caps, slot, -1);
        }
      }
      pc += 3;
    } else if (op === 0) {
      __intrinsic_i32_set(caps, 1, pos);
      return pos;
    } else if (op === 8) {
      if (pos === 0 || (multiline !== 0 && $$reIsLineTerminator(__intrinsic_unit(s, pos - 1)))) {
        pc += 1;
      } else {
        ok = 0;
      }
    } else if (op === 9) {
      if (pos === len || (multiline !== 0 && $$reIsLineTerminator(__intrinsic_unit(s, pos)))) {
        pc += 1;
      } else {
        ok = 0;
      }
    } else if (op === 10) {
      const a = pos > 0 && $$reIsWordAt(s, pos - 1, icase !== 0, unicode !== 0);
      const b = pos < len && $$reIsWordAt(s, pos, icase !== 0, unicode !== 0);
      if ((a !== b) === (__intrinsic_i32_get(prog, pc + 1) === 0)) {
        pc += 2;
      } else {
        ok = 0;
      }
    } else if (op === 11) {
      const groups = sets[__intrinsic_i32_get(prog, pc + 1)];
      const end = $$reBackref(s, pos, len, caps, groups, __intrinsic_i32_get(prog, pc + 2),
        icase !== 0, unicode !== 0);
      if (end < 0) {
        ok = 0;
      } else {
        pos = end;
        pc += 3;
      }
    } else if (op === 12) {
      __intrinsic_i32_set(stack, sp, __intrinsic_i32_get(prog, pc + 1) === 1 ? -4 : -3);
      __intrinsic_i32_set(stack, sp + 1, pos);
      __intrinsic_i32_set(stack, sp + 2, __intrinsic_i32_get(prog, pc + 2));
      sp += 3;
      pc += 3;
    } else if (op === 13) {
      // The body matched: find the barrier.
      let b = sp - 3;
      while (__intrinsic_i32_get(stack, b) !== -3 && __intrinsic_i32_get(stack, b) !== -4) {
        b -= 3;
      }
      if (__intrinsic_i32_get(stack, b) === -3) {
        // Positive: drop the choices above it, keep the undo entries.
        pos = __intrinsic_i32_get(stack, b + 1);
        let j = b;
        for (let e = b + 3; e < sp; e += 3) {
          const k = __intrinsic_i32_get(stack, e);
          if (k === -1 || k === -2) {
            __intrinsic_i32_set(stack, j, k);
            __intrinsic_i32_set(stack, j + 1, __intrinsic_i32_get(stack, e + 1));
            __intrinsic_i32_set(stack, j + 2, __intrinsic_i32_get(stack, e + 2));
            j += 3;
          }
        }
        sp = j;
        pc += 1;
      } else {
        // Negative: undo the body, then fail.
        while (sp > b + 3) {
          sp -= 3;
          const k = __intrinsic_i32_get(stack, sp);
          if (k === -1) {
            __intrinsic_i32_set(caps, __intrinsic_i32_get(stack, sp + 1), __intrinsic_i32_get(stack, sp + 2));
          } else if (k === -2) {
            __intrinsic_i32_set(regs, __intrinsic_i32_get(stack, sp + 1), __intrinsic_i32_get(stack, sp + 2));
          }
        }
        sp = b;
        ok = 0;
      }
    }
    if (ok === 0) {
      // Backtrack.
      while (true) {
        if (sp === 0) {
          return -1;
        }
        sp -= 3;
        const k = __intrinsic_i32_get(stack, sp);
        if (k >= 0) {
          pc = k;
          pos = __intrinsic_i32_get(stack, sp + 1);
          break;
        }
        if (k <= -16) {
          // A STAR loop gives back one character.
          pos = __intrinsic_i32_get(stack, sp + 1) - 1;
          if (pos > __intrinsic_i32_get(stack, sp + 2)) {
            __intrinsic_i32_set(stack, sp + 1, pos);
            sp += 3;
          }
          pc = -16 - k;
          break;
        }
        if (k === -1) {
          __intrinsic_i32_set(caps, __intrinsic_i32_get(stack, sp + 1), __intrinsic_i32_get(stack, sp + 2));
        } else if (k === -2) {
          __intrinsic_i32_set(regs, __intrinsic_i32_get(stack, sp + 1), __intrinsic_i32_get(stack, sp + 2));
        } else if (k === -4) {
          // A negative lookaround's body failed: it succeeds.
          pos = __intrinsic_i32_get(stack, sp + 1);
          pc = __intrinsic_i32_get(stack, sp + 2);
          break;
        }
      }
    }
  }
}

// Does the one-character instruction at `apc` (CHAR, ANY or CLASS; no `u`) match the unit `c`
// (1 or 0; `icase`, `dotAll`: numbers)?
function $$reOne(prog, apc, sets, c, icase, dotAll) {
  const op = __intrinsic_i32_get(prog, apc);
  if (op === 1) {
    if (icase !== 0) {
      c = c < 128 ? (c >= 97 && c <= 122 ? c - 32 : c) : $$reCanon(c, false);
    }
    return c === __intrinsic_i32_get(prog, apc + 1) ? 1 : 0;
  }
  if (op === 2) {
    return dotAll !== 0 || !$$reIsLineTerminator(c) ? 1 : 0;
  }
  const k = __intrinsic_i32_get(prog, apc + 1);
  const r = sets[2 * k + 1];
  const found = $$reInRanges(r, icase !== 0 ? $$reCanon(c, false) : c);
  return found !== sets[2 * k] ? 1 : 0;
}

function $$reIsWordAt(s, i, icase, unicode) {
  const c = __intrinsic_unit(s, i);
  if ($$reIsWord(c)) {
    return true;
  }
  // With `i` and `u`, ſ and K (Kelvin) fold to word characters.
  return icase && unicode && (c === 0x17f || c === 0x212a);
}

// A backreference at `pos`: the new position, or -1. `groups`: the first that matched counts.
function $$reBackref(s, pos, len, caps, groups, dir, icase, unicode) {
  let from = -1;
  let to = -1;
  for (let i = 0; i < groups.length; i++) {
    const g = +groups[i];
    const a = __intrinsic_i32_get(caps, 2 * g);
    const b = __intrinsic_i32_get(caps, 2 * g + 1);
    if (a >= 0 && b >= 0) {
      from = a;
      to = b;
      break;
    }
  }
  if (from < 0) {
    return pos;
  }
  const n = to - from;
  const at = dir === 0 ? pos : pos - n;
  if (at < 0 || at + n > len) {
    return -1;
  }
  for (let k = 0; k < n; k++) {
    const a = __intrinsic_unit(s, from + k);
    const b = __intrinsic_unit(s, at + k);
    if (a !== b) {
      if (!icase || $$reCanon(a, unicode) !== $$reCanon(b, unicode)) {
        return -1;
      }
    }
  }
  return dir === 0 ? pos + n : pos - n;
}

// ---- exec ---------------------------------------------------------------------------------------

function $$reToLength(v) {
  const n = $$toInteger(v);
  if (n <= 0) {
    return 0;
  }
  return n > 9007199254740991 ? 9007199254740991 : n;
}

// The data of `this` for a method named `name`, or a TypeError.
function $$reThis(r, name) {
  const data = __intrinsic_regexp_data(r);
  if (!data) {
    throw new TypeError("Method RegExp.prototype." + name + " called on incompatible receiver " +
      $$reWhat(r));
  }
  return data;
}

function $$reWhat(v) {
  if (typeof v === "string") {
    return '"' + v + '"';
  }
  if ((typeof v === "object" && v !== null) || typeof v === "function") {
    return __intrinsic_object_desc(v);
  }
  return __intrinsic_to_string(v);
}

// RegExpBuiltinExec up to the match: the end of the match (the captures in `data[2][8]`) or -1;
// `lastIndex` updated.
function $$reSearch(r, data, s) {
  const code = data[2];
  const bits = +code[4];
  const global = (bits & 130) !== 0;
  const sticky = (bits & 128) !== 0;
  const unicode = (bits & 96) !== 0;
  let lastIndex = $$reToLength(r.lastIndex);
  if (!global) {
    lastIndex = 0;
  }
  const len = s.length;
  // In Unicode mode V8 starts at the lead of a surrogate pair that `lastIndex` points into.
  if (unicode && lastIndex > 0 && lastIndex < len && $$reIsTrail(__intrinsic_unit(s, lastIndex)) &&
    $$reIsLead(__intrinsic_unit(s, lastIndex - 1))) {
    lastIndex--;
  }
  let end = -1;
  while (lastIndex <= len) {
    end = $$reRun(code, s, lastIndex);
    if (end >= 0 || sticky) {
      break;
    }
    if (unicode && lastIndex + 1 < len && $$reIsLead(__intrinsic_unit(s, lastIndex)) &&
      $$reIsTrail(__intrinsic_unit(s, lastIndex + 1))) {
      lastIndex += 2;
    } else {
      lastIndex++;
    }
  }
  if (end < 0) {
    if (global) {
      r.lastIndex = 0;
    }
    return -1;
  }
  if (global) {
    r.lastIndex = end;
  }
  return end;
}

// RegExpBuiltinExec: the match array or null.
function $$reExec(r, data, s) {
  if ($$reSearch(r, data, s) < 0) {
    return null;
  }
  const code = data[2];
  const bits = +code[4];
  const caps = code[8];
  const a = [];
  const n = code[2];
  for (let g = 0; g < n; g++) {
    const from = __intrinsic_i32_get(caps, 2 * g);
    const to = __intrinsic_i32_get(caps, 2 * g + 1);
    $$append(a, from >= 0 && to >= 0 ? __intrinsic_sub(s, from, to) : undefined);
  }
  __intrinsic_define(a, "index", __intrinsic_i32_get(caps, 0));
  __intrinsic_define(a, "input", s);
  const names = code[3];
  let groups = undefined;
  if (names.length > 0) {
    groups = Object.create(null);
    for (let i = 0; i < names.length; i++) {
      const name = names[i][0];
      const v = a[names[i][1]];
      if (v !== undefined || !(name in groups)) {
        __intrinsic_define(groups, name, v);
      }
    }
  }
  __intrinsic_define(a, "groups", groups);
  if ((bits & 1) !== 0) {
    const indices = [];
    for (let g = 0; g < n; g++) {
      const from = __intrinsic_i32_get(caps, 2 * g);
      const to = __intrinsic_i32_get(caps, 2 * g + 1);
      $$append(indices, from >= 0 && to >= 0 ? [from, to] : undefined);
    }
    let igroups = undefined;
    if (names.length > 0) {
      igroups = Object.create(null);
      for (let i = 0; i < names.length; i++) {
        const name = names[i][0];
        const v = indices[names[i][1]];
        if (v !== undefined || !(name in igroups)) {
          __intrinsic_define(igroups, name, v);
        }
      }
    }
    __intrinsic_define(indices, "groups", igroups);
    __intrinsic_define(a, "indices", indices);
  }
  return a;
}

// RegExpExec: `r.exec` if it's a function (the built-in one, or a user's), else the built-in.
function $$reExecAny(r, s) {
  const exec = r.exec;
  if (__intrinsic_regexp_builtin_exec(exec)) {
    return $$reExec(r, $$reThis(r, "exec"), s);
  }
  if (typeof exec === "function") {
    const result = exec.call(r, s);
    if (result !== null && typeof result !== "object") {
      throw new TypeError("RegExp exec method returned something other than an Object or null");
    }
    return result;
  }
  return $$reExec(r, $$reThis(r, "exec"), s);
}

// ---- RegExp.prototype ---------------------------------------------------------------------------

function $$RegExp_prototype_exec(string) {
  const data = $$reThis(this, "exec");
  return $$reExec(this, data, __intrinsic_to_string(string));
}

function $$RegExp_prototype_test(string) {
  if (!((typeof this === "object" && this !== null) || typeof this === "function")) {
    throw new TypeError("Method RegExp.prototype.test called on incompatible receiver " +
      $$reWhat(this));
  }
  const s = __intrinsic_to_string(string);
  if (__intrinsic_regexp_builtin_exec(this.exec)) {
    // The built-in exec: no match array needed.
    return $$reSearch(this, $$reThis(this, "exec"), s) >= 0;
  }
  return $$reExecAny(this, s) !== null;
}

function $$RegExp_prototype_toString() {
  if (!((typeof this === "object" && this !== null) || typeof this === "function")) {
    throw new TypeError("Method RegExp.prototype.toString called on incompatible receiver " +
      $$reWhat(this));
  }
  return "/" + __intrinsic_to_string(this.source) + "/" + __intrinsic_to_string(this.flags);
}

// EscapeRegExpPattern as V8 does it: `/` outside a class and line terminators escaped. (The
// result is an equivalent pattern: `RegExp(re)` compiles it again.)
function $$reEscapeSource(src) {
  if (src.length === 0) {
    return "(?:)";
  }
  let out = "";
  let from = 0;
  let inClass = false;
  for (let i = 0; i < src.length; i++) {
    const c = __intrinsic_unit(src, i);
    let rep = "";
    if (c === 92) {
      if (i + 1 < src.length) {
        const d = __intrinsic_unit(src, i + 1);
        // An escaped line terminator loses its backslash's partner, not the backslash.
        if (!$$reIsLineTerminator(d)) {
          i++;
        }
      }
      continue;
    }
    if (c === 47 && !inClass) {
      rep = "\\/";
    } else if (c === 91) {
      inClass = true;
    } else if (c === 93) {
      inClass = false;
    } else if (c === 10) {
      rep = "\\n";
    } else if (c === 13) {
      rep = "\\r";
    } else if (c === 0x2028) {
      rep = "\\u2028";
    } else if (c === 0x2029) {
      rep = "\\u2029";
    }
    if (rep !== "") {
      out = out + __intrinsic_sub(src, from, i) + rep;
      from = i + 1;
    }
  }
  return out + __intrinsic_sub(src, from, src.length);
}

function $$RegExp_prototype_source() {
  const data = __intrinsic_regexp_data(this);
  if (data === null) {
    return "(?:)";
  }
  if (!data) {
    throw new TypeError("RegExp.prototype.source getter called on non-RegExp object");
  }
  return data[0];
}

function $$RegExp_prototype_flags() {
  const r = this;
  if (!((typeof r === "object" && r !== null) || typeof r === "function")) {
    throw new TypeError("RegExp.prototype.flags getter called on non-object " + $$reWhat(r));
  }
  let f = "";
  if (r.hasIndices) {
    f = f + "d";
  }
  if (r.global) {
    f = f + "g";
  }
  if (r.ignoreCase) {
    f = f + "i";
  }
  if (r.multiline) {
    f = f + "m";
  }
  if (r.dotAll) {
    f = f + "s";
  }
  if (r.unicode) {
    f = f + "u";
  }
  if (r.unicodeSets) {
    f = f + "v";
  }
  if (r.sticky) {
    f = f + "y";
  }
  return f;
}

// The flag getters.
function $$reFlag(r, bit, name) {
  const data = __intrinsic_regexp_data(r);
  if (data === null) {
    return undefined;
  }
  if (!data) {
    throw new TypeError("RegExp.prototype." + name + " getter called on non-RegExp object");
  }
  return (data[2][4] & bit) !== 0;
}

function $$RegExp_prototype_hasIndices() {
  return $$reFlag(this, 1, "hasIndices");
}

function $$RegExp_prototype_global() {
  return $$reFlag(this, 2, "global");
}

function $$RegExp_prototype_ignoreCase() {
  return $$reFlag(this, 4, "ignoreCase");
}

function $$RegExp_prototype_multiline() {
  return $$reFlag(this, 8, "multiline");
}

function $$RegExp_prototype_dotAll() {
  return $$reFlag(this, 16, "dotAll");
}

function $$RegExp_prototype_unicode() {
  return $$reFlag(this, 32, "unicode");
}

function $$RegExp_prototype_unicodeSets() {
  return $$reFlag(this, 64, "unicodeSets");
}

function $$RegExp_prototype_sticky() {
  return $$reFlag(this, 128, "sticky");
}

// ---- RegExp.prototype[Symbol.*] and the string methods that use them (slice 2) ----------------
// `$$RegExp_prototype_$match` is `RegExp.prototype[Symbol.match]` (the backend installs a `$name`
// key as `Symbol.name`). They're linked only when a RegExp can exist and the code names the
// method (`crates/lower/src/builtins.rs`), so `"a,b".split(",")` doesn't link the engine.

// `this` of a `RegExp.prototype[Symbol.x]` method: an object.
function $$reSymThis(r, name) {
  if (!((typeof r === "object" && r !== null) || typeof r === "function")) {
    throw new TypeError("Method RegExp.prototype.@@" + name + " called on incompatible receiver " +
      $$reWhat(r));
  }
}

// AdvanceStringIndex.
function $$reAdvance(s, i, unicode) {
  if (unicode && i + 1 < s.length && $$reIsLead(__intrinsic_unit(s, i)) &&
    $$reIsTrail(__intrinsic_unit(s, i + 1))) {
    return i + 2;
  }
  return i + 1;
}

// Does the flags string contain the code unit `c`?
function $$reHasFlag(flags, c) {
  for (let i = 0; i < flags.length; i++) {
    if (__intrinsic_unit(flags, i) === c) {
      return true;
    }
  }
  return false;
}

// IsRegExp.
function $$isRegExp(v) {
  if (!((typeof v === "object" && v !== null) || typeof v === "function")) {
    return false;
  }
  const m = v[Symbol.match];
  if (m !== undefined) {
    return !!m;
  }
  return !!__intrinsic_regexp_data(v);
}

// `matchAll`/`replaceAll`: a RegExp argument must have the `g` flag. (`flags` is read through a
// variable key so that string `replaceAll` doesn't link the getters; a RegExp's creation does.)
function $$reRequireGlobal(v, name) {
  if (!$$isRegExp(v)) {
    return;
  }
  const key = "flags";
  const flags = v[key];
  if (flags === undefined || flags === null) {
    throw new TypeError("String.prototype." + name + " called on null or undefined");
  }
  if (!$$reHasFlag(__intrinsic_to_string(flags), 103)) {
    throw new TypeError("String.prototype." + name + " called with a non-global RegExp argument");
  }
}

function $$RegExp_prototype_$match(string) {
  const rx = this;
  $$reSymThis(rx, "match");
  const s = __intrinsic_to_string(string);
  const flags = __intrinsic_to_string(rx.flags);
  if (!$$reHasFlag(flags, 103)) {
    return $$reExecAny(rx, s);
  }
  const unicode = $$reHasFlag(flags, 117) || $$reHasFlag(flags, 118);
  rx.lastIndex = 0;
  const a = [];
  while (true) {
    const result = $$reExecAny(rx, s);
    if (result === null) {
      return a.length === 0 ? null : a;
    }
    const matched = __intrinsic_to_string(result[0]);
    $$append(a, matched);
    if (matched === "") {
      rx.lastIndex = $$reAdvance(s, $$reToLength(rx.lastIndex), unicode);
    }
  }
}

function $$RegExp_prototype_$search(string) {
  const rx = this;
  $$reSymThis(rx, "search");
  const s = __intrinsic_to_string(string);
  const previous = rx.lastIndex;
  if (!$$reSameValue(previous, 0)) {
    rx.lastIndex = 0;
  }
  const result = $$reExecAny(rx, s);
  if (!$$reSameValue(rx.lastIndex, previous)) {
    rx.lastIndex = previous;
  }
  return result === null ? -1 : result.index;
}

function $$reSameValue(a, b) {
  if (a === b) {
    return a !== 0 || 1 / a === 1 / b;
  }
  return a !== a && b !== b;
}

// GetSubstitution (ES2025 22.1.3.19.1): `$$`, `$&`, `` $` ``, `$'`, `$n`/`$nn`, `$<name>`.
function $$reSubstitute(matched, s, position, captures, named, replacement) {
  const n = replacement.length;
  const m = captures.length;
  let r = "";
  let from = 0;
  for (let i = 0; i + 1 < n; i++) {
    if (__intrinsic_unit(replacement, i) !== 36) {
      continue;
    }
    const c = __intrinsic_unit(replacement, i + 1);
    let add = "";
    let skip = 1;
    if (c === 36) {
      add = "$";
    } else if (c === 38) {
      add = matched;
    } else if (c === 96) {
      add = __intrinsic_sub(s, 0, position);
    } else if (c === 39) {
      const end = position + matched.length;
      add = end < s.length ? __intrinsic_sub(s, end, s.length) : "";
    } else if (c >= 48 && c <= 57) {
      let k = c - 48;
      const d = i + 2 < n ? __intrinsic_unit(replacement, i + 2) : -1;
      if (d >= 48 && d <= 57 && k * 10 + (d - 48) >= 1 && k * 10 + (d - 48) <= m) {
        k = k * 10 + (d - 48);
        skip = 2;
      }
      if (k < 1 || k > m) {
        continue;
      }
      const v = captures[k - 1];
      add = v === undefined ? "" : v;
    } else if (c === 60) {
      if (named === undefined) {
        continue;
      }
      let close = -1;
      for (let j = i + 2; j < n; j++) {
        if (__intrinsic_unit(replacement, j) === 62) {
          close = j;
          break;
        }
      }
      if (close < 0) {
        continue;
      }
      const v = named[__intrinsic_sub(replacement, i + 2, close)];
      add = v === undefined ? "" : __intrinsic_to_string(v);
      skip = close - i;
    } else {
      continue;
    }
    r = r + __intrinsic_sub(replacement, from, i) + add;
    i += skip;
    from = i + 1;
  }
  return r + __intrinsic_sub(replacement, from, n);
}

function $$RegExp_prototype_$replace(string, replaceValue) {
  const rx = this;
  $$reSymThis(rx, "replace");
  const s = __intrinsic_to_string(string);
  const len = s.length;
  const functional = typeof replaceValue === "function";
  let replacement = "";
  if (!functional) {
    replacement = __intrinsic_to_string(replaceValue);
  }
  const flags = __intrinsic_to_string(rx.flags);
  const global = $$reHasFlag(flags, 103);
  let unicode = false;
  if (global) {
    unicode = $$reHasFlag(flags, 117) || $$reHasFlag(flags, 118);
    rx.lastIndex = 0;
  }
  const results = [];
  while (true) {
    const result = $$reExecAny(rx, s);
    if (result === null) {
      break;
    }
    $$append(results, result);
    if (!global) {
      break;
    }
    if (__intrinsic_to_string(result[0]) === "") {
      rx.lastIndex = $$reAdvance(s, $$reToLength(rx.lastIndex), unicode);
    }
  }
  // The pieces, joined once at the end (repeated `+` would be quadratic: strings are flat).
  const out = [];
  let next = 0;
  for (let i = 0; i < results.length; i++) {
    const result = results[i];
    const n = $$reToLength(result.length);
    const matched = __intrinsic_to_string(result[0]);
    let position = $$toInteger(result.index);
    position = position < 0 ? 0 : position > len ? len : position;
    const captures = [];
    for (let k = 1; k < n; k++) {
      const v = result[k];
      $$append(captures, v === undefined ? undefined : __intrinsic_to_string(v));
    }
    let named = result.groups;
    let rep = "";
    if (functional) {
      const args = [matched];
      for (let k = 0; k < captures.length; k++) {
        $$append(args, captures[k]);
      }
      $$append(args, position);
      $$append(args, s);
      if (named !== undefined) {
        $$append(args, named);
      }
      rep = __intrinsic_to_string(__intrinsic_call_list(replaceValue, undefined, args));
    } else {
      if (named !== undefined) {
        named = __intrinsic_to_object(named);
      }
      rep = $$reSubstitute(matched, s, position, captures, named, replacement);
    }
    if (position >= next) {
      $$append(out, __intrinsic_sub(s, next, position));
      $$append(out, rep);
      next = position + matched.length;
    }
  }
  if (next < len) {
    $$append(out, __intrinsic_sub(s, next, len));
  }
  return out.join("");
}

function $$RegExp_prototype_$split(string, limit) {
  const rx = this;
  $$reSymThis(rx, "split");
  const s = __intrinsic_to_string(string);
  const flags = __intrinsic_to_string(rx.flags);
  const unicode = $$reHasFlag(flags, 117) || $$reHasFlag(flags, 118);
  const newFlags = $$reHasFlag(flags, 121) ? flags : flags + "y";
  // (Species: not implemented, the splitter is a RegExp.)
  const splitter = $$RegExp(rx, newFlags);
  const a = [];
  const lim = limit === undefined ? 4294967295 : (+limit) >>> 0;
  if (lim === 0) {
    return a;
  }
  const size = s.length;
  if (size === 0) {
    if ($$reExecAny(splitter, s) === null) {
      $$append(a, s);
    }
    return a;
  }
  const data = __intrinsic_regexp_data(splitter);
  const code = data[2];
  // The built-in exec on the fresh splitter: run the matcher directly (no lastIndex writes or
  // match arrays; the splitter isn't reachable from user code).
  const direct = __intrinsic_regexp_builtin_exec(splitter.exec);
  let p = 0;
  let q = 0;
  while (q < size) {
    let e = -1;
    let z = null;
    if (direct) {
      e = $$reRun(code, s, q);
    } else {
      splitter.lastIndex = q;
      z = $$reExecAny(splitter, s);
      if (z !== null) {
        e = $$reToLength(splitter.lastIndex);
        if (e > size) {
          e = size;
        }
      }
    }
    if (e < 0 || e === p) {
      q = $$reAdvance(s, q, unicode);
      continue;
    }
    $$append(a, __intrinsic_sub(s, p, q));
    if (a.length === lim) {
      return a;
    }
    p = e;
    if (direct) {
      const caps = code[8];
      const groups = +code[2];
      for (let g = 1; g < groups; g++) {
        const from = __intrinsic_i32_get(caps, 2 * g);
        const to = __intrinsic_i32_get(caps, 2 * g + 1);
        $$append(a, from >= 0 && to >= 0 ? __intrinsic_sub(s, from, to) : undefined);
        if (a.length === lim) {
          return a;
        }
      }
    } else {
      const n = $$reToLength(z.length);
      for (let i = 1; i < n; i++) {
        $$append(a, z[i]);
        if (a.length === lim) {
          return a;
        }
      }
    }
    q = p;
  }
  $$append(a, __intrinsic_sub(s, p, size));
  return a;
}

// The iterator of `matchAll` (a generator here; node's is a `RegExp String Iterator`).
function* $$reMatchAllIterator(matcher, s, global, unicode) {
  while (true) {
    const match = $$reExecAny(matcher, s);
    if (match === null) {
      return;
    }
    if (!global) {
      yield match;
      return;
    }
    if (__intrinsic_to_string(match[0]) === "") {
      matcher.lastIndex = $$reAdvance(s, $$reToLength(matcher.lastIndex), unicode);
    }
    yield match;
  }
}

function $$RegExp_prototype_$matchAll(string) {
  const rx = this;
  $$reSymThis(rx, "matchAll");
  const s = __intrinsic_to_string(string);
  const flags = __intrinsic_to_string(rx.flags);
  // (Species: not implemented, the matcher is a RegExp.)
  const matcher = $$RegExp(rx, flags);
  matcher.lastIndex = $$reToLength(rx.lastIndex);
  const global = $$reHasFlag(flags, 103);
  const unicode = $$reHasFlag(flags, 117) || $$reHasFlag(flags, 118);
  return $$reMatchAllIterator(matcher, s, global, unicode);
}

// GetMethod(v, key) for the string methods: undefined/null → undefined; else it must be callable
// (node's message).
function $$reMethod(v, key, what) {
  const m = v[key];
  if (m === undefined || m === null) {
    return undefined;
  }
  if (typeof m !== "function") {
    throw new TypeError("'" + __intrinsic_to_string(m) + "' returned for property '" + what +
      "' of object '" + $$describe(v) + "' is not a function");
  }
  return m;
}

function $$String_prototype_match(regexp) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.match called on null or undefined");
  }
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof regexp === "object" && regexp !== null) || typeof regexp === "function") {
    const matcher = $$reMethod(regexp, Symbol.match, "Symbol(Symbol.match)");
    if (matcher !== undefined) {
      return __intrinsic_call_list(matcher, regexp, [this]);
    }
  }
  const s = __intrinsic_to_string(this);
  const rx = $$reCreate(regexp === undefined ? "" : __intrinsic_to_string(regexp), "");
  return rx[Symbol.match](s);
}

function $$String_prototype_matchAll(regexp) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.matchAll called on null or undefined");
  }
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof regexp === "object" && regexp !== null) || typeof regexp === "function") {
    $$reRequireGlobal(regexp, "matchAll");
    const matcher = $$reMethod(regexp, Symbol.matchAll, "Symbol(Symbol.matchAll)");
    if (matcher !== undefined) {
      return __intrinsic_call_list(matcher, regexp, [this]);
    }
  }
  const s = __intrinsic_to_string(this);
  const rx = $$reCreate(regexp === undefined ? "" : __intrinsic_to_string(regexp), "g");
  return rx[Symbol.matchAll](s);
}

function $$String_prototype_search(regexp) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.search called on null or undefined");
  }
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof regexp === "object" && regexp !== null) || typeof regexp === "function") {
    const searcher = $$reMethod(regexp, Symbol.search, "Symbol(Symbol.search)");
    if (searcher !== undefined) {
      return __intrinsic_call_list(searcher, regexp, [this]);
    }
  }
  const s = __intrinsic_to_string(this);
  const rx = $$reCreate(regexp === undefined ? "" : __intrinsic_to_string(regexp), "");
  return rx[Symbol.search](s);
}
