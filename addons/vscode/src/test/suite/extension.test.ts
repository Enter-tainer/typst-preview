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

	test('Executable Configuration Test', () => {
		assert.strictEqual('typst-ws', vscode.workspace.getConfiguration().get<string>('typst-ws.executable'));
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
