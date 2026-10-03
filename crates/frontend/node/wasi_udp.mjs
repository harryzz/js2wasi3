// `wasi:sockets` UDP for `node:dgram` (T67), apart from `node:internal/wasi_sockets` so that programs
// using only `net` or `dns` don't import it. Non-blocking in a command: `send` and `receive` return
// promises. Errors are thrown with `payload` an `error-code`.
import {
  "[static]udp-socket.create" as $create,
  "[method]udp-socket.bind" as $bind,
  "[method]udp-socket.connect" as $connect,
  "[method]udp-socket.disconnect" as $disconnect,
  "[method]udp-socket.send" as $send,
  "[method]udp-socket.receive" as $receive,
  "[method]udp-socket.get-local-address" as $getLocalAddress,
  "[method]udp-socket.get-remote-address" as $getRemoteAddress,
  "[method]udp-socket.set-unicast-hop-limit" as $setUnicastHopLimit,
  "[resource-drop]udp-socket" as $drop,
} from "wasi:sockets/types@0.3.1";

// (`export { x }` of an imported WIT binding isn't linked yet: each one is re-bound.)
export const create = $create;
export const bind = $bind;
export const connect = $connect;
export const disconnect = $disconnect;
export const send = $send;
export const receive = $receive;
export const getLocalAddress = $getLocalAddress;
export const getRemoteAddress = $getRemoteAddress;
export const setUnicastHopLimit = $setUnicastHopLimit;
export const drop = $drop;

// Cancel the pending call whose promise `p` is (a `receive` when its socket closes: the socket
// can't be dropped while a call borrows it). A call that already returned settles now; a cancelled
// one's promise never settles.
export function cancel(p) {
  __intrinsic_import_cancel(p);
}
