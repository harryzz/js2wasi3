"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Color = exports.PI_ISH = void 0;
exports.area = area;
exports.PI_ISH = 3.14;
function area(s) {
    return s.kind === 'square' ? s.size * s.size : exports.PI_ISH * s.r * s.r;
}
var Color;
(function (Color) {
    Color[Color["Red"] = 0] = "Red";
    Color[Color["Green"] = 1] = "Green";
})(Color || (exports.Color = Color = {}));
