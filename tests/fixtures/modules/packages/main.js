// Packages (T41 slice 5): bare specifiers resolved at build time as node does: `node_modules`
// upwards from the importer, `exports` (conditions, patterns, `null`), `imports` (`#name`), a
// package's own nested `node_modules`, legacy `main`, JSON through `exports`.
import { area } from "shapes";
import { circle } from "shapes/kinds/circle";
import red, { names } from "@acme/colors";
import legacy from "legacy";
import { unit } from "#config";
import { square } from "#shapes/kinds/square";
import meta from "shapes/package.json" with { type: "json" };
import { version as utilVersion } from "util-lib";

console.log(area(2), circle(1).toFixed(3), square(3), unit);
console.log(red, names, legacy, meta.name, utilVersion);

// `import()` of a package: resolved at build time too; errors reject with node's codes.
const colors = await import("@acme/colors");
console.log(colors.default === red);
for (const p of [import("shapes/internal/secret"), import("no-such-package")]) {
  try {
    await p;
  } catch (e) {
    console.log(e.code, e.message.split(" imported from")[0].replaceAll(import.meta.dirname, "DIR"));
  }
}
