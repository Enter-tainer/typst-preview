# Typst Preview VSCode

Preview your Typst files in vscode instantly!

Install this extension from [marketplace](https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview), open command palette (Ctrl+Shift+P), and type `>Typst Preview:`.

https://github.com/Enter-tainer/typst-preview-vscode/assets/25521218/600529ce-8f42-4c2f-a224-b6b73e6ad017

This repo contains:
- the native part of the extension, in rust
- a vscode extension, in typescript

## How it works?

The extension watches for file changes, and incrementally compile your document to svg files. Then we use a websocket to send the rendered svg to the client. The client calculates the diff between the new svg and the old one, and apply the diff to the old one. This is done by a VDOM based incremental rendering technique.

With all these techniques, we can achieve instant preview on type.

## Acknowledgements

- [typst.ts](https://github.com/Myriad-Dreamin/typst.ts): typst.ts provide incremental svg export.
- [typst-lsp](https://github.com/nvarner/typst-lsp): The CI and the vscode extension are heavily inspired by typst-lsp.
