export interface PlatformProbe {
  platform?: NodeJS.Platform;
  arch?: string;
  libc?: string | null;
}

export interface ResolvedBinary {
  packageName: string;
  binary: string;
  platform: NodeJS.Platform;
  arch: string;
  libc: string | null;
}

export function resolveBinary(
  requireResolve?: (id: string) => string,
  probe?: PlatformProbe,
): ResolvedBinary;
