// A computed name hides pbkdf2 from the build; --ext-lib crypto links the helper anyway (T149).
import crypto from 'node:crypto';
const k = crypto['pbkdf' + 2 + 'Sync']('p', 's', 1, 8, 'sha1');
console.log(k.toString('hex'));
