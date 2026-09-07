/*!
 * detect-libc 2.1.2
 * Copyright 2017 Lovell Fuller and others.
 * SPDX-License-Identifier: Apache-2.0
 *
 *                                  Apache License
 *                            Version 2.0, January 2004
 *                         http://www.apache.org/licenses/
 *
 *    TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION
 *
 *    1. Definitions.
 *
 *       "License" shall mean the terms and conditions for use, reproduction,
 *       and distribution as defined by Sections 1 through 9 of this document.
 *
 *       "Licensor" shall mean the copyright owner or entity authorized by
 *       the copyright owner that is granting the License.
 *
 *       "Legal Entity" shall mean the union of the acting entity and all
 *       other entities that control, are controlled by, or are under common
 *       control with that entity. For the purposes of this definition,
 *       "control" means (i) the power, direct or indirect, to cause the
 *       direction or management of such entity, whether by contract or
 *       otherwise, or (ii) ownership of fifty percent (50%) or more of the
 *       outstanding shares, or (iii) beneficial ownership of such entity.
 *
 *       "You" (or "Your") shall mean an individual or Legal Entity
 *       exercising permissions granted by this License.
 *
 *       "Source" form shall mean the preferred form for making modifications,
 *       including but not limited to software source code, documentation
 *       source, and configuration files.
 *
 *       "Object" form shall mean any form resulting from mechanical
 *       transformation or translation of a Source form, including but
 *       not limited to compiled object code, generated documentation,
 *       and conversions to other media types.
 *
 *       "Work" shall mean the work of authorship, whether in Source or
 *       Object form, made available under the License, as indicated by a
 *       copyright notice that is included in or attached to the work
 *       (an example is provided in the Appendix below).
 *
 *       "Derivative Works" shall mean any work, whether in Source or Object
 *       form, that is based on (or derived from) the Work and for which the
 *       editorial revisions, annotations, elaborations, or other modifications
 *       represent, as a whole, an original work of authorship. For the purposes
 *       of this License, Derivative Works shall not include works that remain
 *       separable from, or merely link (or bind by name) to the interfaces of,
 *       the Work and Derivative Works thereof.
 *
 *       "Contribution" shall mean any work of authorship, including
 *       the original version of the Work and any modifications or additions
 *       to that Work or Derivative Works thereof, that is intentionally
 *       submitted to Licensor for inclusion in the Work by the copyright owner
 *       or by an individual or Legal Entity authorized to submit on behalf of
 *       the copyright owner. For the purposes of this definition, "submitted"
 *       means any form of electronic, verbal, or written communication sent
 *       to the Licensor or its representatives, including but not limited to
 *       communication on electronic mailing lists, source code control systems,
 *       and issue tracking systems that are managed by, or on behalf of, the
 *       Licensor for the purpose of discussing and improving the Work, but
 *       excluding communication that is conspicuously marked or otherwise
 *       designated in writing by the copyright owner as "Not a Contribution."
 *
 *       "Contributor" shall mean Licensor and any individual or Legal Entity
 *       on behalf of whom a Contribution has been received by Licensor and
 *       subsequently incorporated within the Work.
 *
 *    2. Grant of Copyright License. Subject to the terms and conditions of
 *       this License, each Contributor hereby grants to You a perpetual,
 *       worldwide, non-exclusive, no-charge, royalty-free, irrevocable
 *       copyright license to reproduce, prepare Derivative Works of,
 *       publicly display, publicly perform, sublicense, and distribute the
 *       Work and such Derivative Works in Source or Object form.
 *
 *    3. Grant of Patent License. Subject to the terms and conditions of
 *       this License, each Contributor hereby grants to You a perpetual,
 *       worldwide, non-exclusive, no-charge, royalty-free, irrevocable
 *       (except as stated in this section) patent license to make, have made,
 *       use, offer to sell, sell, import, and otherwise transfer the Work,
 *       where such license applies only to those patent claims licensable
 *       by such Contributor that are necessarily infringed by their
 *       Contribution(s) alone or by combination of their Contribution(s)
 *       with the Work to which such Contribution(s) was submitted. If You
 *       institute patent litigation against any entity (including a
 *       cross-claim or counterclaim in a lawsuit) alleging that the Work
 *       or a Contribution incorporated within the Work constitutes direct
 *       or contributory patent infringement, then any patent licenses
 *       granted to You under this License for that Work shall terminate
 *       as of the date such litigation is filed.
 *
 *    4. Redistribution. You may reproduce and distribute copies of the
 *       Work or Derivative Works thereof in any medium, with or without
 *       modifications, and in Source or Object form, provided that You
 *       meet the following conditions:
 *
 *       (a) You must give any other recipients of the Work or
 *           Derivative Works a copy of this License; and
 *
 *       (b) You must cause any modified files to carry prominent notices
 *           stating that You changed the files; and
 *
 *       (c) You must retain, in the Source form of any Derivative Works
 *           that You distribute, all copyright, patent, trademark, and
 *           attribution notices from the Source form of the Work,
 *           excluding those notices that do not pertain to any part of
 *           the Derivative Works; and
 *
 *       (d) If the Work includes a "NOTICE" text file as part of its
 *           distribution, then any Derivative Works that You distribute must
 *           include a readable copy of the attribution notices contained
 *           within such NOTICE file, excluding those notices that do not
 *           pertain to any part of the Derivative Works, in at least one
 *           of the following places: within a NOTICE text file distributed
 *           as part of the Derivative Works; within the Source form or
 *           documentation, if provided along with the Derivative Works; or,
 *           within a display generated by the Derivative Works, if and
 *           wherever such third-party notices normally appear. The contents
 *           of the NOTICE file are for informational purposes only and
 *           do not modify the License. You may add Your own attribution
 *           notices within Derivative Works that You distribute, alongside
 *           or as an addendum to the NOTICE text from the Work, provided
 *           that such additional attribution notices cannot be construed
 *           as modifying the License.
 *
 *       You may add Your own copyright statement to Your modifications and
 *       may provide additional or different license terms and conditions
 *       for use, reproduction, or distribution of Your modifications, or
 *       for any such Derivative Works as a whole, provided Your use,
 *       reproduction, and distribution of the Work otherwise complies with
 *       the conditions stated in this License.
 *
 *    5. Submission of Contributions. Unless You explicitly state otherwise,
 *       any Contribution intentionally submitted for inclusion in the Work
 *       by You to the Licensor shall be under the terms and conditions of
 *       this License, without any additional terms or conditions.
 *       Notwithstanding the above, nothing herein shall supersede or modify
 *       the terms of any separate license agreement you may have executed
 *       with Licensor regarding such Contributions.
 *
 *    6. Trademarks. This License does not grant permission to use the trade
 *       names, trademarks, service marks, or product names of the Licensor,
 *       except as required for reasonable and customary use in describing the
 *       origin of the Work and reproducing the content of the NOTICE file.
 *
 *    7. Disclaimer of Warranty. Unless required by applicable law or
 *       agreed to in writing, Licensor provides the Work (and each
 *       Contributor provides its Contributions) on an "AS IS" BASIS,
 *       WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
 *       implied, including, without limitation, any warranties or conditions
 *       of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
 *       PARTICULAR PURPOSE. You are solely responsible for determining the
 *       appropriateness of using or redistributing the Work and assume any
 *       risks associated with Your exercise of permissions under this License.
 *
 *    8. Limitation of Liability. In no event and under no legal theory,
 *       whether in tort (including negligence), contract, or otherwise,
 *       unless required by applicable law (such as deliberate and grossly
 *       negligent acts) or agreed to in writing, shall any Contributor be
 *       liable to You for damages, including any direct, indirect, special,
 *       incidental, or consequential damages of any character arising as a
 *       result of this License or out of the use or inability to use the
 *       Work (including but not limited to damages for loss of goodwill,
 *       work stoppage, computer failure or malfunction, or any and all
 *       other commercial damages or losses), even if such Contributor
 *       has been advised of the possibility of such damages.
 *
 *    9. Accepting Warranty or Additional Liability. While redistributing
 *       the Work or Derivative Works thereof, You may choose to offer,
 *       and charge a fee for, acceptance of support, warranty, indemnity,
 *       or other liability obligations and/or rights consistent with this
 *       License. However, in accepting such obligations, You may act only
 *       on Your own behalf and on Your sole responsibility, not on behalf
 *       of any other Contributor, and only if You agree to indemnify,
 *       defend, and hold each Contributor harmless for any liability
 *       incurred by, or claims asserted against, such Contributor by reason
 *       of your accepting any such warranty or additional liability.
 *
 *    END OF TERMS AND CONDITIONS
 *
 *    APPENDIX: How to apply the Apache License to your work.
 *
 *       To apply the Apache License to your work, attach the following
 *       boilerplate notice, with the fields enclosed by brackets "{}"
 *       replaced with your own identifying information. (Don't include
 *       the brackets!)  The text should be enclosed in the appropriate
 *       comment syntax for the file format. We also recommend that a
 *       file or class name and description of purpose be included on the
 *       same "printed page" as the copyright notice for easier
 *       identification within third-party archives.
 *
 *    Copyright {yyyy} {name of copyright owner}
 *
 *    Licensed under the Apache License, Version 2.0 (the "License");
 *    you may not use this file except in compliance with the License.
 *    You may obtain a copy of the License at
 *
 *        http://www.apache.org/licenses/LICENSE-2.0
 *
 *    Unless required by applicable law or agreed to in writing, software
 *    distributed under the License is distributed on an "AS IS" BASIS,
 *    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *    See the License for the specific language governing permissions and
 *    limitations under the License.
 */
