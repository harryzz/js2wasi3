// Reduce node's stderr for an uncaught exception to what a js2wasi3 command prints:
// the output before it, then "Uncaught <message>" (see `x.uncaught` in scripts/diff.sh).
//
// node prints the exception as
//   [output so far]
//   [blank line, for a non-error value]
//   <file:///x.js:N | node:internal/...:N>
//   <source line>
//   <caret line>
//   [blank line]
//   <message lines>
//   <"    at ..." stack lines | "(Use `node --trace-uncaught ...`" | blank line>
// Usage: node node-uncaught.mjs <node stderr file>  (writes the reduced text to stdout)
import { readFileSync } from "node:fs";

const lines = readFileSync(process.argv[2], "utf8").split("\n");
const header = lines.findIndex((l) => /^(file:\/\/|node:internal\/).*:\d+$/.test(l));
if (header < 0) {
  process.stdout.write(lines.join("\n"));
  process.exit(0);
}
let before = lines.slice(0, header);
if (lines[header].startsWith("node:internal/") && before.at(-1) === "") before.pop();
let i = header + 1;
while (i < lines.length && !/^\s*\^+\s*$/.test(lines[i])) i++;
i++;
if (lines[i] === "") i++;
const message = [];
for (; i < lines.length; i++) {
  const l = lines[i];
  if (l === "" || l.startsWith("    at ") || l.startsWith("(Use `node --trace-uncaught")) break;
  message.push(l);
}
const out = [...before, "Uncaught " + message.join("\n"), ""];
process.stdout.write(out.join("\n"));
