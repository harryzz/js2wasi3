// Default exports in every form, statements relying on ASI at the ends of modules, a module
// shadowing a global another module uses, and one module reached through two paths.
import K, { default as K2, v as def, JSONish } from './a.js'
import obj from './b.js'
import C from './c.js'
import snap, { bump, live } from './lib/snap.js'
import { count } from './lib/../lib/once.js'
import './lib/uses-once.js'
(function () { console.log('iife after imports') })()
console.log(K === K2, K.name, def, JSONish, JSON.stringify({ a: 1 }))
console.log(obj, typeof C, C.name, new C().hi())
bump()
console.log(snap, live, count)
