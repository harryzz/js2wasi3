#!/bin/bash
# docs/performance.md, "Same programs under scriptc": one source file per program for node, scriptc 0.2.7 (native and
# wasm32-wasi) and js2wasi3, and the builds. Paths as on the test machine: ~/t101-bench (a scripts/bench.sh -o run),
# ~/t103-sw/src (scriptc commit 1bf547a: benchmarks/runtime/workloads/*.ts + workloads.json), ~/scriptc-0.2.7 (x/ = the
# linux-x64-gnu release, wasi/package = @scriptc/runtime-wasm32-wasi 0.2.7), zig 0.17.0 in ~/.local. Run run.sh next.
set -u
export PATH=~/.local/zig-x86_64-linux-0.17.0:$PATH
D=~/t104; rm -rf $D; mkdir -p $D/a $D/b; S=~/scriptc-0.2.7/x/bin/scriptc J=~/js2wasi3/target/release/js2wasi3
cp ~/t101-bench/fasta-*.fa $D/a/; echo '{"type":"commonjs"}' > $D/a/package.json
# Set A part 1: the benchmarks, as scripts/bench.sh builds them for js2wasi3, plus scriptc's two patches
for n in binarytrees fannkuchredux fasta knucleotide mandelbrot nbody pidigits regexredux revcomp spectralnorm \
         octane-richards octane-deltablue octane-raytrace octane-navier-stokes octane-crypto octane-splay octane-earley-boyer octane-regexp; do
  f=$D/a/$n.js; cp ~/t101-bench/$n.js $f
  case $n in octane-*) sed -i '1i var window;' $f ;; esac
  perl -pi -e 's/\+process\.argv\[(\d)\]/Number(process.argv[$1])/g; s/\+process\.argv\.length/process.argv.length/g; s/\+\(process\.argv\[(\d)\] \|\| (\d+)\)/Number(process.argv[$1] || $2)/g' $f
done
# Set A part 2 and set B: scriptc's workloads; .mjs = the .ts with node's type stripping (same file for everyone in A)
mkdir -p $D/b; cp ~/t103-sw/src/*.ts $D/b/; echo '{"type":"module"}' > $D/b/package.json
cd $D/b && node -e 'const m=require("node:module"),fs=require("fs");for(const f of process.argv.slice(1))fs.writeFileSync("../a/"+f.replace(/\.ts$/,".mjs"),m.stripTypeScriptTypes(fs.readFileSync(f,"utf8")))' *.ts 2>/dev/null
sha256sum $D/a/* $D/b/*.ts > $D/SOURCES.sha256
# Builds: every compiler from the same file
build() { # dir file base
  cd $1; local f=$2 n=$3; local r=$n
  $J build $f -o $n.j.wasm >$n.j.log 2>&1 && wasmtime compile $n.j.wasm -o $n.j.cwasm 2>>$n.j.log && r+=" js2wasi3:ok" || r+=" js2wasi3:FAIL($(grep -m1 -i error $n.j.log | cut -c1-80))"
  $S build $f -o $n.sc >$n.sc.log 2>&1 && r+=" scriptc:ok" || r+=" scriptc:FAIL($(grep -m1 error $n.sc.log | sed 's#.*/##' | cut -c1-80))"
  SCRIPTC_TARGET=wasm32-wasi SCRIPTC_RUNTIME_PACK=~/scriptc-0.2.7/wasi/package $S build $f -o $n.scw.wasm >$n.scw.log 2>&1 \
    && wasmtime compile $n.scw.wasm -o $n.scw.cwasm 2>>$n.scw.log && r+=" scriptc-wasm:ok" || r+=" scriptc-wasm:FAIL"
  echo "$r"
}
cd $D/a; for f in *.js *.mjs; do build $D/a $f ${f%.*}; done
cd $D/b; for f in *.ts; do n=${f%.ts}; r="$n(ts)"
  $S build $f -o $n.sc >$n.sc.log 2>&1 && r+=" scriptc:ok" || r+=" scriptc:FAIL"
  SCRIPTC_TARGET=wasm32-wasi SCRIPTC_RUNTIME_PACK=~/scriptc-0.2.7/wasi/package $S build $f -o $n.scw.wasm >$n.scw.log 2>&1 && wasmtime compile $n.scw.wasm -o $n.scw.cwasm 2>>$n.scw.log && r+=" scriptc-wasm:ok" || r+=" scriptc-wasm:FAIL"
  echo "$r"; done
