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
