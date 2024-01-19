#import "@preview/book:0.2.2": *

#import "./templates/gh-ebook.typ" as ebook

#show: ebook.project.with(title: "Typst Preview Book", spec: "book.typ")

// set a resolver for inclusion
#ebook.resolve-inclusion(it => include it)
