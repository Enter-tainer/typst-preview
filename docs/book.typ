
#import "@preview/book:0.2.3": *

#show: book

#book-meta(
  title: "Typst Preview Book",
  description: "Document for typst preview ",
  authors: ("Enter-tainer", "Myriad-Dreamin"),
  language: "en",
  repository: "https://github.com/Enter-tainer/typst-preview",
  summary: [
    #prefix-chapter("intro.typ")[Get Started],
    = User Guide
    - #chapter("vscode.typ")[Use In VScode]
      - #chapter("config.typ")[Configuration]
    - #chapter("standalone.typ")[Standalone]
    = Developer Guide
    - #chapter("arch.typ")[Typst-Preview Architecture]
    - #chapter("dev.typ")[Set Up Development Environment]
    - #chapter("editor.typ")[Port Typst-Preview To Other Editors]
  ]
)



// re-export page template
#import "./templates/gh-page.typ": project
#let book-page = project
