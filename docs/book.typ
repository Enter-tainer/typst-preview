
#import "@preview/book:0.2.2": *

#show: book

#book-meta(
  title: "typst-preview",
  description: "typst preview document",
  authors: ("Enter-tainer", "Myriad-Dreamin"),
  language: "en",
  repository: "https://github.com/Enter-tainer/typst-preview",
  summary: [
    #prefix-chapter("intro.typ")[Get Started],
    = User Guide
    - #chapter(none, section: "1")[Usage]
      - #chapter(none, section: "1.1")[Use In VScode]
      - #chapter(none, section: "1.2")[Standalone]
    - #chapter(none, section: "2")[Configuration]
    - #chapter(none, section: "3")[Report Bug]
    = Developer Guide
    - #chapter(none, section: "4")[Set Up Development Environment]
    - #chapter(none, section: "5")[Typst-Preview Architecture]
  ]
)



// re-export page template
#import "/templates/page.typ": project
#let book-page = project
