// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from 'vscode';
import { ChildProcessWithoutNullStreams } from 'child_process';
import { spawn, sync as spawnSync } from 'cross-spawn';
import { readFile } from 'fs/promises';
import * as path from 'path';

async function loadHTMLFile(context: vscode.ExtensionContext, relativePath: string) {
	const filePath = path.resolve(__dirname, relativePath);
	const fileContents = await readFile(filePath, 'utf8');
	return fileContents;
}

export async function getTypstWsPath(): Promise<string> {
	const state = getTypstWsPath as unknown as any;
	(!state.BINARY_NAME) && (state.BINARY_NAME = "typst-ws");
	(!state.getConfig) && (state.getConfig = (
	  () => vscode.workspace.getConfiguration().get<string>('typst-preview.executable')));

	const bundledPath = path.resolve(__dirname, state.BINARY_NAME);
	const configPath = state.getConfig();

	if (state.bundledPath === bundledPath && state.configPath === configPath) {
		// console.log('getTypstWsPath cached', state.resolved);
		return state.resolved;
	}
	state.bundledPath = bundledPath;
	state.configPath = configPath;

	const executableExists = (path: string) => {
		return new Promise(resolve => {
			try {
				const spawnRet = spawn(path, ['--help'], {
					timeout: 1000, /// 1 second
				});
				spawnRet.on('error', () => resolve(false));
				spawnRet.on('exit', (code: number) => resolve(code === 0));
			} catch {
				resolve(false);
			}
		});
	};

	const resolvePath = async () => {
		console.log('getTypstWsPath resolving', bundledPath, configPath);

		if (configPath?.length) {
			return configPath;
		}
	
		if (await executableExists(bundledPath)) {
			return bundledPath;
		}
	
		vscode.window.showWarningMessage(
			`Failed to find ${state.BINARY_NAME} executable at ${bundledPath},`+
			`maybe we didn't ship it for your platform? Using ${state.BINARY_NAME} from PATH`);
		return state.BINARY_NAME;
	};

	return (state.resolved = await resolvePath());
}

export function getTypstWsFontArgs(fontPaths?: string[]): string[] {
	return (!fontPaths) ? [] : fontPaths.map(
	  (fontPath) => ["--font-path", fontPath]).flat();
}

export function codeGetTypstWsFontArgs(): string[] {
	return getTypstWsFontArgs(vscode.workspace.getConfiguration().get<string[]>(
	  'typst-preview.font-paths'));
}

const serverProcesses: Array<any> = [];
const shadowFilePaths: Array<string> = [];

function runServer(command: string, args: string[], outputChannel: vscode.OutputChannel): Promise<[string, ChildProcessWithoutNullStreams]> {
	const serverProcess = spawn(command, args, {
		env: {
			...process.env,
			// eslint-disable-next-line @typescript-eslint/naming-convention
			"RUST_BACKTRACE": "1",
		}
	});
	serverProcess.on('error', (err: any) => {
		console.error('Failed to start server process');
		vscode.window.showErrorMessage(`Failed to start typst-ws(${command}) process: ${err}`);
	});
	serverProcess.stdout.on('data', (data: Buffer) => {
		outputChannel.append(data.toString());
	});
	serverProcess.stderr.on('data', (data: Buffer) => {
		outputChannel.append(data.toString());
	});
	serverProcess.on('exit', (code: any) => {
		if (code !== null && code !== 0) {
			vscode.window.showErrorMessage(`typst-ws process exited with code ${code}`);
		}
		console.log(`child process exited with code ${code}`);
	});

	serverProcesses.push(serverProcesses);
	return new Promise((resolve, reject) => {
		serverProcess.stderr.on('data', (data: Buffer) => {
			if (data.toString().includes("Listening on")) {
				// port is 127.0.0.1:{port}, use regex
				const port = data.toString().match(/127\.0\.0\.1:(\d+)/)?.[1];
				if (port === undefined) {
					reject("Failed to get port from log: " + data.toString());
				} else {
					resolve([port, serverProcess]);
				}
			}
		});
	});
}

// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {

	// Use the console to output diagnostic information (console.log) and errors (console.error)
	// This line of code will only be executed once when your extension is activated
	// The command has been defined in the package.json file
	// Now provide the implementation of the command with registerCommand
	// The commandId parameter must match the command field in package.json
	const outputChannel = vscode.window.createOutputChannel('typst-preview');
	let disposable = vscode.commands.registerCommand('typst-preview.preview', async () => {
		// The code you place here will be executed every time your command is executed
		// Display a message box to the user
		const activeEditor = vscode.window.activeTextEditor;
		const refreshStyle = vscode.workspace.getConfiguration().get<string>('typst-preview.refresh') || "onSave";
		if (activeEditor) {
			const filePath = activeEditor.document.uri.fsPath;
			console.log('File path:', filePath);
			// get file dir using path
			const rootDir = path.dirname(filePath);
			const filename = path.basename(filePath);
			const shadowFilePath = path.join(rootDir, '.typst-preview.' + filename);
			const filePathToWatch = refreshStyle === "onSave" ? filePath : shadowFilePath;
			if (refreshStyle === "onType") {
				console.log('shadow file path:', shadowFilePath);
				// copy file content to shadow file
				const fileContent = activeEditor.document.getText();
				await vscode.workspace.fs.writeFile(vscode.Uri.file(shadowFilePath), Buffer.from(fileContent));
				const update = async () => {
					// save file content to shadow file
					if (activeEditor?.document) {
						const fileContent = activeEditor?.document.getText();
						await vscode.workspace.fs.writeFile(vscode.Uri.file(shadowFilePath), Buffer.from(fileContent));
					}
				};
				vscode.workspace.onDidChangeTextDocument(async (e) => {
					if (e.document === activeEditor?.document) {
						await update();
					}
				});
				shadowFilePaths.push(shadowFilePath);
			}
			const serverPath = await getTypstWsPath();
			console.log(`Watching ${filePathToWatch} for changes`);
			const [port, serverProcess] = await runServer(serverPath, [
				"--host", "127.0.0.1:23625",
				...codeGetTypstWsFontArgs(),
				"watch", filePathToWatch,
			], outputChannel);
			console.log('Launched server, port:', port);

			// Create and show a new WebView
			const panel = vscode.window.createWebviewPanel(
				'typst-ws-preview', // 标识符
				'Preview', // 面板标题
				vscode.ViewColumn.Beside, // 显示在编辑器的哪一侧
				{
					enableScripts: true, // 启用 JS
				}
			);

			panel.onDidDispose(async () => {
				// remove shadow file
				if (refreshStyle === "onType") {
					await vscode.workspace.fs.delete(vscode.Uri.file(shadowFilePath));
					console.log('removed shadow file');
				}
				serverProcess.kill();
				console.log('killed preview services');
				panel.dispose();
			});

			// 将已经准备好的 HTML 设置为 Webview 内容
			let html = await loadHTMLFile(context, "./frontend/index.html");
			html = html.replace(
			  /\/typst-webview-assets/g,
			  `${panel.webview
				.asWebviewUri(vscode.Uri.file(path.resolve(__dirname, "frontend")))
				.toString()}/typst-webview-assets`
			);
			panel.webview.html = html.replace("ws://127.0.0.1:23625", `ws://127.0.0.1:${port}`);
		} else {
			vscode.window.showWarningMessage('No active editor');
		}
	});

	context.subscriptions.push(disposable);
	process.on('SIGINT', () => {
		for (const serverProcess of serverProcesses) {
			serverProcess.kill();
		}
	});
}

// This method is called when your extension is deactivated
export async function deactivate() {
	console.log(shadowFilePaths);
	for (const shadowFilePath of shadowFilePaths) {
		try {
			await vscode.workspace.fs.delete(vscode.Uri.file(shadowFilePath));
		} catch (e) {
			console.error('Failed to remove shadow file: ' + shadowFilePath);
			console.error(e);
		}
	}
	console.log('killing preview services');
	for (const serverProcess of serverProcesses) {
		serverProcess.kill();
	}
}
