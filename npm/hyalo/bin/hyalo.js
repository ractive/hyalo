#!/usr/bin/env node
'use strict';
const { resolveBinary } = require('../lib/resolve-platform');
const { runBinary } = require('../lib/run-child');

function main() {
  let resolved;
  try {
    resolved = resolveBinary();
  } catch (error) {
    console.error(`hyalo: ${error.message}`);
    process.exitCode = 1;
    return;
  }
  runBinary(resolved.binary, process.argv.slice(2));
}

main();
