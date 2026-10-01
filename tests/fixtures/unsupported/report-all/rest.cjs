// T62: a rest parameter with `arguments` is a diagnostic (was a panic when the function is a value).
class Logger {
  log(level, ...rest) {
    return arguments.length + rest.length;
  }
}
console.log(new Logger().log(1, 2));
