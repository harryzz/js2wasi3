// Writes and reads the streams of `acme:streams/bytes`.
import { echo, upper, take, words } from "acme:streams/bytes";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// `text` as a few chunks of bytes (ASCII), one after a timer.
async function* chunks(text) {
  for (let i = 0; i < text.length; i += 4) {
    if (i === 4) {
      await sleep(3);
    }
    const c = new Uint8Array(Math.min(4, text.length - i));
    for (let j = 0; j < c.length; j++) {
      c[j] = text.charCodeAt(i + j);
    }
    yield c;
  }
}

// Chunks forever: only a reader that stops (drops the stream) ends the pump.
function* endless() {
  for (;;) {
    yield new Uint8Array(5);
  }
}

function* big() {
  for (let i = 0; i < 100; i++) {
    const c = new Uint8Array(3000);
    c.fill(i);
    yield c;
  }
}

async function collect(stream) {
  let s = "";
  for await (const chunk of stream) {
    for (const b of chunk) {
      s += String.fromCharCode(b);
    }
  }
  return s;
}

export async function go(text) {
  const out = [];
  const e = await echo(chunks(text));
  out.push(`echo ${await collect(e)}`);
  out.push(`upper ${await collect(await upper(chunks(text)))}`);
  // A sync iterable of chunks works too; the provider stops reading after 12 bytes.
  out.push(`take ${await take(endless(), 12)}`);
  out.push(`take ${await take([new Uint8Array(3), [1, 2]], 100)}`);
  // 300 KB through `echo`: several reads of 64 KiB.
  let n = 0;
  let sum = 0;
  for await (const chunk of await echo(big())) {
    n += chunk.length;
    for (const b of chunk) {
      sum = (sum + b) % 65521;
    }
  }
  out.push(`big ${n} ${sum}`);
  const w = [];
  for await (const chunk of await words(3)) {
    w.push(chunk.join("+"));
  }
  out.push(`words ${w.join(",")}`);
  return out.join("; ");
}
