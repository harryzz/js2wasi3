// Math (T21). Conventions: crates/lower/src/builtins.rs. `$$Math_x` is `Math.x`.

function $$Math_abs(x) {
  return __intrinsic_abs(+x);
}

function $$Math_floor(x) {
  return __intrinsic_floor(+x);
}

function $$Math_ceil(x) {
  return __intrinsic_ceil(+x);
}

function $$Math_trunc(x) {
  return __intrinsic_trunc(+x);
}

// Round half up; -0 for x in [-0.5, -0].
function $$Math_round(x) {
  x = +x;
  const r = __intrinsic_floor(x);
  if (x - r >= 0.5) {
    return __intrinsic_copysign(r + 1, x);
  }
  return r;
}

function $$Math_sign(x) {
  x = +x;
  if (x > 0) {
    return 1;
  }
  if (x < 0) {
    return -1;
  }
  return x;
}

function $$Math_sqrt(x) {
  return __intrinsic_sqrt(+x);
}

function $$Math_fround(x) {
  return __intrinsic_fround(+x);
}

function $$Math_clz32(x) {
  return __intrinsic_clz32(+x);
}

function $$Math_imul(a, b) {
  return __intrinsic_imul(+a, +b);
}

function $$Math_pow(x, y) {
  return (+x) ** (+y);
}

// xorshift128+ (V8's generator) on 32-bit halves, seeded from the host on first use.
let $$random_seeded = false;
let $$random_s0hi = 0;
let $$random_s0lo = 0;
let $$random_s1hi = 0;
let $$random_s1lo = 0;

function $$Math_random() {
  if (!$$random_seeded) {
    $$random_seeded = true;
    $$random_s0hi = __intrinsic_random_seed();
    $$random_s0lo = __intrinsic_random_seed();
    $$random_s1hi = __intrinsic_random_seed();
    $$random_s1lo = __intrinsic_random_seed() | 1;
  }
  // s1 = state0; s0 = state1; state0 = s0;
  let ahi = $$random_s0hi;
  let alo = $$random_s0lo;
  const bhi = $$random_s1hi;
  const blo = $$random_s1lo;
  $$random_s0hi = bhi;
  $$random_s0lo = blo;
  // s1 ^= s1 << 23
  ahi ^= (ahi << 23) | (alo >>> 9);
  alo ^= alo << 23;
  // s1 ^= s1 >>> 17
  alo ^= (alo >>> 17) | (ahi << 15);
  ahi ^= ahi >>> 17;
  // s1 ^= s0; s1 ^= s0 >>> 26
  ahi ^= bhi;
  alo ^= blo;
  alo ^= (blo >>> 26) | (bhi << 6);
  ahi ^= bhi >>> 26;
  $$random_s1hi = ahi;
  $$random_s1lo = alo;
  // The top 52 bits of state0 as the mantissa of a double in [1, 2), minus 1.
  const hi = 0x3ff00000 | (bhi >>> 12);
  const lo = (bhi << 20) | (blo >>> 12);
  return __intrinsic_from_words(hi, lo) - 1;
}

// Every argument is converted first (spec order), then compared.
function $$Math_max(...values) {
  let r = -Infinity;
  for (let i = 0; i < values.length; i++) {
    r = __intrinsic_max(r, +values[i]);
  }
  return r;
}

function $$Math_min(...values) {
  let r = Infinity;
  for (let i = 0; i < values.length; i++) {
    r = __intrinsic_min(r, +values[i]);
  }
  return r;
}

// V8's algorithm (math.tq): scale by the largest magnitude, Kahan summation.
function $$Math_hypot(...values) {
  const n = values.length;
  const abs = [];
  let max = 0;
  let nan = false;
  for (let i = 0; i < n; i++) {
    const v = __intrinsic_abs(+values[i]);
    abs.push(v);
    if (v !== v) {
      nan = true;
    } else if (v > max) {
      max = v;
    }
  }
  if (max === Infinity) {
    return Infinity;
  }
  if (nan) {
    return NaN;
  }
  if (max === 0) {
    return 0;
  }
  let sum = 0;
  let compensation = 0;
  for (let i = 0; i < n; i++) {
    const x = abs[i] / max;
    const summand = x * x - compensation;
    const preliminary = sum + summand;
    compensation = preliminary - sum - summand;
    sum = preliminary;
  }
  return __intrinsic_sqrt(sum) * max;
}

// ---- fdlibm -----------------------------------------------------------------------------
// Ports of fdlibm 5.3 / FreeBSD msun, the sources of V8's `base/ieee754.cc`. `__intrinsic_hi`
// / `__intrinsic_lo` read the high / low 32-bit word of a double (as int32), and
// `__intrinsic_from_words(hi, lo)` builds one. Unsigned C comparisons of words use `>>> 0`.

// x * 2^n, exact unless the result is subnormal (musl's scalbn).
function $$scalbn(x, n) {
  let y = x;
  if (n > 1023) {
    y *= __intrinsic_from_words(0x7fe00000, 0);
    n -= 1023;
    if (n > 1023) {
      y *= __intrinsic_from_words(0x7fe00000, 0);
      n -= 1023;
      if (n > 1023) {
        n = 1023;
      }
    }
  } else if (n < -1022) {
    // 2^-1022 * 2^53: avoid double rounding in the subnormal range.
    y *= __intrinsic_from_words(0x03600000, 0);
    n += 1022 - 53;
    if (n < -1022) {
      y *= __intrinsic_from_words(0x03600000, 0);
      n += 1022 - 53;
      if (n < -1022) {
        n = -1022;
      }
    }
  }
  return y * __intrinsic_from_words((0x3ff + n) << 20, 0);
}

function $$kernel_sin(x, y, iy) {
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix < 0x3e400000 && __intrinsic_trunc(x) === 0) {
    return x;
  }
  const z = x * x;
  const v = z * x;
  const r = 8.33333333332248946124e-03 + z * (-1.98412698298579493134e-04 +
    z * (2.75573137070700676789e-06 + z * (-2.50507602534068634195e-08 +
    z * 1.58969099521155010221e-10)));
  if (iy === 0) {
    return x + v * (-1.66666666666666324348e-01 + z * r);
  }
  return x - ((z * (0.5 * y - v * r) - y) - v * -1.66666666666666324348e-01);
}

