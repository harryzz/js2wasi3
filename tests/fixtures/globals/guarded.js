// T63: node globals js2wasi3 lacks, where they may not run, and statics neither has.

// tsc's decorator helper: `Reflect.decorate` is absent in node too, so it takes the loop.
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(r)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
const seen = [];
const log = (name) => (target, key, desc) => { seen.push(`${name}:${key}`); return desc; };
class Service {
  start() { return "started"; }
}
__decorate([log("a"), log("b")], Service.prototype, "start", null);
console.log(seen, new Service().start());

// Functions that would use a missing global, never called.
function userAgent() { return navigator.userAgent; }
function parse(s) { return new URL(s).host; }
const download = async (u) => (await fetch(u)).text();
function blob(parts) { return new Blob(parts); }
console.log(typeof userAgent, typeof parse, typeof download, typeof blob);

// In branches that don't run.
if (false) console.log(navigator.language);
const never = false && new TextDecoder().decode(new Uint8Array());
console.log(never);

// A static neither has: node's TypeError when it's called.
function sum(xs) { return Math.sumPrecise2(xs); }
try {
  sum([1, 2]);
} catch (e) {
  console.log(e instanceof TypeError, e.message);
}
const kinds = () => [typeof Reflect.decorate, typeof Math.sumPrecise2];
console.log(kinds());
