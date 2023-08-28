#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *

#show: book-page.with(title: "Introduction")
#show link: underline

= Get Started

== Use in VSCode

Install the extension from #link("https://marketplace.visualstudio.com/items?itemName=mgt19937.typst-preview")[VSCode Marketplace].

#figure(
  image("./assets/preview.png", width: 90%),
  caption: [
    Preview in VSCode
  ],
)


Then open a typst document and press `Ctrl+Shift+P` to open the command palette. Type `Typst Preview:` and press enter. The preview page will be opened in a new tab.

== Use in other editors
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

#let cell = rect.with(
  inset: 12pt,
  width: 100%,
  height: auto,
  radius: 6pt,
)

#let pros_and_cons(p, c) = {
  let columns = if page-width <= 450pt {
    (1fr)
  } else {
    (1fr, 1fr)
  }
  let (pros_fill, pros_text) = if not is-dark-theme {
    (green.lighten(90%), black)
  } else {
    (black.lighten(10%), green.darken(10%))
  }
  let (cons_fill, cons_text) = if not is-dark-theme {
    (red.lighten(90%), black)
  } else {
    (black.lighten(10%), red.darken(10%))
  }
  table(
    columns: columns,
    inset: 10pt,
    align: horizon,
    stroke: none,
    cell(fill: pros_fill)[
      #text(fill: pros_text)[#fa-check() *Pros*: #p]
    ],
    cell(fill: cons_fill)[
      #text(fill: cons_text)[#fa-times() *Cons*: #c]
    ]
  )
}

- #link("https://typst.app")[_Official Typst Web App_]: The whole typst compiler runs in your browser as a wasm module. It compiles your document to pixmaps and renders them on a canvas. 
#pros_and_cons(
  [It is supported by the official typst team. And it is super fast because all these steps happens in your browser.],
  [It runs in your browser and cannot use local fonts: You have to upload your fonts to the web app.]
)
- _`typst watch`_: Using the official typst cli to watch your files and compile them to pdfs. Then open it with your pdf viewer.
#pros_and_cons(
  [It is supported by the official typst team. Pdf is also the target format of common use cases. This ensures that what you see is what you get.],
  [It is not responsive. You will have to manually save to docuement to trigger re-compile. It may takes a few seconds to compile and reload the pdf when your document gets big. It also does not support cross jump between code and preview.]
)
- _#link("https://github.com/nvarner/typst-lsp")[Typst LSP]'s Preview_: Typst LSP will watch your files in memory and compile them to pdfs. It will then open the pdfs in your VSCode. 
#pros_and_cons(
  [This is quite similar to `typst watch [input]`. But in addition, it can watch changes happening in your VSCode and trigger re-compile. So you can see the preview in real time.],
  [It is not supported by the official typst team. It also does not support cross jump between code and preview.]
)
- #link("https://github.com/Enter-tainer/typst-preview")[_Typst Preview_]: We watch changes in your VSCode editor and send changes to the compiler. The compiler use #link("https://github.com/Myriad-Dreamin/typst.ts")[typst.ts] to export the document to an internal incremental IR and send it the preview frontend. The frontend then uses this IR to render the docuement. 
#pros_and_cons(
  [It is super fast and responsive. It supports cross jump between code and preview. It also supports local fonts.],
  [It is not supported by the official typst team. It may also introduce some bugs because it have a different rendering backend from the official typst web app.]
)

== Acknowledgement
This book is written in typst, and compiled to HTML using #link("https://github.com/Myriad-Dreamin/typst-book/")[typst book]