function $$kernel_cos(x, y) {
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix < 0x3e400000 && __intrinsic_trunc(x) === 0) {
    return 1;
  }
  const z = x * x;
  const r = z * (4.16666666666666019037e-02 + z * (-1.38888888888741095749e-03 +
    z * (2.48015872894767294178e-05 + z * (-2.75573143513906633035e-07 +
    z * (2.08757232129817482790e-09 + z * -1.13596475577881948265e-11)))));
  if (ix < 0x3fd33333) {
    return 1 - (0.5 * z - (z * r - x * y));
  }
  let qx = 0.28125;
  if (ix <= 0x3fe90000) {
    qx = __intrinsic_from_words(ix - 0x00200000, 0);
  }
  const hz = 0.5 * z - qx;
  const a = 1 - qx;
  return a - (hz - (z * r - x * y));
}

function $$kernel_tan(x, y, iy) {
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  let z = 0;
  let w = 0;
  let v = 0;
  let s = 0;
  let r = 0;
  if (ix < 0x3e300000 && __intrinsic_trunc(x) === 0) {
    if (((ix | __intrinsic_lo(x)) | (iy + 1)) === 0) {
      return 1 / __intrinsic_abs(x);
    }
    if (iy === 1) {
      return x;
    }
    // -1 / (x + y), carefully.
    w = x + y;
    z = __intrinsic_from_words(__intrinsic_hi(w), 0);
    v = y - (z - x);
    const a = -1 / w;
    const t = __intrinsic_from_words(__intrinsic_hi(a), 0);
    s = 1 + t * z;
    return t + a * (s + t * v);
  }
  if (ix >= 0x3fe59428) {
    // |x| >= 0.6744
    if (hx < 0) {
      x = -x;
      y = -y;
    }
    z = 7.85398163397448278999e-01 - x;
    w = 3.06161699786838301793e-17 - y;
    x = z + w;
    y = 0;
  }
  z = x * x;
  w = z * z;
  r = 1.33333333333201242699e-01 + w * (2.18694882948595424599e-02 +
    w * (3.59207910759131235356e-03 + w * (5.88041240820264096874e-04 +
    w * (7.81794442939557092300e-05 + w * -1.85586374855275456654e-05))));
  v = z * (5.39682539762260521377e-02 + w * (8.86323982359930005737e-03 +
    w * (1.45620945432529025516e-03 + w * (2.46463134818469906812e-04 +
    w * (7.14072491382608190305e-05 + w * 2.59073051863633712884e-05)))));
  s = z * x;
  r = y + z * (s * (r + v) + y);
  r += 3.33333333333334091986e-01 * s;
  w = x + r;
  if (ix >= 0x3fe59428) {
    v = iy;
    return (1 - ((hx >> 30) & 2)) * (v - 2 * (x - (w * w / (w + v) - r)));
  }
  if (iy === 1) {
    return w;
  }
  z = __intrinsic_from_words(__intrinsic_hi(w), 0);
  v = r - (z - x);
  const a = -1 / w;
  const t = __intrinsic_from_words(__intrinsic_hi(a), 0);
  s = 1 + t * z;
  return t + a * (s + t * v);
}

// 2/pi in 24-bit chunks, for the reduction of huge arguments.
function $$two_over_pi() {
  return [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62,
    0x95993C, 0x439041, 0xFE5163, 0xABDEBB, 0xC561B7, 0x246E3A,
    0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41,
    0x3991D6, 0x398353, 0x39F49C, 0x845F8B, 0xBDF928, 0x3B1FF8,
    0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5,
    0xF17B3D, 0x0739F7, 0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08,
    0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880,
    0x4D7327, 0x310606, 0x1556CA, 0x73A8C9, 0x60E27B, 0xC08C6B,
  ];
}

