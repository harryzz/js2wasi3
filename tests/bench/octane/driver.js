// T78: the driver appended to base.js + one Octane benchmark (scripts/bench.sh makes one file of the three; Octane's
// shell harness, run.js, uses load() and print, which neither node nor js2wasi3 has).
// Arguments: `score` (Octane's own measurement: each benchmark runs for at least a second, after a warm-up, and the
// output is its score) or `fixed [scale]` (no warm-up, each benchmark runs its fixed `deterministicIterations`
// times `scale` (default 1, at least one iteration): the same work for node and js2wasi3, timed from outside; the
// output is the same for both, so it is compared).
// A benchmark that computes a wrong result throws; base.js reports it through NotifyError.
var mode = process.argv[2] || 'score';
var success = true;

if (mode === 'fixed') {
  BenchmarkSuite.config.doWarmup = false;
  BenchmarkSuite.config.doDeterministic = true;
  var scale = +(process.argv[3] || 1);
  BenchmarkSuite.suites.forEach(function (suite) {
    suite.benchmarks.forEach(function (b) {
      b.deterministicIterations = Math.max(1, Math.round(b.deterministicIterations * scale));
    });
  });
}

BenchmarkSuite.RunSuites({
  NotifyResult: function (name, result) {
    console.log(name + ': ' + (mode === 'fixed' ? 'done' : result));
  },
  NotifyError: function (name, error) {
    console.log(name + ': ' + error);
    success = false;
  },
  NotifyScore: function (score) {
    if (success && mode !== 'fixed') {
      console.log('----');
      console.log('Score (version ' + BenchmarkSuite.version + '): ' + score);
    }
  }
});
if (!success) process.exitCode = 1;
