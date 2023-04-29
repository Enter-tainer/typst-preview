import * as assert from 'assert';

// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from 'vscode';
import * as ext from '../../extension';

const jsonIs = (pred: (x: string, y: string) => void) => (x: unknown, y: unknown) =>
  pred(JSON.stringify(x), JSON.stringify(y));

suite('Extension Test Suite', () => {
	vscode.window.showInformationMessage('Start all tests.');

	test('Sample test', () => {
		assert.strictEqual(-1, [1, 2, 3].indexOf(5));
		assert.strictEqual(-1, [1, 2, 3].indexOf(0));
	});

	test('Executable Configuration Test', async () => {
		assert.strictEqual('', vscode.workspace.getConfiguration().get<string>('typst-preview.executable'), 'default path');
    
    assert.notStrictEqual('', await ext.getTypstWsPath(), 'never resolve empty string');
    assert.notStrictEqual(undefined, await ext.getTypstWsPath(), 'never resolve undefined');

    const state = ext.getTypstWsPath as unknown as any;
    let resolved: string;

    const BINARY_NAME = state.BINARY_NAME;
		assert.strictEqual('typst-ws', BINARY_NAME, 'default binary path is typst-ws');

    resolved = await ext.getTypstWsPath();
    assert.strictEqual(state.bundledPath, resolved, 'the bundle path exists and detected');

    state.BINARY_NAME = 'bad-typst-ws';
    assert.strictEqual('bad-typst-ws', await ext.getTypstWsPath(), 'fallback to binary name if not exists');

    const oldGetConfig = state.getConfig;
    state.getConfig = () => 'config-typst-ws';
    assert.strictEqual('config-typst-ws', await ext.getTypstWsPath(), 'use config if set');
    
    state.BINARY_NAME = 'typst-ws';
    state.getConfig = oldGetConfig;
    resolved = await ext.getTypstWsPath();
    assert.strictEqual(state.bundledPath, resolved, 'reactive state');

    resolved = await ext.getTypstWsPath();
    assert.strictEqual(true, resolved.endsWith(state.BINARY_NAME), 'exact file suffix');

    /// fast path should hit
    for (let i = 0; i < 1000; i++) {
      await ext.getTypstWsPath();
    }
  });

  test("FontPaths Configuration Test", async () => {

    /// check that default font paths should be []
    jsonIs(assert.strictEqual)(
      [],
      vscode.workspace.getConfiguration().get<string[]>("typst-preview.font-paths")
    );

    jsonIs(assert.strictEqual)(
      [], ext.getTypstWsFontArgs(undefined));

    jsonIs(assert.strictEqual)(
      [], ext.getTypstWsFontArgs([]));

    jsonIs(assert.strictEqual)(
      [], ext.codeGetTypstWsFontArgs());

    jsonIs(assert.strictEqual)(
      ["--font-path", "/path/to/font1", "--font-path", "/path/to/font2"], 
      ext.getTypstWsFontArgs(["/path/to/font1", "/path/to/font2"]));
  });
});
