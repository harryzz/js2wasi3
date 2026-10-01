// Streams at a user world's boundary (T20 slice 6b). Conventions: crates/lower/src/builtins.rs.
// The backend lifts a received `stream<T>` (the readable end `h` of its stream type `id`) with
// `$$StreamReader(h, id)`, and lowers a value given as a `stream<T>` by making a stream and
// calling `$$StreamPump(value, w, id)` with its writable end. Chunks are shaped like `list<T>`
// (`stream<u8>`: `Uint8Array`s). The copies are native (`__intrinsic_stream_*`, see
// `ir::StreamOp` and crates/backend/src/abi.rs).

// The stream's chunks, one per read, until the writer drops its end. Leaving early (`break`,
// `return()`) drops the readable end, so the writer sees the stream end.
async function* $$StreamReader(h, id) {
  try {
    let ended = false;
    while (!ended) {
      const r = await __intrinsic_stream_read(h, id);
      ended = r[1];
      if (r[0] !== undefined) {
        yield r[0];
      }
    }
  } finally {
    __intrinsic_stream_drop_readable(h, id);
  }
}

// Write each chunk of the (async) iterable `src` to the writable end `w`, then drop it (the
// stream ends). Stops when the reader drops its end. An error of `src` ends the stream too and
// rejects the pump's promise (nobody handles it: the call fails, like node on an unhandled
// rejection).
async function $$StreamPump(src, w, id) {
  try {
    // Start in a job, not in the middle of lowering a value.
    await undefined;
    for await (const chunk of src) {
      if (!(await __intrinsic_stream_write(chunk, w, id))) {
        break;
      }
    }
  } finally {
    __intrinsic_stream_drop_writable(w, id);
  }
}

// A command's blocking WASI imports (T59): a returned stream is read to its end, one value shaped
// like `list<T>` (all of a `stream<u8>`'s bytes in one `Uint8Array`).
function $$StreamReadAll(h, id) {
  const chunks = [];
  let total = 0;
  let ended = false;
  while (!ended) {
    const r = __intrinsic_stream_read_sync(h, id);
    ended = r[1];
    if (r[0] !== undefined) {
      chunks.push(r[0]);
      total += r[0].length;
    }
  }
  __intrinsic_stream_drop_readable(h, id);
  if (chunks.length === 1) {
    return chunks[0];
  }
  // (Streams of other numeric types: chunks are other typed arrays, not joined yet.)
  if (chunks.length === 0 || chunks[0] instanceof Uint8Array) {
    const all = new Uint8Array(total);
    let at = 0;
    for (const c of chunks) {
      all.set(c, at);
      at += c.length;
    }
    return all;
  }
  return [].concat(...chunks);
}

// A stream argument of a blocking call: written (`v`, one chunk) after the call returns.
// (Made on first use: the prelude's top-level values are literals.)
let $$streamWrites;
function $$StreamDefer(v, w, id) {
  if ($$streamWrites === undefined) {
    $$streamWrites = [];
  }
  $$streamWrites.push(v, w, id);
}

function $$StreamFlush() {
  if ($$streamWrites === undefined) {
    return;
  }
  for (let i = 0; i < $$streamWrites.length; i += 3) {
    const v = $$streamWrites[i];
    const w = $$streamWrites[i + 1];
    const id = $$streamWrites[i + 2];
    __intrinsic_stream_write_sync(v, w, id);
    __intrinsic_stream_drop_writable(w, id);
  }
  $$streamWrites.length = 0;
}
