// Mock platform and spawn so every launcher branch can be checked on any host.
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const vm = require('vm');
const source = fs.readFileSync(path.join(__dirname, '../npm/bin/agy-auth'), 'utf8');
const rootPackage = require('../package.json');

for (const [platform, arch] of [
    ['darwin', 'x64'], ['darwin', 'arm64'], ['linux', 'x64'],
    ['linux', 'arm64'], ['win32', 'x64'],
]) {
    const packageName = `@ccvibe/agy-auth-${platform}-${arch}`;
    const binary = platform === 'win32' ? 'agy-auth.exe' : 'agy-auth';
    const manifest = require(`../npm/${platform}-${arch}/package.json`);
    assert.strictEqual(manifest.name, packageName);
    assert.deepStrictEqual(manifest.os, [platform]);
    assert.deepStrictEqual(manifest.cpu, [arch]);
    assert(manifest.files.includes(binary));
    assert(rootPackage.optionalDependencies[packageName]);

    // Exercise both flat and nested npm package layouts.
    for (const layout of ['flat', 'nested']) {
        const binDir = path.resolve('/mock/node_modules/agy-auth/npm/bin');
        const expected = layout === 'flat'
            ? path.join(binDir, '../../..', packageName, binary)
            : path.join(binDir, '../..', 'node_modules', packageName, binary);
        let spawned = false;
        let exited = false;
        vm.runInNewContext(source, {
            __dirname: binDir,
            console,
            process: {
                platform, arch, argv: ['node', 'agy-auth', '--version'],
                exit(code) { assert.strictEqual(code, 0); exited = true; },
            },
            require(name) {
                if (name === 'path') return path;
                if (name === 'fs') return { existsSync: candidate => candidate === expected };
                if (name === 'child_process') return {
                    spawn(file, args, options) {
                        assert.strictEqual(file, expected);
                        assert.strictEqual(JSON.stringify(args), '["--version"]');
                        assert.strictEqual(options.stdio, 'inherit');
                        spawned = true;
                        return { on(event, callback) {
                            assert.strictEqual(event, 'close');
                            callback(0);
                        } };
                    },
                };
                throw new Error(`Unexpected module: ${name}`);
            },
        });
        assert(spawned && exited, `${platform}/${arch}/${layout}`);
    }
}
console.log('Launcher checks passed for all 5 platforms (flat and nested layouts).');
