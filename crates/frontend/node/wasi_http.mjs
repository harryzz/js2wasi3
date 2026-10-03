// `wasi:http@0.3.0` for `fetch` (T72). Non-blocking in a command like `wasi:sockets` (T66): `send`
// returns a promise, `consume-body`'s `stream<u8>` is an async iterable of `Uint8Array`s, futures are
// promises (given ones: a promise or a value); `run`'s event loop waits for them. Handles are numbers.
import {
  "[static]fields.from-list" as $fromList,
  "[method]fields.copy-all" as $copyAll,
  "[resource-drop]fields" as $dropFields,
  "[static]request.new" as $newRequest,
  "[method]request.set-method" as $setMethod,
  "[method]request.set-scheme" as $setScheme,
  "[method]request.set-authority" as $setAuthority,
  "[method]request.set-path-with-query" as $setPathWithQuery,
  "[resource-drop]request" as $dropRequest,
  "[method]response.get-status-code" as $getStatusCode,
  "[method]response.get-headers" as $getHeaders,
  "[static]response.consume-body" as $consumeBody,
  "[resource-drop]response" as $dropResponse,
} from "wasi:http/types@0.3.0";
import { send as $send } from "wasi:http/client@0.3.0";

// (`export { x }` of an imported WIT binding isn't linked yet: each one is re-bound.)
export const fromList = $fromList;
export const copyAll = $copyAll;
export const dropFields = $dropFields;
export const newRequest = $newRequest;
export const setMethod = $setMethod;
export const setScheme = $setScheme;
export const setAuthority = $setAuthority;
export const setPathWithQuery = $setPathWithQuery;
export const dropRequest = $dropRequest;
export const getStatusCode = $getStatusCode;
export const getHeaders = $getHeaders;
export const consumeBody = $consumeBody;
export const dropResponse = $dropResponse;
export const send = $send;

// Cancel the pending `send` whose promise `p` is (an aborted fetch). One that already returned
// settles now; a cancelled one's promise never settles.
export function cancel(p) {
  __intrinsic_import_cancel(p);
}
