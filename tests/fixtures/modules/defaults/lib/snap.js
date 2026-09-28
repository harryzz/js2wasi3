export let live = 1;
// `export default <expr>` exports the value, not the binding.
export default live;
export function bump() {
  live += 10;
}
class Node {
  static make() {
    return new Node();
  }
}
console.log(Node.make() instanceof Node, Node.name);