import { createRequire as __hyaloCreateRequire } from "node:module"; const require = __hyaloCreateRequire(import.meta.url);
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __require = /* @__PURE__ */ ((x) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(x, {
  get: (a, b) => (typeof require !== "undefined" ? require : a)[b]
}) : x)(function(x) {
  if (typeof require !== "undefined") return require.apply(this, arguments);
  throw Error('Dynamic require of "' + x + '" is not supported');
});
var __commonJS = (cb, mod) => function __require2() {
  try {
    return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
  } catch (e) {
    throw mod = 0, e;
  }
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));

// platforms.json
var require_platforms = __commonJS({
  "platforms.json"(exports, module) {
    module.exports = [
      {
        cpu: "arm64",
        libc: null,
        os: "darwin",
        package: "@ractive-ch/hyalo-darwin-arm64",
        target: "aarch64-apple-darwin"
      },
      {
        cpu: "x64",
        libc: "glibc",
        os: "linux",
        package: "@ractive-ch/hyalo-linux-x64",
        target: "x86_64-unknown-linux-gnu"
      },
      {
        cpu: "arm64",
        libc: "glibc",
        os: "linux",
        package: "@ractive-ch/hyalo-linux-arm64",
        target: "aarch64-unknown-linux-gnu"
      },
      {
        cpu: "x64",
        libc: "musl",
        os: "linux",
        package: "@ractive-ch/hyalo-linux-x64-musl",
        target: "x86_64-unknown-linux-musl"
      },
      {
        cpu: "arm64",
        libc: "musl",
        os: "linux",
        package: "@ractive-ch/hyalo-linux-arm64-musl",
        target: "aarch64-unknown-linux-musl"
      },
      {
        cpu: "x64",
        libc: null,
        os: "win32",
        package: "@ractive-ch/hyalo-win32-x64",
        target: "x86_64-pc-windows-msvc"
      },
      {
        cpu: "arm64",
        libc: null,
        os: "win32",
        package: "@ractive-ch/hyalo-win32-arm64",
        target: "aarch64-pc-windows-msvc"
      }
    ];
  }
});

