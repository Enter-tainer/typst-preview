// The module 'vscode' contains the VS Code extensibility API
// Import the module and reference it with the alias vscode in your code below
import * as vscode from 'vscode';
import { ChildProcessWithoutNullStreams } from 'child_process';
import { spawn, sync as spawnSync } from 'cross-spawn';
import { readFile } from 'fs/promises';
import * as path from 'path';
import { WebSocket } from 'ws';

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
			`Failed to find ${state.BINARY_NAME} executable at ${bundledPath},` +
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

function getProjectRoot(currentPath: string): string | null {
	const checkIfPathContains = (base: string, target: string) => {
		const relativePath = path.relative(base, target);
		return !relativePath.startsWith('..') && !path.isAbsolute(relativePath);
	};
	const paths = vscode.workspace.workspaceFolders?.map(folder => folder.uri.fsPath).filter(folder => checkIfPathContains(folder, currentPath));
	if (!paths || paths.length === 0) {
		return null;
	} else {
		return paths[0];
	}
}

const serverProcesses: Array<any> = [];
const shadowFilePathMapping: Map<string, string> = new Map;
const activeTask = new Map<vscode.TextDocument, TaskControlBlock>();

interface JumpInfo {
	filepath: string,
	start: [number, number] | null,
	end: [number, number] | null,
}

async function processJumpInfo(activeEditor: vscode.TextEditor, jump: JumpInfo) {
	if (jump.start === null || jump.end === null) {
		return;
	}
	// check if shadowFilesPaths contains filepath
	const actualPath = shadowFilePathMapping.get(jump.filepath);
	if (actualPath !== undefined) {
		jump.filepath = actualPath;
	}
	// open this file and show in editor
	const doc = await vscode.workspace.openTextDocument(jump.filepath);
	const editor = await vscode.window.showTextDocument(doc, activeEditor.viewColumn);
	const startPosition = new vscode.Position(jump.start[0], jump.start[1]);
	const endPosition = new vscode.Position(jump.end[0], jump.end[1]);
	const range = new vscode.Range(startPosition, endPosition);
	editor.selection = new vscode.Selection(range.start, range.end);
	editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
}

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
			if (data.toString().includes("listening on")) {
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

interface LaunchTask {
	context: vscode.ExtensionContext,
	outputChannel: vscode.OutputChannel,
	activeEditor: vscode.TextEditor,
	bindDocument: vscode.TextDocument,
}

interface LaunchInBrowserTask extends LaunchTask {
	kind: 'browser',
}

interface LaunchInWebViewTask extends LaunchTask {
	kind: 'webview',
}

interface TaskControlBlock {
	/// related panel
	panel?: vscode.WebviewPanel;
	/// channel to communicate with typst-ws
	addonΠserver: WebSocket;
}

const panelScrollTo = async (bindDocument: vscode.TextDocument, activeEditor: vscode.TextEditor) => {
	const tcb = activeTask.get(bindDocument);
	if (tcb === undefined) {
		return;
	}
	const { addonΠserver } = tcb;
	addonΠserver.send(JSON.stringify({
		'event': 'panelScrollTo',
		'filepath': bindDocument.uri.fsPath,
		'line': activeEditor.selection.active.line,
		'character': activeEditor.selection.active.character,
	}));
};

const launchPreview = async (task: LaunchInBrowserTask | LaunchInWebViewTask) => {
	const {
		context,
		outputChannel,
		activeEditor,
		bindDocument,
	} = task;
	const filePath = bindDocument.uri.fsPath;
	// get file dir using path
	const rootDir = path.dirname(filePath);
	const filename = path.basename(filePath);

	const refreshStyle = vscode.workspace.getConfiguration().get<string>('typst-preview.refresh') || "onSave";
	const fontendPath = path.resolve(__dirname, "frontend");
	const { shadowFilePath } = await watchEditorFiles();
	const { serverProcess, port } = await launchTypstWs(task.kind === 'browser' ? fontendPath : null);

	const addonΠserver = new WebSocket("ws://127.0.0.1:23626");
	addonΠserver.addEventListener('message', async (message) => {
		const data = JSON.parse(message.data as string);
		console.log("recv jump data", data);
		await processJumpInfo(activeEditor, data);
	});
	serverProcess.on('exit', (code: any) => {
		addonΠserver.close();
		if (activeTask.has(bindDocument)) {
			activeTask.delete(bindDocument);
		}
	});

	const src2docHandler = (e: vscode.TextEditorSelectionChangeEvent) => {
		if (e.textEditor === activeEditor) {
			console.log('selection changed, sending src2doc jump request');
			panelScrollTo(bindDocument, activeEditor);
		}
	};
	
	const src2docHandlerDispose = vscode.window.onDidChangeTextEditorSelection(src2docHandler);

	switch (task.kind) {
		case 'browser': return launchPreviewInBrowser();
		case 'webview': return launchPreviewInWebView();
	}

	async function launchPreviewInBrowser() {
		// todo: may override the same file
		activeTask.set(bindDocument, {
			addonΠserver,
		});
	}

	async function launchPreviewInWebView() {
		const basename = path.basename(activeEditor.document.fileName);
		// Create and show a new WebView
		const panel = vscode.window.createWebviewPanel(
			'typst-ws-preview', // 标识符
			`${basename}(Preview)`, // 面板标题
			vscode.ViewColumn.Beside, // 显示在编辑器的哪一侧
			{
				enableScripts: true, // 启用 JS
			}
		);

		panel.onDidDispose(async () => {
			// todo: bindDocument.onDidDispose, but we did not find a similar way.
			activeTask.delete(bindDocument);
			// remove shadow file
			if (refreshStyle === "onType") {
				await vscode.workspace.fs.delete(vscode.Uri.file(shadowFilePath));
				console.log('removed shadow file');
			}
			serverProcess.kill();
			console.log('killed preview services');
			panel.dispose();
			src2docHandlerDispose.dispose();
		});

		// 将已经准备好的 HTML 设置为 Webview 内容
		let html = await loadHTMLFile(context, "./frontend/index.html");
		html = html.replace(
			/\/typst-webview-assets/g,
			`${panel.webview
				.asWebviewUri(vscode.Uri.file(fontendPath))
				.toString()}/typst-webview-assets`
		);
		panel.webview.html = html.replace("ws://127.0.0.1:23625", `ws://127.0.0.1:${port}`);
		activeTask.set(bindDocument, {
			panel,
			addonΠserver,
		});
	};

	async function watchEditorFiles() {
		const shadowFilePath = path.join(rootDir, '.typst-preview.' + filename);
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
			shadowFilePathMapping.set(shadowFilePath, filePath);
		}
		return { shadowFilePath };
	};

	async function launchTypstWs(frontendPath: null | string) {
		const filePathToWatch = refreshStyle === "onSave" ? filePath : shadowFilePath;
		const serverPath = await getTypstWsPath();
		console.log(`Watching ${filePathToWatch} for changes`);
		const projectRoot = getProjectRoot(filePath);
		const rootArgs = projectRoot ? ["--root", projectRoot] : [];
		const staticFileArgs = frontendPath ? ["--static-file-path", frontendPath] : [];
		const [port, serverProcess] = await runServer(serverPath, [
			"--data-plane-host", "127.0.0.1:23625",
			...rootArgs,
			...staticFileArgs,
			...codeGetTypstWsFontArgs(),
			"watch", filePathToWatch,
		], outputChannel);
		console.log('Launched server, port:', port);
		// window.typstWebsocket.send("current");
		return {
			serverProcess, port
		};
	};
};


