#!/usr/bin/env bash
# M0 feasibility probes. Tools: wasmtime 49.0.2, wasm-tools 1.259.0, wkg 0.16.1, WASI 0.3.1
set -e; cd "$(dirname "$0")"; mkdir -p out
echo "1) GC + EH + async-lifted export";      wasm-tools parse p.wat -o out/p.wasm && wasmtime run --invoke 'run()' out/p.wasm
echo "2) P3 command: stream stdout + timer + GC/EH (callback loop)"
wasm-tools parse cmd.wat -o out/cmd.core.wasm && wasm-tools component embed wit out/cmd.core.wasm -o out/cmd.emb.wasm && wasm-tools component new out/cmd.emb.wasm -o out/cmd.wasm && wasmtime run -Sp3 out/cmd.wasm
echo "3) GC canonical ABI option (scalar ok, string NOT implemented)"
wasm-tools parse gcabi.wat -o out/gcabi.wasm && wasmtime run -W component-model-gc=y --invoke 'f(5)' out/gcabi.wasm
wasm-tools parse gcstr.wat -o out/gcstr.wasm && (wasmtime run -W component-model-gc=y --invoke 'f()' out/gcstr.wasm || echo "   expected failure")
echo "4) stack switching generator (core) with EH+GC"
wasm-tools parse ss.wat -o out/ss.wasm && wasmtime run -W stack-switching=y --invoke main out/ss.wasm 2>/dev/null
echo "5) direct-style async: continuation parked across P3 WAIT/callback"
wasm-tools parse cmd2.wat -o out/cmd2.core.wasm && wasm-tools component embed wit out/cmd2.core.wasm -o out/cmd2.emb.wasm && wasm-tools component new out/cmd2.emb.wasm -o out/cmd2.wasm && wasmtime run -W stack-switching=y -Sp3 out/cmd2.wasm
echo "6) Component Model threads (🧵) as a generator: expect 60"
wasm-tools parse cmthread.wat -o out/cmthread.wasm && wasmtime run -W component-model-async-stackful=y,component-model-threading=y --invoke 'run()' out/cmthread.wasm
echo "7) 1M coroutine round trips: CM threads vs stack switching"
wasm-tools parse bench_cm.wat -o out/bench_cm.wasm && wasm-tools parse bench_ss.wat -o out/bench_ss.wasm
time wasmtime run -W component-model-async-stackful=y,component-model-threading=y --invoke 'run()' out/bench_cm.wasm
time wasmtime run -W stack-switching=y --invoke main out/bench_ss.wasm 2>/dev/null
echo "8) timers: two concurrent wait-until (50 ms) + a cancelled 5 s one (expect ~50 ms, exit 0)"
wasm-tools parse timers.wat -o out/timers.core.wasm && wasm-tools component embed wit out/timers.core.wasm -o out/timers.emb.wasm && wasm-tools component new out/timers.emb.wasm -o out/timers.wasm && time wasmtime run -Sp3 out/timers.wasm
echo "9) blocking filesystem calls from a callback-lifted run (T59): expect 'hello from a preopened file'"
mkdir -p out/fsdir && printf 'hello from a preopened file\n' > out/fsdir/hello.txt
wasm-tools parse fs.wat -o out/fs.core.wasm && wasm-tools component embed wit --world cmdfs out/fs.core.wasm -o out/fs.emb.wasm && wasm-tools component new out/fs.emb.wasm -o out/fs.wasm && wasmtime run -Sp3 --dir out/fsdir::/ out/fs.wasm
echo "10) T89 (F350): 50M calls through a funcref in a GC struct (call_ref) vs a table index (call_indirect)"
wasm-tools parse funcref.wat -o out/funcref.wasm
time wasmtime run --invoke run_struct out/funcref.wasm 2>/dev/null
time wasmtime run --invoke run_table out/funcref.wasm 2>/dev/null
