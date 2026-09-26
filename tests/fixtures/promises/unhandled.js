// An unhandled rejection ends the program after the microtasks ran.
console.log("start");
Promise.reject(new TypeError("bad"));
Promise.resolve().then(() => console.log("microtask still runs"));