// node_modules/detect-libc/lib/process.js
var require_process = __commonJS({
  "node_modules/detect-libc/lib/process.js"(exports, module) {
    "use strict";
    var isLinux = () => process.platform === "linux";
    var report = null;
    var getReport = () => {
      if (!report) {
        if (isLinux() && process.report) {
          const orig = process.report.excludeNetwork;
          process.report.excludeNetwork = true;
          report = process.report.getReport();
          process.report.excludeNetwork = orig;
        } else {
          report = {};
        }
      }
      return report;
    };
    module.exports = { isLinux, getReport };
  }
});

// node_modules/detect-libc/lib/filesystem.js
var require_filesystem = __commonJS({
  "node_modules/detect-libc/lib/filesystem.js"(exports, module) {
    "use strict";
    var fs = __require("fs");
    var LDD_PATH = "/usr/bin/ldd";
    var SELF_PATH = "/proc/self/exe";
    var MAX_LENGTH = 2048;
    var readFileSync = (path) => {
      const fd = fs.openSync(path, "r");
      const buffer = Buffer.alloc(MAX_LENGTH);
      const bytesRead = fs.readSync(fd, buffer, 0, MAX_LENGTH, 0);
      fs.close(fd, () => {
      });
      return buffer.subarray(0, bytesRead);
    };
    var readFile = (path) => new Promise((resolve, reject) => {
      fs.open(path, "r", (err, fd) => {
        if (err) {
          reject(err);
        } else {
          const buffer = Buffer.alloc(MAX_LENGTH);
          fs.read(fd, buffer, 0, MAX_LENGTH, 0, (_, bytesRead) => {
            resolve(buffer.subarray(0, bytesRead));
            fs.close(fd, () => {
            });
          });
        }
      });
    });
    module.exports = {
      LDD_PATH,
      SELF_PATH,
      readFileSync,
      readFile
    };
  }
});

