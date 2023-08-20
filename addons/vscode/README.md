# [Typst Preview VSCode](https://github.com/Enter-tainer/typst-preview)

Preview your Typst files in vscode instantly!

## Features

- Low latency preview: preview your document instantly on type. The incremental rendering technique makes the preview latency as low as possible.
- Open in browser: open the preview in browser, so you put it in another monitor. https://github.com/typst/typst/issues/1344
- Cross jump between code and preview: We implement SyncTeX-like feature for typst-preview. You can now click on the preview panel to jump to the corresponding code location, and vice versa.

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

![demo](demo.png)

https://github.com/Enter-tainer/typst-preview/assets/25521218/600529ce-8f42-4c2f-a224-b6b73e6ad017

## Extension Settings

- `typst-preview.executable`: The executable path of typst-ws. Typically you don't need to change this because we already bundle typst-ws within the extension for all major platforms.
- `typst-preview.fontPaths`: Absolute path to a directory or file containing font assets inaddition to the default font search paths.
- `typst-preview.refresh`: When to refresh the preview. Refresh preview when the document is saved or when the document is changed. Possible values are `onType` and `onSave`. Default is `onType`.
- `typst-preview.scrollSync`: Whether to sync the preview position with the cursor. Default is `onSelectionChange`.
- `typst-preview.partialRendering`: Whether to render only the visible part of the document. This provides better performance on long document. Default is `false`. This is an experimental feature.

## Known Issues

See [issues](https://github.com/Enter-tainer/typst-preview/issues?q=is%3Aissue+is%3Aopen+sort%3Aupdated-desc) on GitHub.

## Release Notes

### 0.1.0

Initial release 

### 0.1.6

Add preview button

### 0.1.7

- Preview on type
- Add config entry for `typst-ws` path

### 0.2.1

- Bundle typst-ws within vsix. You no longer need to install typst-ws

### 0.2.2

- Fix server process not killed on exit(maybe)
- Add config for OnSave/OnType
- Add output channel for logging

### 0.2.3

- Performance Improvement: only update pages when they are visible. This should improve performance when you have a lot of pages.

### 0.2.4

- Automatically choose a free port to listen. This should fix the problem where you can't preview multiple files at the same time.
- Server will exit right after client disconnects, preventing resource leak.

### 0.3.0

- Upgrade typst to v0.3.0
- Fix panic when pages are removed

### 0.3.1

- Publish to OpenVSX
- allow configuring font paths

### 0.3.3

- Fix nix-ld compatibility by inheriting env vars(#33)

### 0.4.0

- Upgrade to typst v0.4.0

### 0.4.1

- Makes the WebSocket connection retry itself when it is closed, with a delay of 1 second.

### v0.5.0

- Upgrade to typst v0.5.0

### v0.5.1

- Performance improvement(#14): We now use typst.ts. We utilize a  [virtual DOM](https://en.wikipedia.org/wiki/Virtual_DOM) approach to diff and render the document. This is a **significant enhancement** of previewing document in `onType` mode in terms of resource savings and response time for changes.
- Cross jump between code and preview (#36): We implement SyncTeX-like feature for typst-preview. You can now click on the preview panel to jump to the corresponding code location, and vice versa. This feature is still experimental and may not work well in some cases. Please report any issues you encounter. 
- Sync preview position with cursor: We now automatically scroll the preview panel to the corresponding position of the cursor. This feature is controlled by `typst-preview.scrollSync`
- Open preview in separate window(#39): You can type `typst-preview.browser` in command palette to open the preview page in a separate browser.
- Links in preview panel: You can now click on links in the preview panel to open them in browser. The cross reference links are also clickable.
- Text selection in preview panel: You can now select text in the preview panel.

### v0.6.0

- Upgrade to typst v0.6.0
- Bug fix:
  - #48: Webview cannot load frontend resources when VSCode is installed by scoop
  - #46: Preview to source jump not working after inserting new text in the source file
  - #52: Bug fix about VDOM operation
- Enhancement
  - #54: Only scroll the preview panel when the event is triggered by mouse

### v0.6.1

- Fix empty file preview. Previously, if you start with an empty file and type something, the preview will not be updated. This is now fixed.

### v0.6.2

- Fix #60 and #24. Now we watch dirty files in memory therefore no shadow file is needed. Due to the removal of disk read/write, this should also improve performance and latency.
- Preview on type is now enabled by default for new users. Existing users will not be affected.

### v0.6.3

- Fix #13, #63: Now ctrl+wheel zoom should zoom the content to the cursor position. And when the cursor is not within the document, the zoom sill works.

### v0.6.4

- Rename to Typst Preview.
- Add page level partial rendering. This should improve performance on long document. This is an experimental feature and is disabled by default. You can enable it by setting `typst-preview.partialRendering` to `true`.
- The binary `typst-preview` now can be used as a standalone preview server. You can use it to preview your document in browser. For example: `typst-preview ./assets/demo/main.typ --open-in-browser --partial-rendering`
- Fix #70: now you can launch many preview instances at the same time.

### v0.7.0

- Upgrade to typst v0.7.0
- Bug fixes
  - #77 #75: Previously arm64 devices will see a blank preview. This is now fixed.
  - #74: Previously when you open a file without opening in folder, the preview will not work. This is now fixed.

### v0.7.1

- Bug fixes:
  - fix #41. It is now possible to use Typst Preview in VSCode Remote.
  - fix #82. You can have preview button even when typst-lsp is not installed.
- Misc: We downgrade the ci image for Linux to Ubuntu 20.04. This should fix the problem where the extension cannot be installed on some old Linux distros.

### v0.7.2

- Bug fixes:
  - #79: We now put typst compiler and renderer in a dedicate thread. Therefore we should get more stable performance. 
  - #78: Currently only the latest compile/render request is processed. This should fix the problem where the preview request will queue up when you type too fast and the doc takes a lot of time to compile.
  - #81: We now use a more robust way to detect the whether to kill stale server process. This should fix the problem where the when preview tab will become blank when it becomes inactive for a while.
  - #87: Add enum description for `typst-preview.scrollSync`. Previously the description is missing.

### v0.7.3

- Bugfix: fix a subtle rendering issue, [typst.ts#306](https://github.com/Myriad-Dreamin/typst.ts/pull/306).
