// Implements interface `acme:streams/bytes`: streams as parameters (async iterables of
// `Uint8Array`s) and results (any async iterable of chunks, pumped into the stream).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function* upperChunks(input) {
  for await (const chunk of input) {
    yield chunk.map((b) => (b >= 97 && b <= 122 ? b - 32 : b));
  }
}

async function* wordList(n) {
  for (let i = 0; i < n; i++) {
    await sleep(2);
    yield [`w${i}`];
  }
}

export const bytes = {
  async echo(input) {
    return input;
  },
  async upper(input) {
    return upperChunks(input);
  },
  async take(input, n) {
    // Chunks are what each read got: a stream keeps no write boundaries.
    let seen = 0;
    for await (const chunk of input) {
      seen += chunk.length;
      if (seen >= n) {
        break;
      }
    }
    return Math.min(seen, n);
  },
  async words(n) {
    return wordList(n);
  },
};
