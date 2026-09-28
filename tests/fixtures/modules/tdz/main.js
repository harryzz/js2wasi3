// A binding read before its module ran: a ReferenceError naming the importer's local name.
import './a.js';
import { late as renamed } from './b.js';

console.log(renamed);
