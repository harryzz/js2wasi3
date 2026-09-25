# T20: WIT async and user worlds

Status: todo | Depends on: T19

## Goal
Accept a user `.wit` world. Map exported and imported functions with D4's conventions, including `async func` ↔ async JS functions,
`future<T>` ↔ Promise, `stream<u8>` ↔ async iterable of `Uint8Array`, records, variants, options, results and lists.
Imported interfaces become ES imports (`import { f } from "pkg:ns/iface"`).

## Acceptance
- A library world with 1 async export and 1 async import, composed with a test component via `wac`, runs on wasmtime.
- A stream export echoes input bytes.

## Notes / Result
