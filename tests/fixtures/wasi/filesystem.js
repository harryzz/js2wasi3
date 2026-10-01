// T59: resources as handles (numbers), `async func`s blocking in a command. Run with a preopened directory
// holding `hello.txt` ("hello file\n").
import { getDirectories } from "wasi:filesystem/preopens@0.3.1";
import {
  "[method]descriptor.stat-at" as statAt,
  "[method]descriptor.open-at" as openAt,
  "[method]descriptor.stat" as stat,
  "[method]descriptor.create-directory-at" as mkdirAt,
  "[method]descriptor.remove-directory-at" as rmdirAt,
  "[resource-drop]descriptor" as drop,
} from "wasi:filesystem/types@0.3.1";

const dirs = getDirectories();
console.log(dirs.length, typeof dirs[0][0], dirs[0][1]);
const root = dirs[0][0];
const st = statAt(root, { symlinkFollow: true }, "hello.txt");
console.log(st.type, st.size, st.linkCount, typeof st.dataModificationTimestamp);
try {
  statAt(root, {}, "missing.txt");
} catch (e) {
  console.log("stat missing:", e.payload);
}
const fd = openAt(root, {}, "hello.txt", {}, { read: true });
console.log(typeof fd, stat(fd).size);
drop(fd);
mkdirAt(root, "sub");
try {
  mkdirAt(root, "sub");
} catch (e) {
  console.log("mkdir twice:", e.payload);
}
rmdirAt(root, "sub");
console.log("done");
