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

async function bundleSourceExtension(scratch, name = 'extension') {
  const outfile = path.join(scratch, `${name}.mjs`);
  await build({
    entryPoints: [path.join(root, 'pi-package/extensions/hyalo.ts')], outfile,
    bundle: true, platform: 'node', format: 'esm',
    plugins: [{ name: 'schema-only-typebox', setup(builder) {
      builder.onResolve({ filter: /^typebox$/ }, () => ({ path: 'typebox', namespace: 'fixture' }));
      builder.onLoad({ filter: /.*/, namespace: 'fixture' }, () => ({ contents:
        'export const Type = new Proxy({}, { get: () => (...args) => args[0] });' }));
    } }],
  });
  return outfile;
}

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
        exec: async (_command, argv) => {
          if (argv[0] === 'config') return { code: 0, stderr: '', killed: false,
            stdout: '{"results":{"dir":null,"pi":{"session_summary":false}},"hints":[]}' };
          if (argv[0] === 'set' || argv[0] === 'task') return { code, stderr, killed: false,
            stdout: JSON.stringify({ results: { modified: ['note.md'] }, hints: [],
              effects: { paths: [{ file: 'note.md', state: 'committed' }] } }) };
          return { code, stderr, killed: false, stdout: argv[0] === 'find'
            ? '{"results":[],"total":0,"hints":[]}'
            : argv[0] === 'read' ? '{"results":{"content":"body"},"hints":[]}' : 'updated' };
        },
        registerTool: (tool) => tools.set(tool.name, tool),
        registerCommand() {}, on() {}, sendMessage() {},
      };
      (await import(pathToFileURL(outfile).href)).default(pi);
      for (const [name, params, expected] of [
        ['hyalo_find', {}, '{\n  "results": [],\n  "total": 0,\n  "hints": []\n}'],
        ['hyalo_find', { countOnly: true }, '0'],
        ['hyalo_read', { file: 'note.md' }, 'body'],
        ['hyalo_set', { file: 'note.md', property: 'status=done' }, '{\n  "modified": [\n    "note.md"\n  ]\n}'],
        ['hyalo_task', { file: 'note.md', mode: 'all' }, '{\n  "modified": [\n    "note.md"\n  ]\n}'],
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

test('Pi generic argv normalization and mutation guardrail use observed effects', async () => {
  const scratch = await fs.mkdtemp(path.join(os.tmpdir(), 'hyalo-pi-contracts-'));
  try {
    const outfile = await bundleSourceExtension(scratch);
    const tools = new Map();
    const calls = [];
    let mode = 'normal';
    const pi = {
      exec: async (_command, argv, options) => {
        calls.push(argv);
        if (mode === 'timeout') return { code: 2, stdout: '', stderr: '', killed: true };
        if (mode === 'abort') {
          assert.equal(options.signal.aborted, true);
          return { code: 2, stdout: '', stderr: '', killed: true };
        }
        if (argv[0] === 'config') return { code: 0, stderr: '', killed: false,
          stdout: JSON.stringify({ results: { dir: scratch, pi: {} }, hints: [] }) };
        if (argv[0] === 'lint') return { code: 1, stderr: '', killed: false,
          stdout: 'note.md:5: HYALO002 completed document has an unchecked task' };
        if (argv.includes('--internal-mutation-report')) {
          if (mode === 'unchanged') return { code: 0, stderr: '', killed: false,
            stdout: JSON.stringify({ results: { modified: [] }, hints: [], effects: {
              paths: [{ file: 'note.md', state: 'unchanged' }],
            } }) };
          if (mode === 'partial') return { code: 1, stdout: '', killed: false,
            stderr: JSON.stringify({ error: 'finalization failed', category: 'mutation_failure',
              effects: { paths: [
                { file: 'note.md', state: 'kept' },
                { file: 'restored.md', state: 'restored' },
                { file: '.hyalo-index', state: 'committed' },
              ] } }) };
          return { code: 0, stderr: 'authored-frontmatter warning\n', killed: false,
            stdout: JSON.stringify({ results: { modified: ['note.md'] }, hints: [], effects: {
              paths: [
                { file: 'note.md', state: 'committed' },
                { file: 'restored.md', state: 'restored' },
                { file: '.hyalo-index', state: 'committed' },
              ],
            } }) };
        }
        return { code: 0, stderr: 'generic warning\n', killed: false,
          stdout: '{"results":[],"total":0,"hints":[]}' };
      },
      registerTool: (tool) => tools.set(tool.name, tool),
      registerCommand() {}, on() {}, sendMessage() {},
    };
    (await import(pathToFileURL(outfile).href)).default(pi);

    const generic = tools.get('hyalo');
    await generic.execute('id', { subcommand: 'find',
      args: ['--format=json', '--jq=.total', '--', '--format=text'] });
    assert.deepEqual(calls.at(-1), ['find', '--format=json', '--jq=.total', '--', '--format=text']);
    await generic.execute('id', { subcommand: 'find', args: ['-f', '-odd.md'] });
    assert.deepEqual(calls.at(-1), ['find', '--format', 'text', '-f', '-odd.md']);
    await assert.rejects(
      generic.execute('id', { subcommand: 'find', args: ['--format=json', '--format', 'text'] }),
      /conflicting duplicate --format options/,
    );

    const setResult = await tools.get('hyalo_set').execute('id', {
      file: 'note.md', property: 'status=completed',
    });
    assert.equal(setResult.content.filter((item) => item.text.includes('authored-frontmatter warning')).length, 1);
    assert.equal(setResult.content.filter((item) => item.text.includes('HYALO002')).length, 1);
    const lintCalls = calls.filter((argv) => argv[0] === 'lint');
    assert.equal(lintCalls.length, 1);
    assert.equal(lintCalls[0].at(-1), 'note.md');

    mode = 'partial';
    const partial = await tools.get('hyalo_set').execute('id', {
      file: 'requested.md', property: 'status=completed',
    });
    assert.match(partial.content.map((item) => item.text).join('\n'), /finalization failed/);
    assert.match(partial.content.map((item) => item.text).join('\n'), /HYALO002/);
    assert.equal(calls.filter((argv) => argv[0] === 'lint').at(-1).at(-1), 'note.md');

    mode = 'unchanged';
    const checksBefore = calls.filter((argv) => argv[0] === 'config' || argv[0] === 'lint').length;
    await tools.get('hyalo_set').execute('id', { file: 'note.md', property: 'status=completed' });
    assert.equal(calls.filter((argv) => argv[0] === 'config' || argv[0] === 'lint').length, checksBefore);

    mode = 'timeout';
    const timedOut = await tools.get('hyalo_find').execute('id', {});
    assert.match(timedOut.content[0].text, /timed out after 60000ms/);
    mode = 'abort';
    const controller = new AbortController(); controller.abort();
    const aborted = await tools.get('hyalo_find').execute('id', {}, controller.signal);
    assert.match(aborted.content[0].text, /aborted/);
  } finally { await fs.rm(scratch, { recursive: true, force: true }); }
});

test('Pi observed mutation reports unavailable config and retries the lookup', async () => {
  const scratch = await fs.mkdtemp(path.join(os.tmpdir(), 'hyalo-pi-config-retry-'));
  try {
    const outfile = await bundleSourceExtension(scratch, 'config-retry');
    const tools = new Map();
    let configAttempts = 0;
    let lintCalls = 0;
    const pi = {
      exec: async (_command, argv) => {
        if (argv[0] === 'config') {
          configAttempts += 1;
          if (configAttempts === 1) {
            return { code: 2, stdout: '', stderr: 'temporary config failure', killed: false };
          }
          return { code: 0, stderr: '', killed: false,
            stdout: JSON.stringify({ results: { dir: scratch, pi: {} }, hints: [] }) };
        }
        if (argv[0] === 'lint') {
          lintCalls += 1;
          return { code: 1, stderr: '', killed: false,
            stdout: 'note.md:5: HYALO002 completed document has an unchecked task' };
        }
        assert.ok(argv.includes('--internal-mutation-report'));
        return { code: 0, stderr: '', killed: false,
          stdout: JSON.stringify({ results: { modified: ['note.md'] }, hints: [], effects: {
            paths: [{ file: 'note.md', state: 'committed' }],
          } }) };
      },
      registerTool: (tool) => tools.set(tool.name, tool),
      registerCommand() {}, on() {}, sendMessage() {},
    };
    (await import(pathToFileURL(outfile).href)).default(pi);

    const first = await tools.get('hyalo_set').execute('id', {
      file: 'note.md', property: 'status=completed',
    });
    const firstText = first.content.map((item) => item.text).join('\n');
    assert.match(firstText, /write succeeded but its lint status is unknown/);
    assert.match(firstText, /temporary config failure/);
    assert.equal(lintCalls, 0);

    const second = await tools.get('hyalo_set').execute('id', {
      file: 'note.md', property: 'status=completed',
    });
    assert.match(second.content.map((item) => item.text).join('\n'), /HYALO002/);
    assert.equal(configAttempts, 2, 'a transient config failure must not be cached');
    assert.equal(lintCalls, 1);
  } finally { await fs.rm(scratch, { recursive: true, force: true }); }
});

test('Pi typed diagnostics and set guardrail use the real hyalo binary', async () => {
  const scratch = await fs.mkdtemp(path.join(os.tmpdir(), 'hyalo-pi-real-'));
  try {
    await fs.writeFile(path.join(scratch, '.hyalo.toml'), `dir = "."\n\n[schema.types.note]\nrequired = ["title", "status"]\n\n[schema.types.note.properties.status]\ntype = "enum"\nvalues = ["draft", "completed"]\n\n[lint.rules.HYALO002]\nenabled = true\nseverity = "warning"\n`);
    await fs.writeFile(path.join(scratch, 'note.md'), '---\ntitle: Guarded\ntype: note\nstatus: draft\n---\n\n- [ ] unfinished\n');
    await fs.writeFile(path.join(scratch, 'broken.md'), '---\ntitle: [broken\n---\nbody\n');
    const outfile = await bundleSourceExtension(scratch, 'real');
    const tools = new Map();
    const pi = {
      exec: (_command, argv, options) => new Promise((resolve) => {
        execFile(binary, argv, { cwd: scratch, signal: options.signal, timeout: options.timeout },
          (error, stdout, stderr) => resolve({
            code: typeof error?.code === 'number' ? error.code : error ? 2 : 0,
            stdout, stderr, killed: error?.killed === true,
          }));
      }),
      registerTool: (tool) => tools.set(tool.name, tool),
      registerCommand() {}, on() {}, sendMessage() {},
    };
    (await import(pathToFileURL(outfile).href)).default(pi);
    const find = await tools.get('hyalo_find').execute('id', {});
    assert.equal(find.content.filter((item) => item.text.includes('unparsable frontmatter')).length, 1);
    const set = await tools.get('hyalo_set').execute('id', {
      file: 'note.md', property: 'status=completed',
    });
    const text = set.content.map((item) => item.text).join('\n');
    assert.match(text, /HYALO002/);
    assert.match(await fs.readFile(path.join(scratch, 'note.md'), 'utf8'), /status: completed/);
  } finally { await fs.rm(scratch, { recursive: true, force: true }); }
});
