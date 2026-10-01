// T59: streams and futures of blocking calls: a returned `stream<u8>` is read to its end (one
// `Uint8Array`), a returned future read to its value, a `Uint8Array` given as a `stream<u8>` written after the
// call. Run with a preopened directory holding `hello.txt` ("hello file\n").
import { getDirectories } from "wasi:filesystem/preopens@0.3.1";
import {
  "[method]descriptor.open-at" as openAt,
  "[method]descriptor.read-via-stream" as readViaStream,
  "[method]descriptor.write-via-stream" as writeViaStream,
  "[method]descriptor.read-directory" as readDirectory,
  "[method]descriptor.unlink-file-at" as unlinkAt,
  "[resource-drop]descriptor" as drop,
} from "wasi:filesystem/types@0.3.1";
const root = getDirectories()[0][0];
const fd = openAt(root, {}, "hello.txt", {}, { read: true });
const [bytes, status] = readViaStream(fd, 0);
console.log(bytes, status);
drop(fd);
const out = openAt(root, {}, "out.txt", { create: true, truncate: true }, { write: true });
const data = new Uint8Array(200000).fill(65);
console.log(writeViaStream(out, data, 0));
drop(out);
const back = openAt(root, {}, "out.txt", {}, { read: true });
const [b2] = readViaStream(back, 0);
console.log(b2.length, b2[0], b2[199999]);
drop(back);
const [entries, st] = readDirectory(root);
console.log(entries.map((e) => [e.type.tag, e.name]).sort(), st);
unlinkAt(root, "out.txt");