// node_modules/detect-libc/lib/elf.js
var require_elf = __commonJS({
  "node_modules/detect-libc/lib/elf.js"(exports, module) {
    "use strict";
    var interpreterPath = (elf) => {
      if (elf.length < 64) {
        return null;
      }
      if (elf.readUInt32BE(0) !== 2135247942) {
        return null;
      }
      if (elf.readUInt8(4) !== 2) {
        return null;
      }
      if (elf.readUInt8(5) !== 1) {
        return null;
      }
      const offset = elf.readUInt32LE(32);
      const size = elf.readUInt16LE(54);
      const count = elf.readUInt16LE(56);
      for (let i = 0; i < count; i++) {
        const headerOffset = offset + i * size;
        const type = elf.readUInt32LE(headerOffset);
        if (type === 3) {
          const fileOffset = elf.readUInt32LE(headerOffset + 8);
          const fileSize = elf.readUInt32LE(headerOffset + 32);
          return elf.subarray(fileOffset, fileOffset + fileSize).toString().replace(/\0.*$/g, "");
        }
      }
      return null;
    };
    module.exports = {
      interpreterPath
    };
  }
});

// node_modules/detect-libc/lib/detect-libc.js
var require_detect_libc = __commonJS({
  "node_modules/detect-libc/lib/detect-libc.js"(exports, module) {
    "use strict";
    var childProcess = __require("child_process");
    var { isLinux, getReport } = require_process();
    var { LDD_PATH, SELF_PATH, readFile, readFileSync } = require_filesystem();
    var { interpreterPath } = require_elf();
    var cachedFamilyInterpreter;
    var cachedFamilyFilesystem;
    var cachedVersionFilesystem;
    var command = "getconf GNU_LIBC_VERSION 2>&1 || true; ldd --version 2>&1 || true";
    var commandOut = "";
    var safeCommand = () => {
      if (!commandOut) {
        return new Promise((resolve) => {
          childProcess.exec(command, (err, out) => {
            commandOut = err ? " " : out;
            resolve(commandOut);
          });
        });
      }
      return commandOut;
    };
    var safeCommandSync = () => {
      if (!commandOut) {
        try {
          commandOut = childProcess.execSync(command, { encoding: "utf8" });
        } catch (_err) {
          commandOut = " ";
        }
      }
      return commandOut;
    };
    var GLIBC = "glibc";
    var RE_GLIBC_VERSION = /LIBC[a-z0-9 \-).]*?(\d+\.\d+)/i;
    var MUSL = "musl";
    var isFileMusl = (f) => f.includes("libc.musl-") || f.includes("ld-musl-");
    var familyFromReport = () => {
      const report = getReport();
      if (report.header && report.header.glibcVersionRuntime) {
        return GLIBC;
      }
      if (Array.isArray(report.sharedObjects)) {
        if (report.sharedObjects.some(isFileMusl)) {
          return MUSL;
        }
      }
      return null;
    };
    var familyFromCommand = (out) => {
      const [getconf, ldd1] = out.split(/[\r\n]+/);
      if (getconf && getconf.includes(GLIBC)) {
        return GLIBC;
      }
      if (ldd1 && ldd1.includes(MUSL)) {
        return MUSL;
      }
      return null;
    };
    var familyFromInterpreterPath = (path) => {
      if (path) {
        if (path.includes("/ld-musl-")) {
          return MUSL;
        } else if (path.includes("/ld-linux-")) {
          return GLIBC;
        }
      }
      return null;
    };
    var getFamilyFromLddContent = (content) => {
      content = content.toString();
      if (content.includes("musl")) {
        return MUSL;
      }
      if (content.includes("GNU C Library")) {
        return GLIBC;
      }
      return null;
    };
    var familyFromFilesystem = async () => {
      if (cachedFamilyFilesystem !== void 0) {
        return cachedFamilyFilesystem;
      }
      cachedFamilyFilesystem = null;
      try {
        const lddContent = await readFile(LDD_PATH);
        cachedFamilyFilesystem = getFamilyFromLddContent(lddContent);
      } catch (e) {
      }
      return cachedFamilyFilesystem;
    };
    var familyFromFilesystemSync = () => {
      if (cachedFamilyFilesystem !== void 0) {
        return cachedFamilyFilesystem;
      }
      cachedFamilyFilesystem = null;
      try {
        const lddContent = readFileSync(LDD_PATH);
        cachedFamilyFilesystem = getFamilyFromLddContent(lddContent);
      } catch (e) {
      }
      return cachedFamilyFilesystem;
    };
    var familyFromInterpreter = async () => {
      if (cachedFamilyInterpreter !== void 0) {
        return cachedFamilyInterpreter;
      }
      cachedFamilyInterpreter = null;
      try {
        const selfContent = await readFile(SELF_PATH);
        const path = interpreterPath(selfContent);
        cachedFamilyInterpreter = familyFromInterpreterPath(path);
      } catch (e) {
      }
      return cachedFamilyInterpreter;
    };
    var familyFromInterpreterSync = () => {
      if (cachedFamilyInterpreter !== void 0) {
        return cachedFamilyInterpreter;
      }
      cachedFamilyInterpreter = null;
      try {
        const selfContent = readFileSync(SELF_PATH);
        const path = interpreterPath(selfContent);
        cachedFamilyInterpreter = familyFromInterpreterPath(path);
      } catch (e) {
      }
      return cachedFamilyInterpreter;
    };
    var family = async () => {
      let family2 = null;
      if (isLinux()) {
        family2 = await familyFromInterpreter();
        if (!family2) {
          family2 = await familyFromFilesystem();
          if (!family2) {
            family2 = familyFromReport();
          }
          if (!family2) {
            const out = await safeCommand();
            family2 = familyFromCommand(out);
          }
        }
      }
      return family2;
    };
    var familySync = () => {
      let family2 = null;
      if (isLinux()) {
        family2 = familyFromInterpreterSync();
        if (!family2) {
          family2 = familyFromFilesystemSync();
          if (!family2) {
            family2 = familyFromReport();
          }
          if (!family2) {
            const out = safeCommandSync();
            family2 = familyFromCommand(out);
          }
        }
      }
      return family2;
    };
    var isNonGlibcLinux = async () => isLinux() && await family() !== GLIBC;
    var isNonGlibcLinuxSync = () => isLinux() && familySync() !== GLIBC;
    var versionFromFilesystem = async () => {
      if (cachedVersionFilesystem !== void 0) {
        return cachedVersionFilesystem;
      }
      cachedVersionFilesystem = null;
      try {
        const lddContent = await readFile(LDD_PATH);
        const versionMatch = lddContent.match(RE_GLIBC_VERSION);
        if (versionMatch) {
          cachedVersionFilesystem = versionMatch[1];
        }
      } catch (e) {
      }
      return cachedVersionFilesystem;
    };
    var versionFromFilesystemSync = () => {
      if (cachedVersionFilesystem !== void 0) {
        return cachedVersionFilesystem;
      }
      cachedVersionFilesystem = null;
      try {
        const lddContent = readFileSync(LDD_PATH);
        const versionMatch = lddContent.match(RE_GLIBC_VERSION);
        if (versionMatch) {
          cachedVersionFilesystem = versionMatch[1];
        }
      } catch (e) {
      }
      return cachedVersionFilesystem;
    };
    var versionFromReport = () => {
      const report = getReport();
      if (report.header && report.header.glibcVersionRuntime) {
        return report.header.glibcVersionRuntime;
      }
      return null;
    };
    var versionSuffix = (s) => s.trim().split(/\s+/)[1];
    var versionFromCommand = (out) => {
      const [getconf, ldd1, ldd2] = out.split(/[\r\n]+/);
      if (getconf && getconf.includes(GLIBC)) {
        return versionSuffix(getconf);
      }
      if (ldd1 && ldd2 && ldd1.includes(MUSL)) {
        return versionSuffix(ldd2);
      }
      return null;
    };
    var version = async () => {
      let version2 = null;
      if (isLinux()) {
        version2 = await versionFromFilesystem();
        if (!version2) {
          version2 = versionFromReport();
        }
        if (!version2) {
          const out = await safeCommand();
          version2 = versionFromCommand(out);
        }
      }
      return version2;
    };
    var versionSync = () => {
      let version2 = null;
      if (isLinux()) {
        version2 = versionFromFilesystemSync();
        if (!version2) {
          version2 = versionFromReport();
        }
        if (!version2) {
          const out = safeCommandSync();
          version2 = versionFromCommand(out);
        }
      }
      return version2;
    };
    module.exports = {
      GLIBC,
      MUSL,
      family,
      familySync,
      isNonGlibcLinux,
      isNonGlibcLinuxSync,
      version,
      versionSync
    };
  }
});

