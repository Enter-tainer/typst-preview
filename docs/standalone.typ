#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *

#show: book-page.with(title: "Configuration")
#show link: underline

= Use Without VSCode

The `typst-preview` cli tool can be used to preview your documents without VSCode. It is quite similar to `typst watch` but with a few differences:
1. It will open a browser window. And it will automatically reload the page when you change the document.
2. It is faster than `typst watch` because it doesn't need to export the document to disk.

== Installation

Download `typst-preview` binary for your platform from #link("https://github.com/Enter-tainer/typst-preview/releases")[GitHub Release Page]. And put it in your `$PATH`.

== Typical Usage

Let's assume that you have a document `my-super-cool-doc.typ` in your current directory. You can use `typst-preview` to preview it.

Note that if you have extensions like dark-reader installed in your browser, you should *disable* them because the preview has a transparent background. If the background is set to black, you won't be able to see anything.

1. Use `typst-preview` to preview your document with partial rendering enabled. _*This is what you should do most of the time.*_

```bash 
typst-preview --partial-rendering \
  my-super-cool-doc.typ
```

2. Use `typst-preview` to do the same thing above but with `partital-rendering` disabled. Do this if you encounter any rendering issues.

```bash
typst-preview my-super-cool-doc.typ
```

3. Use `typst-preview` to preview your document with a custom host and port:

```bash
typst-preview \
  --host 0.0.0.0:8090 my-super-cool-doc.typ
```

4. Use `typst-preview` to preview your document with a custom root directory. This is useful when you want to preview a document that is not in the current directory.

```bash
typst-preview --root \
  /path/to/my-project \
  /path/to/my-project/cool-doc.typ
```

5. Use `typst-preview` to preview your document with a custom font directory. This is useful when you want to use a custom font in your document.

```bash
typst-preview --font-path \
  /path/to/my-fonts \
  /path/to/my-super-cool-doc.typ
```

Or use the environment variable `TYPST_FONT_PATH` to specify the font directory:

```bash
export TYPST_FONT_PATH=/path/to/my-fonts
typst-preview /path/to/my-super-cool-doc.typ
```

6. Use `typst-preview` to preview your document but don't open the browser automatically:

```bash
typst-preview --no-open \
  /path/to/my-super-cool-doc.typ
```

== CLI Options


```
Usage: typst-preview [OPTIONS] <INPUT>

Arguments:
  <INPUT>  

Options:
      --font-path <DIR>    Add additional directories to search for fonts
      --root <DIR>         Root directory for your project
      --host <HOST>        Host for the preview server [default: 127.0.0.1:23627]
      --no-open            Don't open the preview in the browser after compilation
      --partial-rendering  Only render visible part of the document. This can improve performance but still being experimental
  -h, --help               Print help
```