// __kernel_rem_pio2 with prec = 2 (double): x = tx[0..nx], the result in y[0], y[1].
function $$kernel_rem_pio2(x, y, e0, nx) {
  const ipio2 = $$two_over_pi();
  const PIo2 = [
    1.57079625129699707031e+00, 7.54978941586159635335e-08, 5.39030252995776476554e-15,
    3.28200341580791294123e-22, 1.27065575308067607349e-29, 1.22933308981111328932e-36,
    2.73370053816464559624e-44, 2.16741683877804819444e-51,
  ];
  const two24 = 1.67772160000000000000e+07;
  const twon24 = 5.96046447753906250000e-08;
  const jk = 4;
  const jp = jk;
  const iq = [];
  const f = [];
  const fq = [];
  const q = [];
  for (let i = 0; i < 20; i++) {
    iq.push(0);
    f.push(0);
    fq.push(0);
    q.push(0);
  }
  const jx = nx - 1;
  let jv = __intrinsic_trunc((e0 - 3) / 24);
  if (jv < 0) {
    jv = 0;
  }
  let q0 = e0 - 24 * (jv + 1);
  let j = jv - jx;
  const m = jx + jk;
  for (let i = 0; i <= m; i++, j++) {
    f[i] = j < 0 ? 0 : ipio2[j];
  }
  for (let i = 0; i <= jk; i++) {
    let fw = 0;
    for (j = 0; j <= jx; j++) {
      fw += x[j] * f[jx + i - j];
    }
    q[i] = fw;
  }
  let jz = jk;
  let z = 0;
  let n = 0;
  let ih = 0;
  for (;;) {
    // recompute:
    let i = 0;
    z = q[jz];
    for (j = jz; j > 0; i++, j--) {
      const fw = __intrinsic_trunc(twon24 * z);
      iq[i] = __intrinsic_trunc(z - two24 * fw);
      z = q[j - 1] + fw;
    }
    z = $$scalbn(z, q0);
    z -= 8 * __intrinsic_floor(z * 0.125);
    n = __intrinsic_trunc(z);
    z -= n;
    ih = 0;
    if (q0 > 0) {
      i = iq[jz - 1] >> (24 - q0);
      n += i;
      iq[jz - 1] -= i << (24 - q0);
      ih = iq[jz - 1] >> (23 - q0);
    } else if (q0 === 0) {
      ih = iq[jz - 1] >> 23;
    } else if (z >= 0.5) {
      ih = 2;
    }
    if (ih > 0) {
      n += 1;
      let carry = 0;
      for (i = 0; i < jz; i++) {
        j = iq[i];
        if (carry === 0) {
          if (j !== 0) {
            carry = 1;
            iq[i] = 0x1000000 - j;
          }
        } else {
          iq[i] = 0xffffff - j;
        }
      }
      if (q0 === 1) {
        iq[jz - 1] &= 0x7fffff;
      } else if (q0 === 2) {
        iq[jz - 1] &= 0x3fffff;
      }
      if (ih === 2) {
        z = 1 - z;
        if (carry !== 0) {
          z -= $$scalbn(1, q0);
        }
      }
    }
    if (z === 0) {
      j = 0;
      for (i = jz - 1; i >= jk; i--) {
        j |= iq[i];
      }
      if (j === 0) {
        // Need more of 2/pi.
        let k = 1;
        while (iq[jk - k] === 0) {
          k++;
        }
        for (i = jz + 1; i <= jz + k; i++) {
          f[jx + i] = ipio2[jv + i];
          let fw = 0;
          for (j = 0; j <= jx; j++) {
            fw += x[j] * f[jx + i - j];
          }
          q[i] = fw;
        }
        jz += k;
        continue;
      }
    }
    break;
  }
  if (z === 0) {
    jz -= 1;
    q0 -= 24;
    while (iq[jz] === 0) {
      jz--;
      q0 -= 24;
    }
  } else {
    z = $$scalbn(z, -q0);
    if (z >= two24) {
      const fw = __intrinsic_trunc(twon24 * z);
      iq[jz] = __intrinsic_trunc(z - two24 * fw);
      jz += 1;
      q0 += 24;
      iq[jz] = fw;
    } else {
      iq[jz] = __intrinsic_trunc(z);
    }
  }
  let fw = $$scalbn(1, q0);
  for (let i = jz; i >= 0; i--) {
    q[i] = fw * iq[i];
    fw *= twon24;
  }
  for (let i = jz; i >= 0; i--) {
    fw = 0;
    for (let k = 0; k <= jp && k <= jz - i; k++) {
      fw += PIo2[k] * q[i + k];
    }
    fq[jz - i] = fw;
  }
  fw = 0;
  for (let i = jz; i >= 0; i--) {
    fw += fq[i];
  }
  y[0] = ih === 0 ? fw : -fw;
  fw = fq[0] - fw;
  for (let i = 1; i <= jz; i++) {
    fw += fq[i];
  }
  y[1] = ih === 0 ? fw : -fw;
  return n & 7;
}

// x - n*pi/2 in y[0] + y[1]; returns n.
function $$rem_pio2(x, y) {
  const pio2_1 = 1.57079632673412561417e+00;
  const pio2_1t = 6.07710050650619224932e-11;
  const pio2_2 = 6.07710050630396597660e-11;
  const pio2_2t = 2.02226624879595063154e-21;
  const pio2_3 = 2.02226624871116645580e-21;
  const pio2_3t = 8.47842766036889956997e-32;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  if (ix <= 0x3fe921fb) {
    y[0] = x;
    y[1] = 0;
    return 0;
  }
  if (ix < 0x4002d97c) {
    // |x| < 3pi/4: n = ±1.
    if (hx > 0) {
      let z = x - pio2_1;
      if (ix !== 0x3ff921fb) {
        y[0] = z - pio2_1t;
        y[1] = (z - y[0]) - pio2_1t;
      } else {
        z -= pio2_2;
        y[0] = z - pio2_2t;
        y[1] = (z - y[0]) - pio2_2t;
      }
      return 1;
    }
    let z = x + pio2_1;
    if (ix !== 0x3ff921fb) {
      y[0] = z + pio2_1t;
      y[1] = (z - y[0]) + pio2_1t;
    } else {
      z += pio2_2;
      y[0] = z + pio2_2t;
      y[1] = (z - y[0]) + pio2_2t;
    }
    return -1;
  }
  if (ix <= 0x413921fb) {
    // |x| <= 2^19 * pi/2: medium size.
    let t = __intrinsic_abs(x);
    const n = __intrinsic_trunc(t * 6.36619772367581382433e-01 + 0.5);
    const fn = n;
    let r = t - fn * pio2_1;
    let w = fn * pio2_1t;
    const npio2_hw = [
      0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C,
      0x4025FDBB, 0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C,
      0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A,
      0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
      0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB,
      0x404858EB, 0x404921FB,
    ];
    if (n < 32 && ix !== npio2_hw[n - 1]) {
      y[0] = r - w;
    } else {
      const j = ix >> 20;
      y[0] = r - w;
      let i = j - ((__intrinsic_hi(y[0]) >> 20) & 0x7ff);
      if (i > 16) {
        // 2nd iteration: good to 118 bits.
        t = r;
        w = fn * pio2_2;
        r = t - w;
        w = fn * pio2_2t - ((t - r) - w);
        y[0] = r - w;
        i = j - ((__intrinsic_hi(y[0]) >> 20) & 0x7ff);
        if (i > 49) {
          // 3rd iteration: 151 bits.
          t = r;
          w = fn * pio2_3;
          r = t - w;
          w = fn * pio2_3t - ((t - r) - w);
          y[0] = r - w;
        }
      }
    }
    y[1] = (r - y[0]) - w;
    if (hx < 0) {
      y[0] = -y[0];
      y[1] = -y[1];
      return -n;
    }
    return n;
  }
  if (ix >= 0x7ff00000) {
    y[0] = x - x;
    y[1] = x - x;
    return 0;
  }
  // Huge: z = scalbn(|x|, ilogb(x) - 23), split into three 24-bit chunks.
  const e0 = (ix >> 20) - 1046;
  let z = __intrinsic_from_words(ix - (e0 << 20), __intrinsic_lo(x));
  const tx = [0, 0, 0];
  for (let i = 0; i < 2; i++) {
    tx[i] = __intrinsic_trunc(z);
    z = (z - tx[i]) * 1.67772160000000000000e+07;
  }
  tx[2] = z;
  let nx = 3;
  while (tx[nx - 1] === 0) {
    nx--;
  }
  const n = $$kernel_rem_pio2(tx, y, e0, nx);
  if (hx < 0) {
    y[0] = -y[0];
    y[1] = -y[1];
    return -n;
  }
  return n;
}

