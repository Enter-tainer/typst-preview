# Typst Preview VSCode

Preview your Typst files in vscode instantly

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

![demo](demo.png)

https://user-images.githubusercontent.com/25521218/230921917-e535340e-a535-44c3-964b-d33bc0b0cc88.mp4

## Extension Settings

TODO
## Known Issues

- Ctrl+wheel is kind of strange
- Render output might be kind of blurry

## Acknowledgements

Big thanks to @zzh1996 for graciously granting me access to the GPT-4 API. Thanks to his generosity, I was able to complete this extension in such a short time.

(Yes, the javascript part is mostly done by GPT)
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
