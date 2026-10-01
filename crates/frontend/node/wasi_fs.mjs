// `wasi:filesystem` for `node:fs` (T59): the imports `fs.js` uses. In a command, each call blocks
// until it returns; a returned `stream<u8>` is read to its end (a `Uint8Array`), a returned future
// to its value; a `Uint8Array` passed as a `stream<u8>` is written after the call. Handles are
// numbers; errors are thrown with `payload` an `error-code` (`{ tag }`).
import { getDirectories as $getDirectories } from "wasi:filesystem/preopens@0.3.1";
import {
  "[method]descriptor.read-via-stream" as $readViaStream,
  "[method]descriptor.write-via-stream" as $writeViaStream,
  "[method]descriptor.append-via-stream" as $appendViaStream,
  "[method]descriptor.sync-data" as $syncData,
  "[method]descriptor.sync" as $sync,
  "[method]descriptor.set-size" as $setSize,
  "[method]descriptor.read-directory" as $readDirectory,
  "[method]descriptor.create-directory-at" as $createDirectoryAt,
  "[method]descriptor.stat" as $stat,
  "[method]descriptor.stat-at" as $statAt,
  "[method]descriptor.set-times-at" as $setTimesAt,
  "[method]descriptor.link-at" as $linkAt,
  "[method]descriptor.open-at" as $openAt,
  "[method]descriptor.readlink-at" as $readlinkAt,
  "[method]descriptor.remove-directory-at" as $removeDirectoryAt,
  "[method]descriptor.rename-at" as $renameAt,
  "[method]descriptor.symlink-at" as $symlinkAt,
  "[method]descriptor.unlink-file-at" as $unlinkFileAt,
  "[resource-drop]descriptor" as $drop,
} from "wasi:filesystem/types@0.3.1";

// (`export { x }` of an imported WIT binding isn't linked yet: each one is re-bound.)
export const getDirectories = $getDirectories;
export const readViaStream = $readViaStream;
export const writeViaStream = $writeViaStream;
export const appendViaStream = $appendViaStream;
export const syncData = $syncData;
export const sync = $sync;
export const setSize = $setSize;
export const readDirectory = $readDirectory;
export const createDirectoryAt = $createDirectoryAt;
export const stat = $stat;
export const statAt = $statAt;
export const setTimesAt = $setTimesAt;
export const linkAt = $linkAt;
export const openAt = $openAt;
export const readlinkAt = $readlinkAt;
export const removeDirectoryAt = $removeDirectoryAt;
export const renameAt = $renameAt;
export const symlinkAt = $symlinkAt;
export const unlinkFileAt = $unlinkFileAt;
export const drop = $drop;