function $$Math_sin(x) {
  x = +x;
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix <= 0x3fe921fb) {
    return $$kernel_sin(x, 0, 0);
  }
  if (ix >= 0x7ff00000) {
    return x - x;
  }
  const y = [0, 0];
  const n = $$rem_pio2(x, y) & 3;
  if (n === 0) {
    return $$kernel_sin(y[0], y[1], 1);
  }
  if (n === 1) {
    return $$kernel_cos(y[0], y[1]);
  }
  if (n === 2) {
    return -$$kernel_sin(y[0], y[1], 1);
  }
  return -$$kernel_cos(y[0], y[1]);
}

function $$Math_cos(x) {
  x = +x;
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix <= 0x3fe921fb) {
    return $$kernel_cos(x, 0);
  }
  if (ix >= 0x7ff00000) {
    return x - x;
  }
  const y = [0, 0];
  const n = $$rem_pio2(x, y) & 3;
  if (n === 0) {
    return $$kernel_cos(y[0], y[1]);
  }
  if (n === 1) {
    return -$$kernel_sin(y[0], y[1], 1);
  }
  if (n === 2) {
    return -$$kernel_cos(y[0], y[1]);
  }
  return $$kernel_sin(y[0], y[1], 1);
}

function $$Math_tan(x) {
  x = +x;
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix <= 0x3fe921fb) {
    return $$kernel_tan(x, 0, 1);
  }
  if (ix >= 0x7ff00000) {
    return x - x;
  }
  const y = [0, 0];
  const n = $$rem_pio2(x, y);
  return $$kernel_tan(y[0], y[1], 1 - ((n & 1) << 1));
}

function $$atan(x) {
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  let id = -1;
  if (ix >= 0x44100000) {
    // |x| >= 2^66
    if (ix > 0x7ff00000 || (ix === 0x7ff00000 && __intrinsic_lo(x) !== 0)) {
      return x + x;
    }
    if (hx > 0) {
      return 1.57079632679489655800e+00 + 6.12323399573676603587e-17;
    }
    return -1.57079632679489655800e+00 - 6.12323399573676603587e-17;
  }
  if (ix < 0x3fdc0000) {
    // |x| < 0.4375
    if (ix < 0x3e400000) {
      return x;
    }
  } else {
    x = __intrinsic_abs(x);
    if (ix < 0x3ff30000) {
      if (ix < 0x3fe60000) {
        id = 0;
        x = (2 * x - 1) / (2 + x);
      } else {
        id = 1;
        x = (x - 1) / (x + 1);
      }
    } else if (ix < 0x40038000) {
      id = 2;
      x = (x - 1.5) / (1 + 1.5 * x);
    } else {
      id = 3;
      x = -1 / x;
    }
  }
  const z = x * x;
  const w = z * z;
  const s1 = z * (3.33333333333329318027e-01 + w * (1.42857142725034663711e-01 +
    w * (9.09088713343650656196e-02 + w * (6.66107313738753120669e-02 +
    w * (4.97687799461593236017e-02 + w * 1.62858201153657823623e-02)))));
  const s2 = w * (-1.99999999998764832476e-01 + w * (-1.11111104054623557880e-01 +
    w * (-7.69187620504482999495e-02 + w * (-5.83357013379057348645e-02 +
    w * -3.65315727442169155270e-02))));
  if (id < 0) {
    return x - x * (s1 + s2);
  }
  let hi = 1.57079632679489655800e+00;
  let lo = 6.12323399573676603587e-17;
  if (id === 0) {
    hi = 4.63647609000806093515e-01;
    lo = 2.26987774529616870924e-17;
  } else if (id === 1) {
    hi = 7.85398163397448278999e-01;
    lo = 3.06161699786838301793e-17;
  } else if (id === 2) {
    hi = 9.82793723247329054082e-01;
    lo = 1.39033110312309984516e-17;
  }
  const r = hi - ((x * (s1 + s2) - lo) - x);
  return hx < 0 ? -r : r;
}

function $$Math_atan(x) {
  return $$atan(+x);
}

function $$Math_atan2(y, x) {
  y = +y;
  x = +x;
  const pi_o_4 = 7.8539816339744827900e-01;
  const pi_o_2 = 1.5707963267948965580e+00;
  const pi = 3.1415926535897931160e+00;
  const pi_lo = 1.2246467991473531772e-16;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  const lx = __intrinsic_lo(x);
  const hy = __intrinsic_hi(y);
  const iy = hy & 0x7fffffff;
  const ly = __intrinsic_lo(y);
  if ((ix | ((lx | -lx) >>> 31)) > 0x7ff00000 || (iy | ((ly | -ly) >>> 31)) > 0x7ff00000) {
    return x + y;
  }
  if (((hx - 0x3ff00000) | lx) === 0) {
    return $$atan(y);
  }
  const m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
  if ((iy | ly) === 0) {
    if (m < 2) {
      return y;
    }
    return m === 2 ? pi : -pi;
  }
  if ((ix | lx) === 0) {
    return hy < 0 ? -pi_o_2 : pi_o_2;
  }
  if (ix === 0x7ff00000) {
    if (iy === 0x7ff00000) {
      if (m === 0) {
        return pi_o_4;
      }
      if (m === 1) {
        return -pi_o_4;
      }
      if (m === 2) {
        return 3 * pi_o_4;
      }
      return -3 * pi_o_4;
    }
    if (m === 0) {
      return 0;
    }
    if (m === 1) {
      return -0;
    }
    return m === 2 ? pi : -pi;
  }
  if (iy === 0x7ff00000) {
    return hy < 0 ? -pi_o_2 : pi_o_2;
  }
  const k = (iy - ix) >> 20;
  let z = 0;
  if (k > 60) {
    z = pi_o_2 + 0.5 * pi_lo;
  } else if (hx < 0 && k < -60) {
    z = 0;
  } else {
    z = $$atan(__intrinsic_abs(y / x));
  }
  if (m === 0) {
    return z;
  }
  if (m === 1) {
    return -z;
  }
  if (m === 2) {
    return pi - (z - pi_lo);
  }
  return (z - pi_lo) - pi;
}

