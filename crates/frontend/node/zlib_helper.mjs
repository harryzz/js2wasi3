// The zlib helper component's `codec` resource (T150): Rust codecs (zlib, later Brotli and zstd) behind node's native
// binding. Linked only into programs that name `zlib` (`link::link`).
import {
  "[static]codec.create" as $create,
  "[method]codec.process" as $process,
  "[method]codec.params" as $params,
  "[method]codec.reset" as $reset,
  "[resource-drop]codec" as $drop,
} from "js2wasi3:zlib/codecs@0.1.0";

export const create = $create;
export const process = $process;
export const params = $params;
export const reset = $reset;
export const drop = $drop;
