#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *
#import "@preview/commute:0.1.0": node, arr, commutative-diagram

#show: book-page.with(title: "Set Up Development Environment")
#show link: underline

= Set Up Development Environment

+ Install Node.js & Yarn

  You need to install Node.js and Yarn for your platform first.
  
+ Install npm dependencies for `addons/frontend` and `addons/vscode`

  ```bash
  cd addons/frontend
  yarn install
  cd ../vscode
  yarn install
  ```

+ Install rust toolchain. The common option is to use #link("https://rustup.rs/")[rustup].

+ Build and run:

  - To build and debug the VSCode extension: press `F5` in VSCode. And a new VSCode window will pop up. Open a typst source file in the new window, and you will see the preview button in the top right corner of the editor.
  - To build and run the binary only, run `cargo run` in the root directory of the project.
