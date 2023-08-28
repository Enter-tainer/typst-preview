#import "@preview/book:0.2.2": *

#import "./templates/ebook.typ"

#show: ebook.project.with(title: "Typst Preview Book", spec: "book.typ")

// set a resolver for inclusion
#ebook.resolve-inclusion(it => include it)
