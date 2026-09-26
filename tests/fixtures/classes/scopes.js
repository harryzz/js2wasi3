// Classes in loops capture per-iteration bindings; methods see later-assigned outer variables.
const made = [];
for (let i = 0; i < 3; i++) {
  class K {
    static id = i;
    get() {
      return i * 10 + K.id;
    }
  }
  made.push(new K());
}
console.log(made[0].get(), made[1].get(), made[2].get());

function factory() {
  let greeting = "hi";
  class G {
    greet(name) {
      return `${greeting}, ${name}`;
    }
  }
  const g = new G();
  greeting = "hello";
  return g;
}
console.log(factory().greet("x"));

// A top-level class used by a function declared before it.
function build() {
  return new Later(2).double();
}
class Later {
  constructor(n) {
    this.n = n;
  }
  double() {
    return this.n * 2;
  }
}
console.log(build());

// TDZ: using a class before its declaration.
try {
  new Early();
} catch (e) {
  console.log(e.name, e.message);
}
class Early {}

// `super` in object literal methods and accessors.
const base = {
  hello() {
    return "base hello";
  },
  get who() {
    return "base";
  },
};
const derived = {
  __proto__: base,
  hello() {
    return super.hello() + " + derived";
  },
  get who() {
    return "derived of " + super.who;
  },
};
console.log(derived.hello(), derived.who);

// Static inheritance and `this` in static methods.
class Registry {
  static items = [];
  static add(x) {
    this.items.push(x);
    return this;
  }
  static create() {
    return new this();
  }
}
class SubRegistry extends Registry {
  static items = [];
}
Registry.add(1).add(2);
SubRegistry.add(3);
console.log(Registry.items.length, SubRegistry.items.length, SubRegistry.create() instanceof SubRegistry);

// Accessors inherited through classes, overriding and super getters.
class Temp {
  #c = 0;
  get celsius() {
    return this.#c;
  }
  set celsius(v) {
    this.#c = v;
  }
  get fahrenheit() {
    return this.#c * 1.8 + 32;
  }
}
class LoggedTemp extends Temp {
  set celsius(v) {
    console.log("set", v);
    super.celsius = v;
  }
  get celsius() {
    return super.celsius;
  }
}
const lt = new LoggedTemp();
lt.celsius = 25;
console.log(lt.celsius, lt.fahrenheit);
