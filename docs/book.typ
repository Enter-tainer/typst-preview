
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
  ]
)



// re-export page template
#import "/templates/page.typ": project
#let book-page = project
