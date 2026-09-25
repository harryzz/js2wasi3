// break / continue / labels crossing finally blocks, nested finally ordering.
let log = "";
for (let i = 0; i < 5; i++) {
  try {
    if (i === 1) continue;
    if (i === 3) break;
    log += "body" + i + " ";
  } finally {
    log += "fin" + i + " ";
  }
}
console.log(log);

log = "";
outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    try {
      try {
        if (j === 1) continue outer;
        if (i === 2) break outer;
        log += i + "" + j + " ";
      } finally {
        log += "in ";
      }
    } finally {
      log += "out ";
    }
  }
}
console.log(log);

function find(limit) {
  let i = 0;
  while (true) {
    try {
      if (i === limit) return "found " + i;
      i++;
    } finally {
      if (i > 10) return "gave up";
    }
  }
}
console.log(find(3), find(20));

function breakOverridesThrow() {
  let n = 0;
  do {
    try {
      n++;
      throw "ignored";
    } finally {
      break;
    }
  } while (n < 3);
  return n;
}
console.log(breakOverridesThrow());

log = "";
block: {
  try {
    log += "a";
    break block;
  } finally {
    log += "b";
  }
}
console.log(log);
