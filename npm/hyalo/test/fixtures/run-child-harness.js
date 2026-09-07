'use strict';

const path = require('node:path');
const { runBinary } = require('../../lib/run-child');

if (process.argv[2] === 'missing') {
  runBinary(path.join(__dirname, 'does-not-exist'), []);
} else {
  runBinary(
    process.execPath,
    [path.join(__dirname, 'child.js'), ...process.argv.slice(2)]
  );
}
