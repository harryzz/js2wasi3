// The zstd helper component's `codec` resource (T150 slice 3): libzstd behind node's zstd handles.
// Linked only into programs that name `zstd` (`link::link`).
import {
  "[static]codec.create" as $create,
  "[method]codec.process" as $process,
  "[method]codec.reset" as $reset,
  "[resource-drop]codec" as $drop,
} from "js2wasi3:zstd/codecs@0.1.0";

export const create = $create;
export const process = $process;
export const reset = $reset;
export const drop = $drop;
