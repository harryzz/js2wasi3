// `wasi:sockets` for `node:net` and `dns.lookup` (T66). Non-blocking in a command: `connect` and
// `resolve-addresses` return promises, `receive`'s `stream<u8>` is an async iterable of `Uint8Array`s,
// `send` takes one, `listen`'s `stream<tcp-socket>` yields arrays of socket handles (numbers), futures
// are promises; `run`'s event loop waits for them. Errors are thrown with `payload` an `error-code`.
import {
  "[static]tcp-socket.create" as $create,
  "[method]tcp-socket.bind" as $bind,
  "[method]tcp-socket.connect" as $connect,
  "[method]tcp-socket.listen" as $listen,
  "[method]tcp-socket.send" as $send,
  "[method]tcp-socket.receive" as $receive,
  "[method]tcp-socket.get-local-address" as $getLocalAddress,
  "[method]tcp-socket.get-remote-address" as $getRemoteAddress,
  "[method]tcp-socket.set-listen-backlog-size" as $setListenBacklogSize,
  "[method]tcp-socket.set-keep-alive-enabled" as $setKeepAliveEnabled,
  "[method]tcp-socket.set-keep-alive-idle-time" as $setKeepAliveIdleTime,
  "[resource-drop]tcp-socket" as $drop,
} from "wasi:sockets/types@0.3.1";
import { resolveAddresses as $resolveAddresses } from "wasi:sockets/ip-name-lookup@0.3.1";

// (`export { x }` of an imported WIT binding isn't linked yet: each one is re-bound.)
export const create = $create;
export const bind = $bind;
export const connect = $connect;
export const listen = $listen;
export const send = $send;
export const receive = $receive;
export const getLocalAddress = $getLocalAddress;
export const getRemoteAddress = $getRemoteAddress;
export const setListenBacklogSize = $setListenBacklogSize;
export const setKeepAliveEnabled = $setKeepAliveEnabled;
export const setKeepAliveIdleTime = $setKeepAliveIdleTime;
export const drop = $drop;
export const resolveAddresses = $resolveAddresses;
