// Inside a function (as in a CommonJS module body, T54): closures made before a destructured
// `const` they capture (TDZ until it runs), and a named class expression referring to itself.
function load() {
  class Comparator {
    test(v) {
      return re.num.test(v) && v.length <= max;
    }
  }
  const early = () => {
    try {
      return new Comparator().test('1');
    } catch (e) {
      return e.constructor.name + ': ' + e.message;
    }
  };
  const before = early();
  const { safeRe: re, limits: [max = 3] } = { safeRe: { num: /^\d+$/ }, limits: [] };
  return [before, new Comparator().test('12'), new Comparator().test('1234'), early()];
}
console.log(load());

function bundle() {
  var Schema = class Schema {
    constructor(tags) {
      this.tags = tags;
      this.self = Schema;
    }
    withTags(...more) {
      return new Schema([...this.tags, ...more]);
    }
    static empty = () => new Schema([]);
  };
  const s = Schema.empty().withTags('a').withTags('b', 'c');
  return [s.tags, s instanceof Schema, s.self === Schema, Schema.name];
}
console.log(bundle());
