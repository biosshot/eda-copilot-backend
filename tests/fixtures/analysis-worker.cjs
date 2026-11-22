// Test-only synchronous workload, controlled by explicit ready/release files.
// No sleep/crash controls are compiled into the production worker.
const { existsSync, writeFileSync } = require('node:fs');
const workerpool = require('workerpool');
const wait = new Int32Array(new SharedArrayBuffer(4));
workerpool.worker({
  importAnalysisInWorker(problem, jobId) {
    const command = JSON.parse(problem.boardJson);
    workerpool.workerEmit({ jobId, phase: 'started' });
    if (command.ready) writeFileSync(command.ready, String(process.pid));
    if (command.mode === 'exit') process.exit(23);
    if (command.mode === 'block') {
      const deadline = performance.now() + 20_000;
      while (!existsSync(command.release)) {
        if (performance.now() >= deadline) throw new Error('Test worker watchdog expired');
        Atomics.wait(wait, 0, 0, 10);
      }
    }
    return { imported: true, accepted: {}, diagnostics: [], pid: process.pid };
  },
});
