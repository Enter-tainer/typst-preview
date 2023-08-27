#import "/book.typ": book-page
#import "/templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *

#show: book-page.with(title: "Configuration")
#show link: underline

= Extension Configuration Options

The following are the available options for configuring the typst-preview extension:

1. *`typst-preview.executable`*: 
   - Type: `string` (path)
   - Description: The path to the executable of `typst-preview`, which should be installed locally. Usually, it is not necessary to modify this as `typst-preview` is bundled within the extension for all major platforms.
   - Default: Not provided

2. *`typst-preview.fontPaths`*:
   - Type: `Array<String>`
   - Description: List of additional paths to font assets used by typst-preview.
   - Items:
     - Type: `string`
     - Title: Font path
     - Description: Absolute path to a directory or file containing font assets.
   - Default: `[]`

3. *`typst-preview.refresh`*:
   - Type: `string`
   - Description: Refresh preview when the document is saved or when the document is changed. Choose between refreshing on save or on type.
   - Enum: `["onSave", "onType"]`
   - Default: `"onType"`

4. *`typst-preview.scrollSync`*:
   - Type: `string`
   - Description: Configure scroll sync mode. Disable automatic scroll sync or synchronize the preview with the cursor position when the selection changes.
   - Enum: `["never", "onSelectionChange"]`
   - Default: `onSelectionChange`

5. *`typst-preview.partialRendering`*:
   - Type: `boolean`
   - Description: Only render the visible part of the document. Improves performance but is still experimental. Useful for improving performance on large documents.
   - Default: `false`
