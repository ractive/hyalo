'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { readdirSync, readFileSync, statSync } = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');

const {
  CONSUMER_DIR: consumerDir,
  EXPECTED_ARCH: expectedArch,
  EXPECTED_LIBC: expectedLibc,
  EXPECTED_PLATFORM: expectedPlatform,
  EXPECTED_PLATFORM_PACKAGE: expectedPlatformPackage,
  VERSION: version
} = process.env;

for (const [name, value] of Object.entries({
  CONSUMER_DIR: consumerDir,
  EXPECTED_ARCH: expectedArch,
  EXPECTED_LIBC: expectedLibc,
  EXPECTED_PLATFORM: expectedPlatform,
  EXPECTED_PLATFORM_PACKAGE: expectedPlatformPackage,
  VERSION: version
})) {
  assert(value, `${name} must be set`);
}

assert.equal(process.platform, expectedPlatform);
assert.equal(process.arch, expectedArch);

const requireFromConsumer = createRequire(path.join(consumerDir, 'package.json'));
const mainManifest = JSON.parse(readFileSync(
  requireFromConsumer.resolve('@ractive-ch/hyalo/package.json'),
  'utf8'
));
assert.equal(mainManifest.name, '@ractive-ch/hyalo');
assert.equal(mainManifest.version, version);

const scopeDir = path.join(consumerDir, 'node_modules', '@ractive-ch');
const installedPlatformPackages = readdirSync(scopeDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.startsWith('hyalo-'))
  .map((entry) => `@ractive-ch/${entry.name}`)
  .sort();
assert.deepEqual(installedPlatformPackages, [expectedPlatformPackage]);

const platformManifest = JSON.parse(readFileSync(
  requireFromConsumer.resolve(`${expectedPlatformPackage}/package.json`),
  'utf8'
));
assert.equal(platformManifest.name, expectedPlatformPackage);
assert.equal(platformManifest.version, version);
assert.deepEqual(platformManifest.os, [expectedPlatform]);
assert.deepEqual(platformManifest.cpu, [expectedArch]);
if (expectedLibc === 'none') {
  assert.equal(platformManifest.libc, undefined);
} else {
  assert.deepEqual(platformManifest.libc, [expectedLibc]);
  const detectLibc = requireFromConsumer('detect-libc');
  assert.equal(detectLibc.familySync(), expectedLibc);
}

const binaryName = process.platform === 'win32' ? 'hyalo.exe' : 'hyalo';
const binary = requireFromConsumer.resolve(`${expectedPlatformPackage}/${binaryName}`);
assert(statSync(binary).isFile(), `resolved binary is not a file: ${binary}`);

const output = process.platform === 'win32'
  ? execFileSync(process.env.ComSpec || 'cmd.exe', [
    '/d', '/s', '/c', 'npx.cmd --no-install hyalo --version'
  ], { cwd: consumerDir, encoding: 'utf8' })
  : execFileSync('npx', ['--no-install', 'hyalo', '--version'], {
    cwd: consumerDir,
    encoding: 'utf8'
  });
const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const versionPattern = new RegExp(
  `^hyalo ${escapedVersion}(?: \\([0-9a-f]{7,12}(?:\\+dirty)? ` +
  `\\d{4}-\\d{2}-\\d{2}\\))?\\r?\\n?$`
);
assert.match(output, versionPattern);

console.log(
  `Verified hyalo ${version} with ${expectedPlatformPackage} on ` +
  `${process.platform}/${process.arch}`
);
