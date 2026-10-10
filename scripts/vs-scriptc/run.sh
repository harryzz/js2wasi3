#!/bin/bash
# docs/performance.md: after prep.sh; identical sources, 3 interleaved rounds (node, scriptc native, scriptc wasm, js2wasi3), output vs node's
D=~/t104; R=$D/results.tsv; TO=600
ms() { echo $(( ($2-$1)/1000000 )); }
med() { printf '%s\n' "$@" | sort -n | sed -n 2p; }
printf 'set\tprogram\targs\tnode_ms\tscriptc_ms\tscriptc\tscriptc_wasm_ms\tscriptc_wasm\tjs2wasi3_ms\tjs2wasi3\n' > $R
run() { # set dir src name args input
  local set=$1 d=$2 src=$3 n=$4 a=$5 in=$6 st=/dev/null
  cd $d; if [ -n "$in" ]; then cp $D/a/fasta-$in.fa input.fa; st=$d/input.fa; fi
  local N=() SC=() SW=() JW=() s1=- s2=- s3=-
  for i in 1 2 3; do
    t0=$(date +%s%N); timeout $TO node $src $a <$st >$n.$set.node.o 2>&1; echo "exit $?" >>$n.$set.node.o; t1=$(date +%s%N); N+=($(ms $t0 $t1))
    if [ -x $n.sc ] && [ "$s1" != DIFF ]; then t0=$(date +%s%N); timeout $TO ./$n.sc $a <$st >$n.$set.sc.o 2>&1; echo "exit $?" >>$n.$set.sc.o; t1=$(date +%s%N); SC+=($(ms $t0 $t1))
      cmp -s $n.$set.node.o $n.$set.sc.o && s1=SAME || s1=DIFF; fi
    if [ -f $n.scw.cwasm ] && [ "$s2" != DIFF ]; then t0=$(date +%s%N); timeout $TO wasmtime run --allow-precompiled --dir $d::/ $n.scw.cwasm $a <$st >$n.$set.scw.o 2>&1; echo "exit $?" >>$n.$set.scw.o; t1=$(date +%s%N); SW+=($(ms $t0 $t1))
      cmp -s $n.$set.node.o $n.$set.scw.o && s2=SAME || s2=DIFF; fi
    if [ -f $n.j.cwasm ] && [ "$s3" != DIFF ]; then t0=$(date +%s%N); timeout $TO wasmtime run -Sp3 --allow-precompiled --dir $d::/ $n.j.cwasm $a <$st >$n.$set.j.o 2>&1; echo "exit $?" >>$n.$set.j.o; t1=$(date +%s%N); JW+=($(ms $t0 $t1))
      cmp -s $n.$set.node.o $n.$set.j.o && s3=SAME || s3=DIFF; fi
  done
  [ -x $n.sc ] || s1=NOBUILD; [ -f $n.scw.cwasm ] || s2=NOBUILD; [ -f $n.j.cwasm ] || s3=NOBUILD
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' $set $n "$a${in:+ fasta $in}" $(med ${N[@]}) "$( [ $s1 = SAME ] && med ${SC[@]})" $s1 "$( [ $s2 = SAME ] && med ${SW[@]})" $s2 "$( [ $s3 = SAME ] && med ${JW[@]})" $s3 >> $R
  tail -1 $R
}
while IFS='|' read -r n a in; do run A $D/a $n.js $n "$a" "$in"; done <<'L'
binarytrees|16|
fannkuchredux|10|
fasta|250000|
knucleotide||250000
mandelbrot|1000|
nbody|500000|
pidigits|2000|
regexredux||500000
revcomp||250000
spectralnorm|500|
octane-richards|fixed 0.05|
octane-deltablue|fixed 0.05|
octane-raytrace|fixed 0.05|
octane-navier-stokes|fixed 0.05|
octane-crypto|fixed 0.05|
octane-splay|fixed 0.05|
octane-earley-boyer|fixed 0.05|
octane-regexp|fixed 0.05|
L
W=$(node -e 'for(const w of require(process.env.HOME+"/t103-sw/src/workloads.json").workloads) if(require("fs").existsSync(process.env.HOME+"/t104/b/"+w.entry.replace("workloads/",""))) console.log(w.name+"|"+(w.args||[]).join(" "))')
for x in $W; do IFS='|' read -r n a <<<"$x"; run A $D/a $n.mjs $n "$a" ""; done
# set B: the .ts files; js2wasi3 runs its build of the same file with the types stripped (set A's module)
for x in $W; do IFS='|' read -r n a <<<"$x"; ln -sf $D/a/$n.j.cwasm $D/b/$n.j.cwasm; run B $D/b $n.ts $n "$a" ""; done
echo ALL DONE
# Afterwards: node on each shared file vs node on the original (~/t101-bench/<n>.node.out, <n>.B.node.o for the workloads).
