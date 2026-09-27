// Implements interface `acme:futures/later`: futures as results (written when their promise
// settles) and as a parameter (a promise of the value the app writes).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const later = {
  async doubleLater(n) {
    return { now: n, later: sleep(20).then(() => 2 * n) };
  },
  async shout(s) {
    return (await s).toUpperCase();
  },
  async origin() {
    return { x: 1, y: -2 };
  },
  async done() {
    await sleep(1);
  },
};
