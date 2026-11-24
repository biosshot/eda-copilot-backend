import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

type CargoArtifact = {
    reason?: string;
    target?: { name?: string };
    executable?: string | null;
};

export function buildAnalysisExample(name: string): string {
    const build = spawnSync('cargo', [
        'build', '--release', '--locked', '--manifest-path', 'native/analyze/Cargo.toml',
        '--example', name, '--message-format=json',
    ], {
        encoding: 'utf8', windowsHide: true, timeout: 900_000, maxBuffer: 8 * 1024 * 1024,
    });
    assert.ifError(build.error);
    assert.equal(build.status, 0, build.stderr);
    const artifact = build.stdout.trim().split(/\r?\n/)
        .map(line => JSON.parse(line) as CargoArtifact)
        .find(entry => entry.reason === 'compiler-artifact' && entry.target?.name === name && entry.executable);
    assert.ok(artifact?.executable, 'Cargo did not return the ' + name + ' example executable');
    return artifact.executable;
}
