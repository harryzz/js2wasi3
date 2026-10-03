// T72 step 1: wasi:http@0.3.0 from JS directly. Usage: probe.js <scheme> <authority> <path>
import {
  "[constructor]fields" as newFields,
  "[static]fields.from-list" as fromList,
  "[method]fields.copy-all" as copyAll,
  "[static]request.new" as newRequest,
  "[method]request.set-method" as setMethod,
  "[method]request.set-scheme" as setScheme,
  "[method]request.set-authority" as setAuthority,
  "[method]request.set-path-with-query" as setPath,
  "[method]response.get-status-code" as status,
  "[method]response.get-headers" as getHeaders,
  "[static]response.consume-body" as consumeBody,
} from "wasi:http/types@0.3.0";
import { send } from "wasi:http/client@0.3.0";

const [scheme, authority, path] = [process.argv[2] ?? "HTTP", process.argv[3] ?? "127.0.0.1:8099", process.argv[4] ?? "/"];
const enc = new TextEncoder();
const headers = fromList([["user-agent", enc.encode("js2wasi3-probe")]]);
const [req, done] = newRequest(headers, undefined, Promise.resolve({ tag: "ok", val: undefined }), undefined);
setMethod(req, { tag: "get" });
setScheme(req, { tag: scheme });
setAuthority(req, authority);
setPath(req, path);
try {
  const res = await send(req);
  console.log("status", status(res));
  const hs = copyAll(getHeaders(res));
  const dec = new TextDecoder();
  for (const [k, v] of hs) if (k !== "date") console.log(k, dec.decode(v));
  const [body, trailers] = consumeBody(res, Promise.resolve({ tag: "ok", val: undefined }));
  let n = 0, text = "";
  for await (const chunk of body) { n += chunk.length; text += dec.decode(chunk, { stream: true }); }
  console.log("body bytes", n, JSON.stringify(text.slice(0, 80)));
  console.log("trailers", await trailers);
  console.log("done", await done);
} catch (e) {
  console.log("error", e.message, JSON.stringify(e.payload));
}
