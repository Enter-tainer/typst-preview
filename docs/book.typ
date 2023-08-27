
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
    - #chapter(none, section: "1")[Usage]
      - #chapter(none, section: "1.1")[Use In VScode]
      - #chapter("standalone.typ", section: "1.2")[Standalone]
    - #chapter("config.typ", section: "2")[Configuration]
    - #chapter(none, section: "3")[Report Bug]
    = Developer Guide
    - #chapter("arch.typ", section: "4")[Typst-Preview Architecture]
    - #chapter(none, section: "5")[Set Up Development Environment]
  ]
)



// re-export page template
#import "/templates/page.typ": project
#let book-page = project
