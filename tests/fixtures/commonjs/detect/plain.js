// No ESM syntax and no "type": CommonJS, so `this` is module.exports.
module.exports = { plain: this === exports };
