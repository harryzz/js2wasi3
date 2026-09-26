// An unhandled Promise.any rejection: node prints the AggregateError without a stack.
console.log("start");
Promise.any([Promise.reject(1), Promise.reject("two")]);