// lib/resolve-platform.js
var require_resolve_platform = __commonJS({
  "lib/resolve-platform.js"(exports, module) {
    "use strict";
    var PLATFORMS = Object.freeze(require_platforms());
    function currentPlatform(probe = {}) {
      const platform = probe.platform || process.platform;
      const arch = probe.arch || process.arch;
      const libc = platform === "linux" ? probe.libc === void 0 ? require_detect_libc().familySync() : probe.libc : null;
      return { platform, arch, libc };
    }
    function packageFor(p) {
      return PLATFORMS.find(({ os, cpu, libc }) => os === p.platform && cpu === p.arch && libc === p.libc);
    }
    function resolveBinary2(requireResolve = __require.resolve, probe) {
      const current = currentPlatform(probe);
      const match = packageFor(current);
      if (!match) {
        const libc = current.libc || (current.platform === "linux" ? "unknown" : "n/a");
        throw new Error(`No Hyalo binary package supports os=${current.platform}, cpu=${current.arch}, libc=${libc}. Intel macOS and unknown Linux libc are unsupported; install with cargo install hyalo-cli instead.`);
      }
      try {
        const binaryName = current.platform === "win32" ? "hyalo.exe" : "hyalo";
        return {
          packageName: match.package,
          binary: requireResolve(`${match.package}/${binaryName}`),
          ...current
        };
      } catch (error) {
        throw new Error(`The optional package ${match.package} is missing for os=${current.platform}, cpu=${current.arch}, libc=${current.libc || "n/a"}. Ensure npm optional dependencies are enabled, or install with cargo install hyalo-cli.`, { cause: error });
      }
    }
    module.exports = { PLATFORMS, currentPlatform, packageFor, resolveBinary: resolveBinary2 };
  }
});

