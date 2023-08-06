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
