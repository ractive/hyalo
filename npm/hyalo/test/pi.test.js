'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { pathToFileURL } = require('node:url');
const { build } = require('esbuild');
const execFileAsync = promisify(execFile);
const root = path.resolve(__dirname, '../../..');
const binary = process.env.HYALO_BIN || path.join(root, `target/release/hyalo${process.platform === 'win32' ? '.exe' : ''}`);

test('Pi source and offline init tools display successful warnings as separate text', async () => {
  const scratch = await fs.mkdtemp(path.join(os.tmpdir(), 'hyalo-pi-diagnostics-'));
  try {
    await execFileAsync(binary, ['init', '--pi', '--dir', 'vault'], { cwd: scratch });
    for (const [layout, extension] of [
      ['source', path.join(root, 'pi-package/extensions/hyalo.ts')],
      ['offline', path.join(scratch, '.pi/extensions/hyalo.ts')],
    ]) {
      const outfile = path.join(scratch, `${layout}.mjs`);
      await build({
        entryPoints: [extension], outfile, bundle: true, platform: 'node', format: 'esm',
        plugins: [{ name: 'schema-only-typebox', setup(builder) {
          builder.onResolve({ filter: /^typebox$/ }, () => ({ path: 'typebox', namespace: 'fixture' }));
          builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents:
            'export const Type = new Proxy({}, { get: () => (...args) => args[0] });' }));
        } }],
      });
      const tools = new Map();
      let stderr = 'warning: index older than vault\n';
      let code = 0;
      const pi = {
        exec: async (_command, argv) => ({ code, stderr, killed: false, stdout: argv[0] === 'find'
          ? '{"results":[],"total":0,"hints":[]}'
          : argv[0] === 'read' ? '{"results":{"content":"body"},"hints":[]}' : 'updated' }),
        registerTool: (tool) => tools.set(tool.name, tool),
        registerCommand() {}, on() {}, sendMessage() {},
      };
      (await import(pathToFileURL(outfile).href)).default(pi);
      for (const [name, params, expected] of [
        ['hyalo_find', {}, '{\n  "results": [],\n  "total": 0,\n  "hints": []\n}'],
        ['hyalo_find', { countOnly: true }, '0'],
        ['hyalo_read', { file: 'note.md' }, 'body'],
        ['hyalo_set', { file: 'note.md', property: 'status=done' }, 'updated'],
        ['hyalo_task', { file: 'note.md', mode: 'all' }, 'updated'],
      ]) {
        const result = await tools.get(name).execute('id', params);
        assert.deepEqual(result.content, [
          { type: 'text', text: expected }, { type: 'text', text: `Stderr:\n${stderr}` },
        ], `${layout}: ${name}`);
        assert.equal(result.details, undefined);
      }
      stderr = '';
      assert.equal((await tools.get('hyalo_find').execute('id', {})).content.length, 1);
      code = 1; stderr = 'failed diagnostic';
      const failed = await tools.get('hyalo_find').execute('id', {});
      assert.equal(failed.content.filter((item) => item.text.includes(stderr)).length, 1);
    }
  } finally { await fs.rm(scratch, { recursive: true, force: true }); }
});
