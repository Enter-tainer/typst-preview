# Change Log

All notable changes to the "typst-preview" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.


## 0.1.0

Initial release 

## 0.1.6

Add preview button

## 0.1.7

- Preview on type
- Add config entry for `typst-ws` path

## 0.2.1

- Bundle typst-ws within vsix. You no longer need to install typst-ws

## 0.2.2

- Fix server process not killed on exit(maybe)
- Add config for OnSave/OnType
- Add output channel for logging

## 0.2.3

- Performance Improvement: only update pages when they are visible. This should improve performance when you have a lot of pages.

## 0.2.4

- Automatically choose a free port to listen. This should fix the problem where you can't preview multiple files at the same time.
- Server will exit right after client disconnects, preventing resource leak.

## 0.3.0

- Upgrade typst to v0.3.0
- Fix panic when pages are removed

## 0.3.1

- Publish to OpenVSX
- allow configuring font paths

## 0.3.3

- Fix nix-ld compatibility by inheriting env vars(#33)

## 0.4.0

- Upgrade to typst v0.4.0

## 0.4.1

- Makes the WebSocket connection retry itself when it is closed, with a delay of 1 second.

## v0.5.0

- Upgrade to typst v0.5.0

## v0.5.1

- Performance improvement(#14): We now use typst.ts. We utilize a  [virtual DOM](https://en.wikipedia.org/wiki/Virtual_DOM) approach to diff and render the document. This is a **significant enhancement** of previewing document in `onType` mode in terms of resource savings and response time for changes.
- Cross jump between code and preview (#36): We implement SyncTeX-like feature for typst-preview. You can now click on the preview panel to jump to the corresponding code location, and vice versa. This feature is still experimental and may not work well in some cases. Please report any issues you encounter. 
- Sync preview position with cursor: We now automatically scroll the preview panel to the corresponding position of the cursor. This feature is controlled by `typst-preview.scrollSync`
- Open preview in separate window(#39): You can type `typst-preview.browser` in command palette to open the preview page in a separate browser.
- Links in preview panel: You can now click on links in the preview panel to open them in browser. The cross reference links are also clickable.
- Text selection in preview panel: You can now select text in the preview panel.

## v0.6.0

- Upgrade to typst v0.6.0
- Bug fixes:
  - #48: Webview cannot load frontend resources when VSCode is installed by scoop
  - #46: Preview to source jump not working after inserting new text in the source file
  - #52: Bug fix about VDOM operation
- Enhancement
  - #54: Only scroll the preview panel when the event is triggered by mouse

## v0.6.1

- Fix empty file preview. Previously, if you start with an empty file and type something, the preview will not be updated. This is now fixed.

## v0.6.2

- Fix #60 and #24. Now we watch dirty files in memory therefore no shadow file is needed. Due to the removal of disk read/write, this should also improve performance and latency.
- Preview on type is now enabled by default for new users. Existing users will not be affected.

## v0.6.3

- Fix #13, #63: Now ctrl+wheel zoom should zoom the content to the cursor position. And when the cursor is not within the document, the zoom sill works.

## v0.6.4

- Rename to Typst Preview.
- Add page level partial rendering. This should improve performance on long document. This is an experimental feature and is disabled by default. You can enable it by setting `typst-preview.partialRendering` to `true`.
- The binary `typst-preview` now can be used as a standalone typst server. You can use it to preview your document in browser. For example: `typst-preview ./assets/demo/main.typ --open-in-browser --partial-rendering`
- Fix #70: now you can launch many preview instances at the same time.

## v0.7.0

- Upgrade to typst v0.7.0
- Bug fixes
  - #77 #75: Previously arm64 devices will see a blank preview. This is now fixed.
  - #74: Previously when you open a file without opening in folder, the preview will not work. This is now fixed.

## v0.7.1

- Bug fixes:
  - fix #41. It is now possible to use Typst Preview in VSCode Remote.
  - fix #82. You can have preview button even when typst-lsp is not installed.
- Misc: We downgrade the ci image for Linux to Ubuntu 20.04. This should fix the problem where the extension cannot be installed on some old Linux distros.

## v0.7.2

- Bug fixes:
  - #79: We now put typst compiler and renderer in a dedicate thread. Therefore we should get more stable performance. 
  - #78: Currently only the latest compile/render request is processed. This should fix the problem where the preview request will queue up when you type too fast and the doc takes a lot of time to compile.
  - #81: We now use a more robust way to detect the whether to kill stale server process. This should fix the problem where the when preview tab will become blank when it becomes inactive for a while.
  - #87: Add enum description for `typst-preview.scrollSync`. Previously the description is missing.

## v0.7.3

- Bugfix: fix a subtle rendering issue, [typst.ts#306](https://github.com/Myriad-Dreamin/typst.ts/pull/306).

## v0.7.4

- Typst Preview Book is now available at https://enter-tainer.github.io/typst-preview/ ! You can find the documentation of Typst Preview there.
- Improved standalone usage: Use `typst-preview` without VSCode now becomes easier. All you need is `typst-preview --partial-rendering cool-doc.typ`. Take a look at https://enter-tainer.github.io/typst-preview/standalone.html
- Upgrade to typst.ts 0.4.0-rc2. This fixes a subtle incremental parsing bug.
- Partial rendering is now enabled by default. This should improve performance on long document. You can disable it by setting `typst-preview.partialRendering` to `false`.

## v0.7.5

- Fix #107: now VSCode variables like `${workspaceFolder}` can be used in `typst-preview.fontPaths`.
- Fix cannot open multiple preview tabs at the same time.

## v0.8.0

- Upgrade to typst v0.8.0
- Fix #111: Previously stroke related attributes are not rendered correctly. This is now fixed.
- Fix #105: The compiler will panic randomly. This is now fixed.
- Upstream bug fixes: https://github.com/Myriad-Dreamin/typst.ts/releases/tag/v0.4.0-rc3

## v0.8.1

- Bug fixes:
  - #121: Disable darkreader for preview panel. This should fix the problem where the preview panel is invisible when darkreader is installed in the browser.
  - #123: Fix a VDOM bug which may cause color/clip-path desync.
  - #124: Fix a race condition which may cause the webview process messages out of order, resulting in blank screen.
  - #125: Resizing the preview panel is not that laggy now.
- Features:
  - #120: We now show page breaks and center pages horizontally. By default we will choose the `vscode-sideBar-background` color as the page break color. If it is not distinguishable from white, we will use rgb(82, 86, 89) instead. 

## v0.8.2

- Features:
  - #142 The scroll position of the preview panel is now preserved when you switch between tabs.
  - #133 We now provide a button to show log when the server crashes. This should make debugging easier. You may also use the command `typst-preview.showLog` to show the log.
  - #129 A `--version` flag is now provided in the cli

- Bug fixes:
  - #137 Previously preview page might go blank when saving the file
  - #130 Previously you cannot watch a file in `/tmp`
  - #118 Previously the preview page might flash when you save the file

## v0.8.3

- Bug fixes:
  - #152 Do not pop up error message when the preview window is closed
  - #156 Fix shaking scrollbar/border
  - #161 #151 Should not panic when the file is not exist

- Features:
  - #157 Add a rough indicator for the current cursor position in the preview panel. You may enable this in configuration.

## v0.9.0

- Features:
  - Update to typst v0.9.0
  - Add a status indicator in status bar. When compile fails, it becomes red. Clicking on it will show the error message.

- Bug fixes:
  - #143 Scrolling is not that laggy now
  - #159 Fix a clip path bug

## v0.9.1

- Features:
  - #160: Slides mode is available now! You can enable use `typst-preview.preview-slide` command.
  - Allow adjust the status bar item

- Bug fixes:
  - Previously the `Compiling` status is never sent to the status bar item. This is now fixed.
  - #183 #128 Various rendering fix.

## v0.9.2

- Features:
  - You can now enable a preview panel in the sidebar. See `typst-preview.showInActivityBar`.
  - A new keybinding is added. You can trigger preview by using `Ctrl`/`Cmd` + `k` `v` now.

- Bug fix:
  - Scroll to cursor on 2-column documents is now improved.
