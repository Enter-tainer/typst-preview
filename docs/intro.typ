#import "/book.typ": book-page
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *


#show: book-page.with(title: "Introduction")
#show link: underline

= Get Started

== Usage

=== Use in VSCode

Install the extension from #link("https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview")[VSCode Marketplace].

#figure(
  image("./assets/preview.png", width: 90%),
  caption: [
    Preview in VSCode
  ],
)


Then open a typst document and press `Ctrl+Shift+P` to open the command palette. Type `Typst Preview:` and press enter. The preview page will be opened in a new tab.

=== Use in other editors
#slantedColorbox(
  title: "Note",
  color: "blue",
  radius: 2pt,
  width: auto
)[
  When used in other editors, you will have to start the preview server manually. And the cross jump feature will not work. You will also need to trigger compilation manually by saving your document.
]

Download the `typst-preview` binary for your platform from #link("https://github.com/Enter-tainer/typst-preview/releases")[GitHub Release Page]. Then start the server by running `typst-preview [your-source-file]` in your terminal. The server will open the preview page in your browser by default.

== Why Typst Preview?

#link("https://github.com/Enter-tainer/typst-preview")[Typst Preview] provides low latency preview experience for Typst. Comparing to its alternatives, it is more responsive and has better support for Typst's features like cross jump between code and preview.

- #link("https://typst.app")[Official Typst Web App]: The whole typst compiler runs in your browser as a wasm module. It compiles your document to pixmaps and renders them on a canvas. 
  - #fa-check() *Pros*: It is supported by the official typst team. And it is super fast because all these steps happens in your browser.
  - #fa-times() *Cons*: It runs in your browser and cannot use local fonts: You have to upload your fonts to the web app.
- `typst watch [input]`: Using the official typst cli to watch your files and compile them to pdfs. Then open it with your pdf viewer.
  - #fa-check() *Pros*: It is supported by the official typst team. Pdf is also the target format of common use cases. This ensures that what you see is what you get.
  - #fa-times() *Cons*: It is not responsive. You will have to manually save to docuement to trigger re-compile. It may takes a few seconds to compile and reload the pdf when your document gets big. It also does not support cross jump between code and preview.
- Preview functionality of #link("https://github.com/nvarner/typst-lsp")[Typst LSP]: Typst LSP will watch your files in memory and compile them to pdfs. It will then open the pdfs in your VSCode. 
  - #fa-check() *Pros*: This is quite similar to `typst watch [input]`. But in addition, it can watch changes happening in your VSCode and trigger re-compile. So you can see the preview in real time.
  - #fa-times() *Cons*: It is not supported by the official typst team. It also does not support cross jump between code and preview.
- Our #link("https://github.com/Enter-tainer/typst-preview")[Typst Preview]: We watch changes in your VSCode editor and send changes to the compiler. The compiler use #link("https://github.com/Myriad-Dreamin/typst.ts")[typst.ts] to export the document to an internal incremental IR and send it the preview frontend. The frontend then uses this IR to render the docuement. 
  - #fa-check() *Pros*: It is super fast and responsive. It supports cross jump between code and preview. It also supports local fonts.
  - #fa-times() *Cons*: It is not supported by the official typst team. It may also introduce some bugs because it have a different rendering backend from the official typst web app.
