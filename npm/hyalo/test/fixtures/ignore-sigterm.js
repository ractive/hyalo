'use strict';

// A child that survives SIGTERM, so cancellation must escalate to SIGKILL.
const fs = require('node:fs');
process.on('SIGTERM', () => {});
fs.writeFileSync(process.argv[2], String(process.pid));
setInterval(() => {}, 1000);
