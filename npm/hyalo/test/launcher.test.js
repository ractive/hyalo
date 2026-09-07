'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const {
  PLATFORMS,
  packageFor,
  resolveBinary
} = require('../lib/resolve-platform');

const EXPECTED_PLATFORMS = [
  { target: 'aarch64-apple-darwin', package: '@ractive-ch/hyalo-darwin-arm64', os: 'darwin', cpu: 'arm64', libc: null },
  { target: 'x86_64-unknown-linux-gnu', package: '@ractive-ch/hyalo-linux-x64', os: 'linux', cpu: 'x64', libc: 'glibc' },
  { target: 'aarch64-unknown-linux-gnu', package: '@ractive-ch/hyalo-linux-arm64', os: 'linux', cpu: 'arm64', libc: 'glibc' },
  { target: 'x86_64-unknown-linux-musl', package: '@ractive-ch/hyalo-linux-x64-musl', os: 'linux', cpu: 'x64', libc: 'musl' },
  { target: 'aarch64-unknown-linux-musl', package: '@ractive-ch/hyalo-linux-arm64-musl', os: 'linux', cpu: 'arm64', libc: 'musl' },
  { target: 'x86_64-pc-windows-msvc', package: '@ractive-ch/hyalo-win32-x64', os: 'win32', cpu: 'x64', libc: null },
  { target: 'aarch64-pc-windows-msvc', package: '@ractive-ch/hyalo-win32-arm64', os: 'win32', cpu: 'arm64', libc: null }
];

test('generated table and selection match all seven release targets', () => {
  assert.deepEqual(PLATFORMS, EXPECTED_PLATFORMS);
  for (const expected of EXPECTED_PLATFORMS) {
    assert.deepEqual(packageFor({
      platform: expected.os,
      arch: expected.cpu,
      libc: expected.libc
    }), expected);
  }
});

test('resolves the exact binary path in the selected optional package', () => {
  let requested;
  const resolved = resolveBinary((specifier) => {
    requested = specifier;
    return '/fixture/hyalo';
  }, { platform: 'linux', arch: 'arm64', libc: 'musl' });
  assert.equal(requested, '@ractive-ch/hyalo-linux-arm64-musl/hyalo');
  assert.equal(resolved.packageName, '@ractive-ch/hyalo-linux-arm64-musl');
  assert.equal(resolved.binary, '/fixture/hyalo');
});

test('rejects Darwin x64, unsupported CPUs, and unknown Linux libc', () => {
  assert.throws(
    () => resolveBinary(() => '', { platform: 'darwin', arch: 'x64' }),
    /os=darwin, cpu=x64, libc=n\/a.*cargo install hyalo-cli/
  );
  assert.throws(
    () => resolveBinary(() => '', { platform: 'win32', arch: 'ia32' }),
    /os=win32, cpu=ia32, libc=n\/a.*cargo install hyalo-cli/
  );
  assert.throws(
    () => resolveBinary(() => '', { platform: 'linux', arch: 'x64', libc: null }),
    /os=linux, cpu=x64, libc=unknown.*cargo install hyalo-cli/
  );
});

test('missing optional package names the package, platform, and remedy', () => {
  assert.throws(
    () => resolveBinary(() => {
      throw Object.assign(new Error('not found'), { code: 'MODULE_NOT_FOUND' });
    }, { platform: 'linux', arch: 'x64', libc: 'glibc' }),
    /@ractive-ch\/hyalo-linux-x64.*os=linux, cpu=x64, libc=glibc.*optional dependencies.*cargo install hyalo-cli/
  );
});

const harness = path.join(__dirname, 'fixtures', 'run-child-harness.js');

test('real child inherits stdio, receives argv unchanged, and exits zero', () => {
  const args = ['exit', '0', 'plain', 'space value', '"quoted"', "single'quote"];
  const result = spawnSync(process.execPath, [harness, ...args], {
    input: 'stdin payload',
    encoding: 'utf8'
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /child stderr/);
  const output = JSON.parse(result.stdout);
  assert.deepEqual(output.argv, args);
  assert.equal(output.stdin, 'stdin payload');
});

test('real child numeric exit status is propagated', () => {
  const result = spawnSync(process.execPath, [harness, 'exit', '7'], {
    encoding: 'utf8'
  });
  assert.equal(result.status, 7);
});

test('spawn ENOENT is reported once and exits nonzero', () => {
  const result = spawnSync(process.execPath, [harness, 'missing'], {
    encoding: 'utf8'
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /hyalo: failed to start .*ENOENT/);
  assert.equal(result.stderr.match(/failed to start/g).length, 1);
});

test('real child signal termination is propagated on Unix', {
  skip: process.platform === 'win32'
}, () => {
  const result = spawnSync(process.execPath, [harness, 'signal'], {
    encoding: 'utf8'
  });
  assert.equal(result.status, null);
  assert.equal(result.signal, 'SIGTERM');
});

test('SIGTERM sent to the launcher PID terminates its real child', {
  skip: process.platform === 'win32',
  timeout: 5000
}, async () => {
  const runner = spawn(process.execPath, [harness, 'wait'], {
    stdio: ['ignore', 'pipe', 'pipe']
  });
  const childPid = await new Promise((resolve, reject) => {
    runner.once('error', reject);
    runner.stdout.once('data', (chunk) => {
      const match = /^ready:(\d+)/.exec(chunk.toString());
      if (!match) {
        reject(new Error(`unexpected child readiness output: ${chunk}`));
        return;
      }
      resolve(Number(match[1]));
    });
  });
  const closed = new Promise((resolve) => {
    runner.once('close', (code, signal) => resolve({ code, signal }));
  });
  process.kill(runner.pid, 'SIGTERM');
  assert.deepEqual(await closed, { code: null, signal: 'SIGTERM' });
  assert.throws(
    () => process.kill(childPid, 0),
    (error) => error.code === 'ESRCH'
  );
});
