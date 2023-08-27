# Typst Preview VSCode

Preview your Typst files in vscode instantly!

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

https://github.com/Enter-tainer/typst-preview/assets/25521218/600529ce-8f42-4c2f-a224-b6b73e6ad017

This repo contains:
- the native part of the extension, in rust
- a vscode extension, in typescript

## Features

- Low latency preview: preview your document instantly on type. The incremental rendering technique makes the preview latency as low as possible.
- Open in browser: open the preview in browser, so you put it in another monitor. https://github.com/typst/typst/issues/1344
- Cross jump between code and preview: We implement SyncTeX-like feature for typst-preview. You can now click on the preview panel to jump to the corresponding code location, and vice versa.

## How it works?

The extension watches for file changes, and incrementally compile your document to svg files. Then we use a websocket to send the rendered svg to the client. The client calculates the diff between the new svg and the old one, and apply the diff to the old one. This is done by a VDOM based incremental rendering technique.

With all these techniques, we can achieve instant preview on type.

## Use without VSCode

You can use the binary `typst-preview` as a standalone typst preview server. It can be used to preview your document in browser. For example: `typst-preview ./assets/demo/main.typ --partial-rendering`. This should be useful if you don't use VSCode but still want to experience the low latency preview.

## Acknowledgements

- [typst.ts](https://github.com/Myriad-Dreamin/typst.ts): typst.ts provide incremental svg export.
- [typst-lsp](https://github.com/nvarner/typst-lsp): The CI and the vscode extension are heavily inspired by typst-lsp.
