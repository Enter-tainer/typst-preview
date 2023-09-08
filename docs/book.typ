
#import "@preview/book:0.2.2": *

#show: book

#book-meta(
  title: "Typst Preview Book",
  description: "Docuement for typst preview ",
  authors: ("Enter-tainer", "Myriad-Dreamin"),
  language: "en",
  repository: "https://github.com/Enter-tainer/typst-preview",
  summary: [
    #prefix-chapter("intro.typ")[Get Started],
    = User Guide
    - #chapter("vscode.typ", section: "1")[Use In VScode]
      - #chapter("config.typ", section: "1.1")[Configuration]
    - #chapter("standalone.typ", section: "2")[Standalone]
    = Developer Guide
    - #chapter("arch.typ", section: "3")[Typst-Preview Architecture]
    - #chapter("dev.typ", section: "4")[Set Up Development Environment]
    - #chapter("editor.typ", section: "5")[Port Typst-Preview To Other Editors]
  ]
)



// re-export page template
#import "./templates/page.typ": project
#let book-page = project
