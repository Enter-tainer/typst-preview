#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *
#import "@preview/commute:0.1.0": node, arr, commutative-diagram

#show: book-page.with(title: "Use in VSCode")
#show link: underline

#let commands = json("../addons/vscode/package.json").contributes.commands

= Use in VSCode

This extension provides #commands.len() commands, they are:

#for cmd in commands [
  - #raw(cmd.command): #cmd.title 
    - #cmd.description
]

We also automatically scroll sync the preview with the editor when you are editing the source file. Also, when you click on the preview page, we will automatically scroll the editor to the corresponding position.