// The rational approximation asin and acos share: p(t) / q(t).
function $$asin_r(t) {
  const p = t * (1.66666666666666657415e-01 + t * (-3.25565818622400915405e-01 +
    t * (2.01212532134862925881e-01 + t * (-4.00555345006794114027e-02 +
    t * (7.91534994289814532176e-04 + t * 3.47933107596021167570e-05)))));
  const q = 1 + t * (-2.40339491173441421878e+00 + t * (2.02094576023350569471e+00 +
    t * (-6.88283971605453293030e-01 + t * 7.70381505559019352791e-02)));
  return p / q;
}

function $$Math_asin(x) {
  x = +x;
  const pio2_hi = 1.57079632679489655800e+00;
  const pio2_lo = 6.12323399573676603587e-17;
  const pio4_hi = 7.85398163397448278999e-01;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  if (ix >= 0x3ff00000) {
    if (((ix - 0x3ff00000) | __intrinsic_lo(x)) === 0) {
      return x * pio2_hi + x * pio2_lo;
    }
    return (x - x) / (x - x);
  }
  if (ix < 0x3fe00000) {
    if (ix < 0x3e400000) {
      return x;
    }
    return x + x * $$asin_r(x * x);
  }
  const w0 = 1 - __intrinsic_abs(x);
  const t0 = w0 * 0.5;
  const s = __intrinsic_sqrt(t0);
  let t = 0;
  if (ix >= 0x3fef3333) {
    // |x| > 0.975
    t = pio2_hi - (2 * (s + s * $$asin_r(t0)) - pio2_lo);
  } else {
    const w = __intrinsic_from_words(__intrinsic_hi(s), 0);
    const c = (t0 - w * w) / (s + w);
    const p = 2 * s * $$asin_r(t0) - (pio2_lo - 2 * c);
    const q = pio4_hi - 2 * w;
    t = pio4_hi - (p - q);
  }
  return hx > 0 ? t : -t;
}

function $$Math_acos(x) {
  x = +x;
  const pi = 3.14159265358979311600e+00;
  const pio2_hi = 1.57079632679489655800e+00;
  const pio2_lo = 6.12323399573676603587e-17;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  if (ix >= 0x3ff00000) {
    if (((ix - 0x3ff00000) | __intrinsic_lo(x)) === 0) {
      return hx > 0 ? 0 : pi + 2 * pio2_lo;
    }
    return (x - x) / (x - x);
  }
  if (ix < 0x3fe00000) {
    if (ix <= 0x3c600000) {
      return pio2_hi + pio2_lo;
    }
    return pio2_hi - (x - (pio2_lo - x * $$asin_r(x * x)));
  }
  if (hx < 0) {
    const z = (1 + x) * 0.5;
    const s = __intrinsic_sqrt(z);
    const w = $$asin_r(z) * s - pio2_lo;
    return pi - 2 * (s + w);
  }
  const z = (1 - x) * 0.5;
  const s = __intrinsic_sqrt(z);
  const df = __intrinsic_from_words(__intrinsic_hi(s), 0);
  const c = (z - df * df) / (s + df);
  const w = $$asin_r(z) * s + c;
  return 2 * (df + w);
}

function $$exp(x) {
  const ln2_hi = 6.93147180369123816490e-01;
  const ln2_lo = 1.90821492927058770002e-10;
  const twom1000 = 9.33263618503218878990e-302;
  let hx = __intrinsic_hi(x);
  const xsb = (hx >> 31) & 1;
  hx &= 0x7fffffff;
  let hi = 0;
  let lo = 0;
  let k = 0;
  if (hx >= 0x40862e42) {
    // |x| >= 709.78...
    if (hx >= 0x7ff00000) {
      if (((hx & 0xfffff) | __intrinsic_lo(x)) !== 0) {
        return x + x;
      }
      return xsb === 0 ? x : 0;
    }
    if (x > 7.09782712893383973096e+02) {
      return Infinity;
    }
    if (x < -7.45133219101941108420e+02) {
      return 0;
    }
  }
  if (hx > 0x3fd62e42) {
    // |x| > 0.5 ln2
    if (hx < 0x3ff0a2b2) {
      // and |x| < 1.5 ln2. V8 special-cases exp(1).
      if (x === 1) {
        return 2.718281828459045;
      }
      hi = xsb === 0 ? x - ln2_hi : x + ln2_hi;
      lo = xsb === 0 ? ln2_lo : -ln2_lo;
      k = 1 - xsb - xsb;
    } else {
      k = __intrinsic_trunc(1.44269504088896338700e+00 * x + (xsb === 0 ? 0.5 : -0.5));
      hi = x - k * ln2_hi;
      lo = k * ln2_lo;
    }
    x = hi - lo;
  } else if (hx < 0x3e300000) {
    // |x| < 2^-28
    return 1 + x;
  }
  const t = x * x;
  let twopk = 0;
  if (k >= -1021) {
    twopk = __intrinsic_from_words(0x3ff00000 + (k << 20), 0);
  } else {
    twopk = __intrinsic_from_words(0x3ff00000 + ((k + 1000) << 20), 0);
  }
  const c = x - t * (1.66666666666666019037e-01 + t * (-2.77777777770155933842e-03 +
    t * (6.61375632143793436117e-05 + t * (-1.65339022054652515390e-06 +
    t * 4.13813679705723846039e-08))));
  if (k === 0) {
    return 1 - ((x * c) / (c - 2) - x);
  }
  const y = 1 - ((lo - (x * c) / (2 - c)) - hi);
  if (k >= -1021) {
    if (k === 1024) {
      return y * 2 * __intrinsic_from_words(0x7fe00000, 0);
    }
    return y * twopk;
  }
  return y * twopk * twom1000;
}

