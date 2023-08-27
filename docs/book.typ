
#import "@preview/book:0.2.2": *

#show: book

#book-meta(
  title: "typst-book",
  summary: [
    #prefix-chapter("sample-page.typ")[Hello, typst]
  ]
)



// re-export page template
#import "/templates/page.typ": project
#let book-page = project