// src/api.ts
var import_resolve_platform = __toESM(require_resolve_platform());
import { spawn } from "node:child_process";
var HyaloError = class extends Error {
  exitCode;
  stdout;
  stderr;
  envelope;
  constructor(result, envelope) {
    super(envelope?.error ?? `hyalo exited with code ${result.code}`);
    this.name = "HyaloError";
    this.exitCode = result.code;
    this.stdout = result.stdout;
    this.stderr = result.stderr;
    this.envelope = envelope;
  }
};
var HyaloSpawnError = class extends Error {
  cause;
  constructor(cause) {
    super(cause instanceof Error ? cause.message : String(cause));
    this.name = "HyaloSpawnError";
    this.cause = cause;
  }
};
var HyaloParseError = class extends Error {
  stdout;
  stderr;
  cause;
  constructor(message, result, cause) {
    super(message);
    this.name = "HyaloParseError";
    this.stdout = result.stdout;
    this.stderr = result.stderr;
    this.cause = cause;
  }
};
var HyaloTimeoutError = class extends Error {
  timeoutMs;
  constructor(timeoutMs) {
    super(`hyalo timed out after ${timeoutMs}ms`);
    this.name = "HyaloTimeoutError";
    this.timeoutMs = timeoutMs;
  }
};
var HyaloAbortError = class extends Error {
  constructor() {
    super("hyalo execution was aborted");
    this.name = "HyaloAbortError";
  }
};
var HyaloTransportError = class extends Error {
  constructor(message) {
    super(message);
    this.name = "HyaloTransportError";
  }
};
var DEFAULT_TIMEOUT_MS = 6e4;
var RESERVED_OUTPUT_KEYS = /* @__PURE__ */ new Set([
  "format",
  "jq",
  "count",
  "hints",
  "no_hints",
  "filenames_only",
  "filenames0",
  "strict"
]);
function isClosedStdinWriteError(error) {
  return error.code === "EPIPE" || error.code === "EOF";
}
function executionOptions(options) {
  return {
    binaryPath: options.binaryPath,
    transport: options.transport,
    cwd: options.cwd,
    timeoutMs: options.timeoutMs,
    signal: options.signal,
    stdin: options.stdin
  };
}
function assertNoOutputTransforms(options) {
  for (const key of RESERVED_OUTPUT_KEYS) {
    if (options[key] !== void 0) {
      throw new TypeError(`typed hyalo calls own --format json --no-hints; '${key}' is reserved`);
    }
  }
}
function addFlag(argv, flag, value) {
  if (value === void 0 || value === null || value === false) return;
  if (value === true) {
    argv.push(flag);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) argv.push(`${flag}=${String(item)}`);
    return;
  }
  argv.push(`${flag}=${String(value)}`);
}
function addGlobals(argv, options) {
  addFlag(argv, "--dir", options.dir);
  addFlag(argv, "--site-prefix", options.site_prefix);
  addFlag(argv, "--quiet", options.quiet);
  addFlag(argv, "--index-file", options.index_file);
}
function findArgv(options) {
  const argv = ["find"];
  addFlag(argv, "--view", options.view);
  addFlag(argv, "--regexp", options.regexp);
  addFlag(argv, "--property", options.properties);
  addFlag(argv, "--tag", options.tag);
  addFlag(argv, "--task", options.task);
  addFlag(argv, "--section", options.sections);
  addFlag(argv, "--file", options.file);
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--files-from", options.files_from);
  addFlag(argv, "--fields", options.fields);
  addFlag(argv, "--sort", options.sort);
  addFlag(argv, "--reverse", options.reverse);
  addFlag(argv, "--limit", options.limit);
  addFlag(argv, "--broken-links", options.broken_links);
  addFlag(argv, "--orphan", options.orphan);
  addFlag(argv, "--dead-end", options.dead_end);
  addFlag(argv, "--title", options.title);
  addFlag(argv, "--language", options.language);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  const positionalFiles = Array.isArray(options.file_positional) ? options.file_positional.map(String) : [];
  const positionals = [];
  if (options.pattern !== void 0 && options.pattern !== null) {
    positionals.push(String(options.pattern));
    positionals.push(...positionalFiles);
  } else {
    addFlag(argv, "--file", positionalFiles);
  }
  if (positionals.length > 0) argv.push("--", ...positionals);
  return argv;
}
function readArgv(options) {
  const argv = ["read"];
  addFlag(argv, "--file", options.file);
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--files-from", options.files_from);
  addFlag(argv, "--section", options.section);
  addFlag(argv, "--lines", options.lines);
  addFlag(argv, "--frontmatter", options.frontmatter);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  if (options.file_positional !== void 0 && options.file_positional !== null) {
    argv.push("--", String(options.file_positional));
  }
  return argv;
}
function summaryArgv(options) {
  const argv = ["summary"];
  addFlag(argv, "--glob", options.glob);
  addFlag(argv, "--recent", options.recent);
  addFlag(argv, "--depth", options.depth);
  addFlag(argv, "--index", options.index);
  addGlobals(argv, options);
  return argv;
}
function nativeTransport(binaryPath) {
  return async (argv, options) => {
    if (options.signal?.aborted) throw new HyaloAbortError();
    const binary = binaryPath ?? (0, import_resolve_platform.resolveBinary)().binary;
    return new Promise((resolve, reject) => {
      const child = spawn(binary, [...argv], {
        cwd: options.cwd,
        shell: false,
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true
      });
      const stdout = [];
      const stderr = [];
      let settled = false;
      const cleanup = () => {
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
      };
      const fail = (error) => {
        if (settled) return;
        settled = true;
        cleanup();
        reject(error);
      };
      const abort = () => {
        child.kill();
        fail(new HyaloAbortError());
      };
      const timer = setTimeout(() => {
        child.kill();
        fail(new HyaloTimeoutError(options.timeoutMs));
      }, options.timeoutMs);
      options.signal?.addEventListener("abort", abort, { once: true });
      child.stdout.on("data", (chunk) => stdout.push(chunk));
      child.stderr.on("data", (chunk) => stderr.push(chunk));
      child.stdin.on("error", (error) => {
        if (!isClosedStdinWriteError(error)) fail(new HyaloSpawnError(error));
      });
      child.on("error", (error) => fail(new HyaloSpawnError(error)));
      child.on("close", (code) => {
        if (settled) return;
        settled = true;
        cleanup();
        resolve({
          stdout: Buffer.concat(stdout).toString("utf8"),
          stderr: Buffer.concat(stderr).toString("utf8"),
          code: code ?? 2
        });
      });
      if (options.stdin === void 0) child.stdin.end();
      else child.stdin.end(options.stdin);
    });
  };
}
function createPiTransport(pi) {
  return async (argv, options) => {
    if (options.stdin !== void 0) {
      throw new HyaloTransportError("Pi transport does not support stdin");
    }
    const result = await pi.exec("hyalo", [...argv], {
      cwd: options.cwd,
      signal: options.signal,
      timeout: options.timeoutMs
    });
    if (result.killed) {
      if (options.signal?.aborted) throw new HyaloAbortError();
      throw new HyaloTimeoutError(options.timeoutMs);
    }
    return { stdout: result.stdout, stderr: result.stderr, code: result.code };
  };
}
async function execute(argv, options = {}) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const transport = options.transport ?? nativeTransport(options.binaryPath);
  try {
    return await transport(argv, {
      cwd: options.cwd,
      timeoutMs,
      signal: options.signal,
      stdin: options.stdin
    });
  } catch (error) {
    if (error instanceof HyaloAbortError || error instanceof HyaloTimeoutError || error instanceof HyaloSpawnError || error instanceof HyaloTransportError) {
      throw error;
    }
    throw new HyaloSpawnError(error);
  }
}
function parseErrorEnvelope(result) {
  for (const raw2 of [result.stderr, result.stdout]) {
    const text = raw2.trim();
    if (!text) continue;
    const starts = [0];
    for (let index = text.indexOf("\n{"); index !== -1; index = text.indexOf("\n{", index + 2)) {
      starts.push(index + 1);
    }
    for (const start of starts.reverse()) {
      try {
        const parsed = JSON.parse(text.slice(start));
        if (typeof parsed === "object" && parsed !== null && typeof parsed.error === "string") {
          return parsed;
        }
      } catch {
      }
    }
  }
  return void 0;
}
function parseEnvelope(result) {
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  if (!result.stdout.trim()) throw new HyaloParseError("hyalo returned empty JSON", result);
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (cause) {
    throw new HyaloParseError("hyalo returned invalid JSON", result, cause);
  }
  if (typeof parsed !== "object" || parsed === null || !("results" in parsed) || !Array.isArray(parsed.hints)) {
    throw new HyaloParseError("hyalo returned an invalid envelope", result);
  }
  return parsed;
}
async function jsonCall(argv, options) {
  assertNoOutputTransforms(options);
  const terminator = argv.indexOf("--");
  argv.splice(terminator === -1 ? argv.length : terminator, 0, "--format=json", "--no-hints");
  return parseEnvelope(await execute(argv, executionOptions(options)));
}
function find(options = {}) {
  const values = options;
  return jsonCall(findArgv(values), values);
}
function read(options = {}) {
  const values = options;
  return jsonCall(readArgv(values), values);
}
function summary(options = {}) {
  const values = options;
  return jsonCall(summaryArgv(values), values);
}
function config(options = {}) {
  const values = options;
  const argv = ["config"];
  addGlobals(argv, values);
  return jsonCall(argv, values);
}
async function set(options) {
  const argv = ["set", "--format=text"];
  addFlag(argv, "--property", options.property);
  addFlag(argv, "--tag", options.tag);
  argv.push("--", options.file);
  const result = await execute(argv, options);
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  return result;
}
async function task(options) {
  const argv = ["task", "toggle"];
  if (options.mode === "section") {
    if (options.section === void 0) throw new TypeError("task mode 'section' requires section");
    addFlag(argv, "--section", options.section);
  } else if (options.mode === "line") {
    if (!options.lines?.length) throw new TypeError("task mode 'line' requires at least one line");
    addFlag(argv, "--line", options.lines.map(Math.trunc).join(","));
  } else if (options.mode === "all") {
    argv.push("--all");
  } else {
    throw new TypeError(`unknown task mode '${String(options.mode)}'`);
  }
  argv.push("--", options.file);
  const result = await execute(argv, options);
  if (result.code !== 0) throw new HyaloError(result, parseErrorEnvelope(result));
  return result;
}
async function lint(file, options = {}) {
  const argv = ["lint"];
  argv.push("--format=text", "--no-hints");
  if (file !== void 0) argv.push("--", file);
  const result = await execute(argv, options);
  if (result.code !== 0 && result.code !== 1) {
    throw new HyaloError(result, parseErrorEnvelope(result));
  }
  return result;
}
function raw(argv, options = {}) {
  return execute(argv, options);
}

// src/pi-runtime.ts
async function configForPi(options = {}) {
  const result = await raw(["config", "--format=json", "--no-hints"], options);
  if (result.code !== 0) throw new HyaloError(result);
  if (!result.stdout.trim()) throw new HyaloParseError("hyalo returned empty JSON", result);
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch (cause) {
    throw new HyaloParseError("hyalo returned invalid JSON", result, cause);
  }
  if (typeof parsed !== "object" || parsed === null) {
    throw new HyaloParseError("hyalo returned an invalid config object", result);
  }
  const top = parsed;
  const nested = typeof top.results === "object" && top.results !== null ? top.results : top;
  const candidateDir = typeof top.dir === "string" ? top.dir : nested.dir;
  const pi = typeof nested.pi === "object" && nested.pi !== null ? nested.pi : void 0;
  return {
    vaultDir: typeof candidateDir === "string" && candidateDir ? candidateDir : null,
    sessionSummary: pi?.session_summary === true
  };
}
export {
  HyaloAbortError,
  HyaloError,
  HyaloParseError,
  HyaloSpawnError,
  HyaloTimeoutError,
  HyaloTransportError,
  config,
  configForPi,
  createPiTransport,
  find,
  lint,
  raw,
  read,
  set,
  summary,
  task
};