function $$Math_exp(x) {
  return $$exp(+x);
}

function $$expm1(x) {
  const ln2_hi = 6.93147180369123816490e-01;
  const ln2_lo = 1.90821492927058770002e-10;
  let hx = __intrinsic_hi(x);
  const xsb = hx & 0x80000000;
  hx &= 0x7fffffff;
  let k = 0;
  let c = 0;
  if (hx >= 0x4043687a) {
    // |x| >= 56 ln2
    if (hx >= 0x40862e42) {
      if (hx >= 0x7ff00000) {
        if (((hx & 0xfffff) | __intrinsic_lo(x)) !== 0) {
          return x + x;
        }
        return xsb === 0 ? x : -1;
      }
      if (x > 7.09782712893383973096e+02) {
        return Infinity;
      }
    }
    if (xsb !== 0) {
      return -1;
    }
  }
  if (hx > 0x3fd62e42) {
    // |x| > 0.5 ln2
    let hi = 0;
    let lo = 0;
    if (hx < 0x3ff0a2b2) {
      if (xsb === 0) {
        hi = x - ln2_hi;
        lo = ln2_lo;
        k = 1;
      } else {
        hi = x + ln2_hi;
        lo = -ln2_lo;
        k = -1;
      }
    } else {
      k = __intrinsic_trunc(1.44269504088896338700e+00 * x + (xsb === 0 ? 0.5 : -0.5));
      hi = x - k * ln2_hi;
      lo = k * ln2_lo;
    }
    x = hi - lo;
    c = (hi - x) - lo;
  } else if (hx < 0x3c900000) {
    // |x| < 2^-54
    return x;
  }
  const hfx = 0.5 * x;
  const hxs = x * hfx;
  const r1 = 1 + hxs * (-3.33333333333331316428e-02 + hxs * (1.58730158725481460165e-03 +
    hxs * (-7.93650757867487942473e-05 + hxs * (4.00821782732936239552e-06 +
    hxs * -2.01099218183624371326e-07))));
  let t = 3 - r1 * hfx;
  let e = hxs * ((r1 - t) / (6 - x * t));
  if (k === 0) {
    return x - (x * e - hxs);
  }
  const twopk = __intrinsic_from_words((0x3ff + k) << 20, 0);
  e = x * (e - c) - c;
  e -= hxs;
  if (k === -1) {
    return 0.5 * (x - e) - 0.5;
  }
  if (k === 1) {
    if (x < -0.25) {
      return -2 * (e - (x + 0.5));
    }
    return 1 + 2 * (x - e);
  }
  if (k <= -2 || k > 56) {
    let y = 1 - (e - x);
    if (k === 1024) {
      y = y * 2 * __intrinsic_from_words(0x7fe00000, 0);
    } else {
      y = y * twopk;
    }
    return y - 1;
  }
  if (k < 20) {
    t = __intrinsic_from_words(0x3ff00000 - (0x200000 >> k), 0);
    return (t - (e - x)) * twopk;
  }
  t = __intrinsic_from_words((0x3ff - k) << 20, 0);
  return ((x - (e + t)) + 1) * twopk;
}

function $$Math_expm1(x) {
  return $$expm1(+x);
}

function $$log(x) {
  const ln2_hi = 6.93147180369123816490e-01;
  const ln2_lo = 1.90821492927058770002e-10;
  let hx = __intrinsic_hi(x);
  const lx = __intrinsic_lo(x);
  let k = 0;
  if (hx < 0x00100000) {
    // x < 2^-1022
    if (((hx & 0x7fffffff) | lx) === 0) {
      return -Infinity;
    }
    if (hx < 0) {
      return NaN;
    }
    // Subnormal: scale up.
    k -= 54;
    x *= 1.80143985094819840000e+16;
    hx = __intrinsic_hi(x);
  }
  if (hx >= 0x7ff00000) {
    return x + x;
  }
  k += (hx >> 20) - 1023;
  hx &= 0x000fffff;
  let i = (hx + 0x95f64) & 0x100000;
  // Normalize x or x/2.
  x = __intrinsic_from_words(hx | (i ^ 0x3ff00000), __intrinsic_lo(x));
  k += i >> 20;
  const f = x - 1;
  if ((0x000fffff & (2 + hx)) < 3) {
    // -2^-20 <= f < 2^-20
    if (f === 0) {
      if (k === 0) {
        return 0;
      }
      return k * ln2_hi + k * ln2_lo;
    }
    const R = f * f * (0.5 - 0.33333333333333333 * f);
    if (k === 0) {
      return f - R;
    }
    return k * ln2_hi - ((R - k * ln2_lo) - f);
  }
  const s = f / (2 + f);
  const dk = k;
  const z = s * s;
  i = hx - 0x6147a;
  const w = z * z;
  const j = 0x6b851 - hx;
  const t1 = w * (3.999999999940941908e-01 + w * (2.222219843214978396e-01 +
    w * 1.531383769920937332e-01));
  const t2 = z * (6.666666666666735130e-01 + w * (2.857142874366239149e-01 +
    w * (1.818357216161805012e-01 + w * 1.479819860511658591e-01)));
  i |= j;
  const R = t2 + t1;
  if (i > 0) {
    const hfsq = 0.5 * f * f;
    if (k === 0) {
      return f - (hfsq - s * (hfsq + R));
    }
    return dk * ln2_hi - ((hfsq - (s * (hfsq + R) + dk * ln2_lo)) - f);
  }
  if (k === 0) {
    return f - s * (f - R);
  }
  return dk * ln2_hi - ((s * (f - R) - dk * ln2_lo) - f);
}

function $$Math_log(x) {
  return $$log(+x);
}

