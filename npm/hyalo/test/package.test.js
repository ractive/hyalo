'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { pathToFileURL } = require('node:url');
const { currentPlatform, packageFor } = require('../lib/resolve-platform');

const execFileAsync = promisify(execFile);
const packageDir = path.resolve(__dirname, '..');
const npmCli = process.env.npm_execpath;
const binary = process.env.HYALO_BIN || path.resolve(
  packageDir,
  `../../target/release/hyalo${process.platform === 'win32' ? '.exe' : ''}`
);

test('packed ESM/CJS API includes declarations and the default native resolver works', { timeout: 60_000 }, async () => {
  assert.ok(npmCli, 'npm_execpath is required; run this package test through npm test');
  const scratch = await fsp.mkdtemp(path.join(os.tmpdir(), 'hyalo-packed-api-'));
  try {
    const packed = await execFileAsync(process.execPath, [npmCli, 'pack', '--json', '--pack-destination', scratch], {
      cwd: packageDir,
      encoding: 'utf8',
    });
    const [{ filename, files }] = JSON.parse(packed.stdout);
    assert.ok(files.some((entry) => entry.path === 'dist/index.mjs'));
    assert.ok(files.some((entry) => entry.path === 'dist/index.cjs'));
    assert.ok(files.some((entry) => entry.path === 'dist/index.d.ts'));

    const consumer = path.join(scratch, 'fresh consumer');
    await fsp.mkdir(consumer, { recursive: true });
    await fsp.writeFile(path.join(consumer, 'package.json'), '{"type":"module"}\n');
    await execFileAsync(
      process.execPath,
      // The Hyalo tarball is local; npm may fetch normal dependency metadata
      // because a pristine CI cache does not contain detect-libc yet.
      [npmCli, 'install', '--ignore-scripts', '--omit=optional', path.join(scratch, filename)],
      { cwd: consumer, encoding: 'utf8' },
    );

    const platform = packageFor(currentPlatform());
    assert.ok(platform, `test runner platform ${process.platform}/${process.arch} must be supported`);
    const nativePackage = path.join(consumer, 'node_modules', ...platform.package.split('/'));
    await fsp.mkdir(nativePackage, { recursive: true });
    const binaryName = process.platform === 'win32' ? 'hyalo.exe' : 'hyalo';
    await fsp.copyFile(path.resolve(binary), path.join(nativePackage, binaryName));

    for (const bundled of [
      path.join(consumer, 'node_modules/@ractive-ch/hyalo/dist/index.mjs'),
      path.join(consumer, 'node_modules/@ractive-ch/hyalo/dist/index.cjs'),
      path.resolve(packageDir, '../../pi-package/lib/hyalo-api.js'),
    ]) {
      const source = await fsp.readFile(bundled, 'utf8');
      assert.match(source, /Copyright 2017 Lovell Fuller and others\./);
      assert.match(source, /SPDX-License-Identifier: Apache-2\.0/);
      assert.match(source, /END OF TERMS AND CONDITIONS/);
    }

    const esm = await import(pathToFileURL(path.join(consumer, 'node_modules/@ractive-ch/hyalo/dist/index.mjs')).href);
    const config = await esm.config({ cwd: packageDir });
    assert.equal(config.results.snapshot_format_version, 2);
    assert.deepEqual(config.hints, []);

    const cjs = require(path.join(consumer, 'node_modules/@ractive-ch/hyalo/dist/index.cjs'));
    const explicit = await cjs.config({ binaryPath: binary, cwd: packageDir });
    assert.equal(explicit.results.snapshot_format_version, 2);
  } finally {
    await fsp.rm(scratch, { recursive: true, force: true });
  }
});
