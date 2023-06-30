# [Typst Preview VSCode](https://github.com/Enter-tainer/typst-preview-vscode)

Preview your Typst files in vscode instantly!

## Features

- Instant preview on type, powered by VDOM based incremental rendering technique.
- Cross jump between code and preview like SyncTeX.
- Open preview in separate browser window.

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

![demo](demo.png)

https://github.com/Enter-tainer/typst-preview-vscode/assets/25521218/600529ce-8f42-4c2f-a224-b6b73e6ad017

## Extension Settings

TODO
## Known Issues

- Ctrl+wheel is kind of strange

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