function $$log1p(x) {
  const ln2_hi = 6.93147180369123816490e-01;
  const ln2_lo = 1.90821492927058770002e-10;
  const hx = __intrinsic_hi(x);
  const ax = hx & 0x7fffffff;
  let k = 1;
  let f = 0;
  let hu = 0;
  let c = 0;
  if (hx < 0x3fda827a) {
    // 1 + x < sqrt(2)+
    if (ax >= 0x3ff00000) {
      // x <= -1
      if (x === -1) {
        return -Infinity;
      }
      return NaN;
    }
    if (ax < 0x3e200000) {
      // |x| < 2^-29
      if (ax < 0x3c900000) {
        return x;
      }
      return x - x * x * 0.5;
    }
    if (hx > 0 || hx <= (0xbfd2bec4 | 0)) {
      // sqrt(2)/2- <= 1 + x < sqrt(2)+
      k = 0;
      f = x;
      hu = 1;
    }
  }
  if (hx >= 0x7ff00000) {
    return x + x;
  }
  if (k !== 0) {
    let u = 0;
    if (hx < 0x43400000) {
      u = 1 + x;
      hu = __intrinsic_hi(u);
      k = (hu >> 20) - 1023;
      // Correction term.
      c = k > 0 ? 1 - (u - x) : x - (u - 1);
      c /= u;
    } else {
      u = x;
      hu = __intrinsic_hi(u);
      k = (hu >> 20) - 1023;
      c = 0;
    }
    hu &= 0x000fffff;
    if (hu < 0x6a09e) {
      // Normalize u.
      u = __intrinsic_from_words(hu | 0x3ff00000, __intrinsic_lo(u));
    } else {
      // Normalize u/2.
      k += 1;
      u = __intrinsic_from_words(hu | 0x3fe00000, __intrinsic_lo(u));
      hu = (0x00100000 - hu) >> 2;
    }
    f = u - 1;
  }
  const hfsq = 0.5 * f * f;
  if (hu === 0) {
    // |f| < 2^-20
    if (f === 0) {
      if (k === 0) {
        return 0;
      }
      c += k * ln2_lo;
      return k * ln2_hi + c;
    }
    const R = hfsq * (1 - 0.66666666666666666 * f);
    if (k === 0) {
      return f - R;
    }
    return k * ln2_hi - ((R - (k * ln2_lo + c)) - f);
  }
  const s = f / (2 + f);
  const z = s * s;
  const R = z * (6.666666666666735130e-01 + z * (3.999999999940941908e-01 +
    z * (2.857142874366239149e-01 + z * (2.222219843214978396e-01 +
    z * (1.818357216161805012e-01 + z * (1.531383769920937332e-01 +
    z * 1.479819860511658591e-01))))));
  if (k === 0) {
    return f - (hfsq - s * (hfsq + R));
  }
  return k * ln2_hi - ((hfsq - (s * (hfsq + R) + (k * ln2_lo + c))) - f);
}

function $$Math_log1p(x) {
  return $$log1p(+x);
}

// log(1 + f) - f + f*f/2 for f in [sqrt(2)/2 - 1, sqrt(2) - 1] (FreeBSD k_log.h).
function $$k_log1p(f) {
  const s = f / (2 + f);
  const z = s * s;
  const w = z * z;
  const t1 = w * (3.999999999940941908e-01 + w * (2.222219843214978396e-01 +
    w * 1.531383769920937332e-01));
  const t2 = z * (6.666666666666735130e-01 + w * (2.857142874366239149e-01 +
    w * (1.818357216161805012e-01 + w * 1.479819860511658591e-01)));
  const R = t2 + t1;
  const hfsq = 0.5 * f * f;
  return s * (hfsq + R);
}

// log2 of x (FreeBSD e_log2.c).
function $$log2(x) {
  let hx = __intrinsic_hi(x);
  const lx = __intrinsic_lo(x);
  let k = 0;
  if (hx < 0x00100000) {
    if (((hx & 0x7fffffff) | lx) === 0) {
      return -Infinity;
    }
    if (hx < 0) {
      return NaN;
    }
    k -= 54;
    x *= 1.80143985094819840000e+16;
    hx = __intrinsic_hi(x);
  }
  if (hx >= 0x7ff00000) {
    return x + x;
  }
  if (hx === 0x3ff00000 && __intrinsic_lo(x) === 0) {
    return 0;
  }
  k += (hx >> 20) - 1023;
  hx &= 0x000fffff;
  const i = (hx + 0x95f64) & 0x100000;
  x = __intrinsic_from_words(hx | (i ^ 0x3ff00000), __intrinsic_lo(x));
  k += i >> 20;
  const y = k;
  const f = x - 1;
  const hfsq = 0.5 * f * f;
  const r = $$k_log1p(f);
  const hi = __intrinsic_from_words(__intrinsic_hi(f - hfsq), 0);
  const lo = (f - hi) - hfsq + r;
  let val_hi = hi * 1.44269504072144627571e+00;
  let val_lo = (lo + hi) * 1.67517131648865118353e-10 + lo * 1.44269504072144627571e+00;
  const w = y + val_hi;
  val_lo += (y - w) + val_hi;
  val_hi = w;
  return val_lo + val_hi;
}

// log10 of x (fdlibm e_log10.c, which V8 uses; not FreeBSD's).
function $$log10(x) {
  let hx = __intrinsic_hi(x);
  const lx = __intrinsic_lo(x);
  let k = 0;
  if (hx < 0x00100000) {
    if (((hx & 0x7fffffff) | lx) === 0) {
      return -Infinity;
    }
    if (hx < 0) {
      return NaN;
    }
    k -= 54;
    x *= 1.80143985094819840000e+16;
    hx = __intrinsic_hi(x);
  }
  if (hx >= 0x7ff00000) {
    return x + x;
  }
  k += (hx >> 20) - 1023;
  const i = (k & 0x80000000) >>> 31;
  hx = (hx & 0x000fffff) | ((0x3ff - i) << 20);
  const y = k + i;
  x = __intrinsic_from_words(hx, __intrinsic_lo(x));
  const z = y * 3.69423907715893078616e-13 + 4.34294481903251816668e-01 * $$log(x);
  return z + y * 3.01029995663611771306e-01;
}

function $$Math_log2(x) {
  return $$log2(+x);
}

function $$Math_log10(x) {
  return $$log10(+x);
}

