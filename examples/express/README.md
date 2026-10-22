# Notes + todo on Express 5, as a js2wasi3 component

A web app written for **Express 5.3.0**, compiled by **js2wasi3** into one WASI 0.3 component (`server.wasm`,
~5.8 MB) and run with **wasmtime**. The same `server.js` also runs under node, so you can compare the two.

It is a notes + todo manager:

- **Todos**: priority (a coloured dot), optional due date (overdue ones turn red), All / Open / Done filter, "Clear
  done", delete with an Undo toast, a progress bar.
- **Notes**: coloured sticky-note cards (6 colours), pin, change colour, delete with Undo, a "New note" dialog.
- Search across both lists, keyboard shortcuts (`/` search, `t` new todo, `n` new note), light and dark theme
  (follows the system, remembered), responsive layout, no outside libraries or CDN files (it works offline).
- A badge shows the runtime (`$APP_RUNTIME`, see below) and the header cards show requests served and uptime.
- A JSON API behind it: `/api/todos`, `/api/notes` (with `?q=` search and `?filter=open|done`), `/api/stats`.

Files:

- `server.js`: the app. Notes and todos in memory, saved to `data/db.json`; a first run starts with a few samples.
- `public/index.html`: the page (HTML, CSS and plain JavaScript in one file).
- `shims/`: the one change Express's dependencies need under js2wasi3 (see "The shim").
- `smoke.sh`: a fixed series of 14 requests against a running server, for comparing node and the component.

## What you need

| tool | version | check |
|---|---|---|
| js2wasi3 | this repository, built: `cargo build --release` in the repository root | `target/release/js2wasi3 --help` |
| wasmtime | 50.0.0 (the pinned test host; see `TOOLS.md`) | `wasmtime --version` |
| node + npm | 24 (only to install Express and to compare) | `node --version` |

## 1. Get Express and apply the shim

```sh
cd examples/express
npm run setup          # = npm install --ignore-scripts && sh shims/apply.sh
```

`npm install` fetches Express 5.3.0 and its 64 dependencies into `node_modules/` (`--ignore-scripts`: nothing from
the packages runs at install). `shims/apply.sh` then replaces one file, `node_modules/depd/index.js`, with
`shims/depd.js`. Run it again after every fresh `npm install`.

## 2. Build the component

```sh
../../target/release/js2wasi3 build -o server.wasm server.js      # about 25 s
```

js2wasi3 compiles `server.js` and every module it reaches in `node_modules/` ahead of time. Nothing is looked up
at run time.

## 3. Run it

```sh
wasmtime run -Sp3 -S inherit-network --dir "$PWD" server.wasm 3300
```

Open **http://localhost:3300/** (or http://127.0.0.1:3300/). The port is an argument (`3300` above); without it the
app uses `$PORT`, then 3300. **Changing the port needs no rebuild**: it is read when the program starts
(`--env PORT=4000` works too). To label the page's badge, add `--env "APP_RUNTIME=js2wasi3 · WASI 0.3 · wasmtime 50"`
(without it the badge shows `node <version>`, also in the component, because js2wasi3 reports a node-compatible
`process.version`: set the label when you run the component).

- `-Sp3`: WASI 0.3 (what js2wasi3 targets). `-S inherit-network`: lets the component open sockets.
- `--dir "$PWD"`: gives the component this folder **at the same path it was built in**. The app reads `public/`
  and writes `data/db.json` through it. Run the command from `examples/express`; if you move the `.wasm` to
  another folder, rebuild there (the build records the paths of the files it read).
- Stop it with Ctrl-C. Your notes and todos are in `data/db.json` and survive a restart.

## The same app under node

```sh
node server.js 3300
```

## Check that both give the same answers

```sh
rm -rf data; node server.js 3300 &  NODE_PID=$!         # or start the component in another terminal
sleep 1; ./smoke.sh http://127.0.0.1:3300 > node.txt; kill $NODE_PID

rm -rf data; wasmtime run -Sp3 -S inherit-network --dir "$PWD" server.wasm 3300 &  WASM_PID=$!
sleep 3; ./smoke.sh http://127.0.0.1:3300 > wasm.txt; kill $WASM_PID

diff node.txt wasm.txt && echo identical
```

`smoke.sh` creates todos and a note, patches and deletes some, searches and filters, requests an unknown route and
fetches the page, and drops what changes from run to run (`Date`, `ETag`, uptime, timestamps). Start both with the
same `APP_RUNTIME` value. On 2026-10-21 all 14 responses were identical.

## The API

```sh
curl -s localhost:3300/api/todos -H 'content-type: application/json' -d '{"title":"try js2wasi3"}'
curl -s localhost:3300/api/notes -H 'content-type: application/json' -d '{"title":"idea","text":"Express on WASI"}'
curl -s localhost:3300/api/todos                           # list
curl -s -X PATCH localhost:3300/api/todos/1 -H 'content-type: application/json' -d '{"done":true}'
curl -s -X DELETE localhost:3300/api/notes/2
curl -s localhost:3300/api/stats
```

If `curl` goes through a proxy on your machine, add `--noproxy '*'`.

## The shim

Express depends on `depd`, which prints deprecation warnings. To find the caller it uses **V8's call-site API**
(`Error.prepareStackTrace` with `CallSite` objects) and, in places, builds functions with `new Function` from a
string it computes at run time. js2wasi3 compiles ahead of time and keeps no structured stack frames, so it has
neither. `shims/depd.js` keeps depd's interface and prints no deprecation warnings. Nothing else is changed: Express
and its other 63 dependencies run as they are.

Other gaps this app used to need shims for are fixed in js2wasi3 itself (T148): reading `x.__proto__` (the
`Object.prototype.__proto__` getter), using a function as a property key (`mime-types` does), `Function(...)` of
code only known at run time (it builds and throws an `EvalError` if it ever runs; `function-bind` has such a branch
that node never takes), `listen(port)` without a host now accepts IPv4 as well as IPv6 clients, and `fs.stat` times
keep their fraction and round like node's (Express's `send` builds its `ETag` from them).

## What does not work (yet)

Found by running Express's own example apps (2026-10-21):

- **`morgan`** (the request logger) and **`ejs`** (templates) build code with `new Function` at run time; js2wasi3
  cannot compile that, so apps using them fail on the first request. This app logs with a one-line middleware.
- **`crypto.pbkdf2`** (`pbkdf2-password`, used by Express's `auth` example) is computed by a Rust helper component (T149): after
  `build`, run `../../scripts/link-crypto.sh server.wasm` (it composes `crates/js2wasi3/assets/crypto-helper.wasm` into the program).
  The npm package `pbkdf2-password` 1.2.1 gives node's results.
- A computed `require(name)` finds only modules known at build time; list others with `js2wasi3 build --include '<glob>'`.
- Error pages in development mode show an `err.stack` with one frame, not node's list.

## Troubleshooting

- **Port 3000 already in use / `502 Bad Gateway`**: something else listens there (a dev proxy, for example). Pick another port.
- **`ERR_CONNECTION_REFUSED`**: the server is not running (the program ends when you stop it), or you used another port.
- **`Cannot find module ...`** while building: run `npm run setup` first.
- **Page is `{"error":"no route","path":"/"}`**: the component did not get the folder; use `--dir "$PWD"` from this directory.
- **The notes disappear**: they are in `data/db.json` of the folder passed with `--dir`.
