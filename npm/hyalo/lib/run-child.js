'use strict';

const { spawn } = require('node:child_process');

function runBinary(binary, argv) {
  const child = spawn(binary, argv, {
    stdio: 'inherit',
    shell: false
  });
  const signalHandlers = new Map();
  let finished = false;

  function removeSignalHandlers() {
    for (const [signal, handler] of signalHandlers) {
      process.removeListener(signal, handler);
    }
    signalHandlers.clear();
  }

  for (const signal of ['SIGINT', 'SIGTERM']) {
    const handler = () => {
      if (!finished) child.kill(signal);
    };
    signalHandlers.set(signal, handler);
    process.on(signal, handler);
  }

  child.once('error', (error) => {
    if (finished) return;
    finished = true;
    removeSignalHandlers();
    console.error(`hyalo: failed to start ${binary}: ${error.message}`);
    process.exitCode = 1;
  });
  child.once('exit', (code, signal) => {
    if (finished) return;
    finished = true;
    removeSignalHandlers();
    if (signal) {
      try {
        process.kill(process.pid, signal);
      } catch (error) {
        console.error(`hyalo: child exited with signal ${signal}: ${error.message}`);
        process.exitCode = 1;
      }
      return;
    }
    process.exitCode = code ?? 1;
  });
}

module.exports = { runBinary };
