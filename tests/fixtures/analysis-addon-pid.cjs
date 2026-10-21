// Loaded only by the integration test subprocess via --require. Observe dlopen
// without changing the production task/report or adding a runtime debug flag.
const { appendFileSync } = require('node:fs');
const original = process.dlopen;
process.dlopen = function (...args) {
  appendFileSync(process.env.ANALYSIS_TEST_DLOPEN_LOG, JSON.stringify({ pid: process.pid, file: args[1] }) + '\n');
  return Reflect.apply(original, this, args);
};