// This method is called when your extension is activated
// Your extension is activated the very first time the command is executed
export function activate(context: vscode.ExtensionContext) {
	// Use the console to output diagnostic information (console.log) and errors (console.error)
	// This line of code will only be executed once when your extension is activated
	// The command has been defined in the package.json file
	// Now provide the implementation of the command with registerCommand
	// The commandId parameter must match the command field in package.json
	const outputChannel = vscode.window.createOutputChannel('typst-preview');

	let webviewDisposable = vscode.commands.registerCommand('typst-preview.preview', launchPrologue('webview'));
	let browserDisposable = vscode.commands.registerCommand('typst-preview.browser', launchPrologue('browser'));
	let syncDisposable = vscode.commands.registerCommand('typst-preview.sync', async () => {
		const activeEditor = vscode.window.activeTextEditor;
		if (activeEditor !== undefined) {
			panelScrollTo(activeEditor.document, activeEditor);
		}
	});

	context.subscriptions.push(webviewDisposable, browserDisposable, syncDisposable);
	process.on('SIGINT', () => {
		for (const serverProcess of serverProcesses) {
			serverProcess.kill();
		}
	});

	function launchPrologue(kind: 'browser' | 'webview') {
		return async () => {
			const activeEditor = vscode.window.activeTextEditor;
			if (!activeEditor) {
				vscode.window.showWarningMessage('No active editor');
				return;
			}
			const bindDocument = activeEditor.document;
			launchPreview({
				kind,
				context,
				outputChannel,
				activeEditor,
				bindDocument,
			});
		};
	};
}

// This method is called when your extension is deactivated
export async function deactivate() {
	console.log(shadowFilePathMapping);
	for (const [shadowFilePath, _] of shadowFilePathMapping) {
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