function $$Math_cbrt(x) {
  x = +x;
  let hx = __intrinsic_hi(x);
  const sign = hx & 0x80000000;
  hx ^= sign;
  if (hx >= 0x7ff00000) {
    return x + x;
  }
  let t = 0;
  if (hx < 0x00100000) {
    // Zero or subnormal.
    if ((hx | __intrinsic_lo(x)) === 0) {
      return x;
    }
    t = __intrinsic_from_words(0x43500000, 0) * x;
    const high = __intrinsic_hi(t);
    t = __intrinsic_from_words(sign | (__intrinsic_trunc((high & 0x7fffffff) / 3) + 696219795), 0);
  } else {
    t = __intrinsic_from_words(sign | (__intrinsic_trunc(hx / 3) + 715094163), 0);
  }
  // New cbrt to 23 bits.
  let r = (t * t) * (t / x);
  t = t * ((1.87595182427177009643 + r * (-1.88497979543377169875 + r * 1.621429720105354466140)) +
    ((r * r) * r) * (-0.758397934778766047437 + r * 0.145996192886612446982));
  // Round t away from zero to 23 bits.
  let lo = (__intrinsic_lo(t) >>> 0) + 0x80000000;
  let hi = __intrinsic_hi(t);
  if (lo >= 4294967296) {
    lo -= 4294967296;
    hi += 1;
  }
  t = __intrinsic_from_words(hi, lo & 0xc0000000);
  // One Newton step to 53 bits.
  const s = t * t;
  r = x / s;
  const w = t + t;
  r = (r - t) / (w + r);
  return t + t * r;
}

function $$Math_sinh(x) {
  x = +x;
  const h = x < 0 ? -0.5 : 0.5;
  const ax = __intrinsic_abs(x);
  if (ax < 22) {
    if (ax < 3.725290298461914e-9) {
      return x;
    }
    const t = $$expm1(ax);
    if (ax < 1) {
      return h * (2 * t - t * t / (t + 1));
    }
    return h * (t + t / (t + 1));
  }
  if (ax < 709.7822265625) {
    return h * $$exp(ax);
  }
  if (ax <= 710.4758600739439) {
    const w = $$exp(0.5 * ax);
    const t = h * w;
    return t * w;
  }
  return x * 1.0e307;
}

function $$Math_cosh(x) {
  x = +x;
  const ix = __intrinsic_hi(x) & 0x7fffffff;
  if (ix < 0x3fd62e43) {
    const t = $$expm1(__intrinsic_abs(x));
    const w = 1 + t;
    if (ix < 0x3c800000) {
      return w;
    }
    return 1 + (t * t) / (w + w);
  }
  if (ix < 0x40360000) {
    const t = $$exp(__intrinsic_abs(x));
    return 0.5 * t + 0.5 / t;
  }
  if (ix < 0x40862e42) {
    return 0.5 * $$exp(__intrinsic_abs(x));
  }
  if (__intrinsic_abs(x) <= 710.4758600739439) {
    const w = $$exp(0.5 * __intrinsic_abs(x));
    const t = 0.5 * w;
    return t * w;
  }
  if (ix >= 0x7ff00000) {
    return x * x;
  }
  return Infinity;
}

function $$Math_tanh(x) {
  x = +x;
  const jx = __intrinsic_hi(x);
  const ix = jx & 0x7fffffff;
  if (ix >= 0x7ff00000) {
    if (jx >= 0) {
      return 1 / x + 1;
    }
    return 1 / x - 1;
  }
  let z = 1;
  if (ix < 0x40360000) {
    // |x| < 22
    if (ix < 0x3e300000) {
      return x;
    }
    if (ix >= 0x3ff00000) {
      const t = $$expm1(2 * __intrinsic_abs(x));
      z = 1 - 2 / (t + 2);
    } else {
      const t = $$expm1(-2 * __intrinsic_abs(x));
      z = -t / (t + 2);
    }
  }
  return jx >= 0 ? z : -z;
}

function $$Math_asinh(x) {
  x = +x;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  if (ix >= 0x7ff00000) {
    return x + x;
  }
  if (ix < 0x3e300000) {
    return x;
  }
  let w = 0;
  if (ix > 0x41b00000) {
    w = $$log(__intrinsic_abs(x)) + 6.93147180559945286227e-01;
  } else if (ix > 0x40000000) {
    const t = __intrinsic_abs(x);
    w = $$log(2 * t + 1 / (__intrinsic_sqrt(x * x + 1) + t));
  } else {
    const t = x * x;
    w = $$log1p(__intrinsic_abs(x) + t / (1 + __intrinsic_sqrt(1 + t)));
  }
  return hx > 0 ? w : -w;
}

function $$Math_acosh(x) {
  x = +x;
  const hx = __intrinsic_hi(x);
  if (hx < 0x3ff00000) {
    return (x - x) / (x - x);
  }
  if (hx >= 0x41b00000) {
    if (hx >= 0x7ff00000) {
      return x + x;
    }
    return $$log(x) + 6.93147180559945286227e-01;
  }
  if (((hx - 0x3ff00000) | __intrinsic_lo(x)) === 0) {
    return 0;
  }
  if (hx > 0x40000000) {
    const t = x * x;
    return $$log(2 * x - 1 / (x + __intrinsic_sqrt(t - 1)));
  }
  const t = x - 1;
  return $$log1p(t + __intrinsic_sqrt(2 * t + t * t));
}

function $$Math_atanh(x) {
  x = +x;
  const hx = __intrinsic_hi(x);
  const ix = hx & 0x7fffffff;
  const lx = __intrinsic_lo(x);
  if ((ix | ((lx | -lx) >>> 31)) > 0x3ff00000) {
    return (x - x) / (x - x);
  }
  if (ix === 0x3ff00000) {
    return x / 0;
  }
  if (ix < 0x3e300000) {
    return x;
  }
  x = __intrinsic_from_words(ix, lx);
  let t = 0;
  if (ix < 0x3fe00000) {
    t = x + x;
    t = 0.5 * $$log1p(t + t * x / (1 - x));
  } else {
    t = 0.5 * $$log1p((x + x) / (1 - x));
  }
  return hx >= 0 ? t : -t;
}
