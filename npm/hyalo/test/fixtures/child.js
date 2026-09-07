'use strict';

const mode = process.argv[2];
if (mode === 'wait') {
  process.stdout.write(`ready:${process.pid}\n`);
  setInterval(() => {}, 1000);
  return;
}
const stdin = [];
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => stdin.push(chunk));
process.stdin.on('end', () => {
  if (mode === 'signal') {
    process.kill(process.pid, 'SIGTERM');
    return;
  }
  process.stdout.write(JSON.stringify({
    argv: process.argv.slice(2),
    stdin: stdin.join('')
  }));
  process.stderr.write('child stderr\n');
  process.exitCode = Number(process.argv[3]);
});
