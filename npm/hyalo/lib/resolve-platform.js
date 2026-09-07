'use strict';

const PLATFORMS = Object.freeze(require('../platforms.json'));

function currentPlatform(probe = {}) {
  const platform = probe.platform || process.platform;
  const arch = probe.arch || process.arch;
  const libc = platform === 'linux' ? (probe.libc === undefined ? require('detect-libc').familySync() : probe.libc) : null;
  return { platform, arch, libc };
}

function packageFor(p) {
  return PLATFORMS.find(({ os, cpu, libc }) =>
    os === p.platform && cpu === p.arch && libc === p.libc);
}

function resolveBinary(requireResolve = require.resolve, probe) {
  const current = currentPlatform(probe);
  const match = packageFor(current);
  if (!match) {
    const libc = current.libc || (current.platform === 'linux' ? 'unknown' : 'n/a');
    throw new Error(`No Hyalo binary package supports os=${current.platform}, cpu=${current.arch}, libc=${libc}. Intel macOS and unknown Linux libc are unsupported; install with cargo install hyalo-cli instead.`);
  }
  try {
    const binaryName = current.platform === 'win32' ? 'hyalo.exe' : 'hyalo';
    return {
      packageName: match.package,
      binary: requireResolve(`${match.package}/${binaryName}`),
      ...current
    };
  } catch (error) {
    throw new Error(`The optional package ${match.package} is missing for os=${current.platform}, cpu=${current.arch}, libc=${current.libc || 'n/a'}. Ensure npm optional dependencies are enabled, or install with cargo install hyalo-cli.`, { cause: error });
  }
}

module.exports = { PLATFORMS, currentPlatform, packageFor, resolveBinary };
