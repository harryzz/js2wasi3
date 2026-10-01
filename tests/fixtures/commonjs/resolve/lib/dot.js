// `.` and `..` are relative, and directories (T62: `@serialport/bindings-cpp`'s `require(".")`).
module.exports = [require('.') === require('./index.js'), require('../lib/.') === require('.')];
