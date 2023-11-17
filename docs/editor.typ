#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *

#show: book-page.with(title: "Port Typst-Preview to Other Editors")
#show link: underline

= Port Typst-Preview to Other Editors

Before we start, you should probably read #link("https://enter-tainer.github.io/typst-preview/arch.html")[Typst-Preview Architecture]. In short, to port Typst-Preview to other editors, you need to implement the "VSCode" part.

The editor extension should start typst-preview server, and connect to it using websocket. JSON messages are sent between the editor extension and the preview server.

The editor extension mainly does these things:

+ _In memory editing_: The editor extension should send dirty document to the preview server, without saving it to the disk. This is the most important part of the editor extension. With this, Typst-Preview can get the content of the document and render it every time when user types something.
+ _Source to preview jumping_: This is not necessary, but it's a nice feature to have. With this, the preview panel will be scrolled to the corresponding position when user clicks on the source code.
+ _Preview to source jumping_: This is quite similar to the previous one. With this, the source code will be scrolled to the corresponding position when user clicks on the preview panel.
+ _Compile status reporting_: With this, the preview server can report the compile status to the editor extension. The editor extension can show the compile status to the user.

== In memory editing

To implement in memory editing, the preview server implements an overlay virtual file system. This allows adding "memory files" on top of the real file system. The preview server will read the memory files(if present) instead of the real files when rendering the preview.

There are three types of messages related to in memory editing:

1. `updateMemoryFiles`: Update the content of the memory files. The `event` field should be set to `updateMemoryFiles`. The `files` field is a map from file path to file content. The file path should be absolute path.

This is used when the user types something in the editor. The editor extension should send the dirty document to the preview server. The preview server will update the content of the memory file in the overlay virtual file system.

Note that the editor doesn't need to debounce or throttle the `updateMemoryFiles` message. The preview server will only keep the latest `updateMemoryFiles` message.

Example:

```json
{
  "event": "updateMemoryFiles",
  "files": {
    "/home/mgt/proj/typst-preview/docs/dev.typ": "FULL_CONTENT_OF_THE_FILE",
  }
}
```

2. `removeMemoryFiles`: Remove the memory files. The `event` field should be set to `removeMemoryFiles`. The `files` field is a list of file paths. The file path should be absolute path.

This is used when the user saves the document. The editor extension should send the file path to the preview server. The preview server will remove the memory file in the overlay virtual file system. Therefore, the preview server will read the real file when rendering the preview.

Example:

```json
{
  "event": "removeMemoryFiles",
  "files": [
    "/home/mgt/proj/typst-preview/docs/dev.typ",
  ]
}
```

3. `syncMemoryFiles`: Sync the memory files. The `event` field should be set to `syncMemoryFiles`. The `files` field is a map from file path to file content. The file path should be absolute path.

This is used when the preview server starts. The editor extension should send the content of all the dirty files to the preview server. The preview server will discard all previous memory files, and use the memory files in the `syncMemoryFiles` message. This is also used in response to the `SyncEditorChanges` message from the preview server.

Example:

```json
{
  "event": "syncMemoryFiles",
  "files": {
    "/home/mgt/proj/typst-preview/docs/dev.typ": "FULL_CONTENT_OF_THE_FILE",
  }
}
```

== Source to preview jumping

To implement source to preview jumping, the editor extension should send the `SrcToDocJump` message to the preview server. The `event` field should be set to `panelScrollTo`. The `filepath` field is the absolute path of the file. The `line` field is the line number of the file. The `character` field is the character number of the file. The line number and the character number are 0-based.

Example:

```json
{
  "event": "panelScrollTo",
  "filepath": "/home/mgt/proj/typst-preview/docs/dev.typ",
  "line": 0,
  "character": 0
}
```

== Preview to source jumping

To implement preview to source jumping, the editor extension should listen to the `EditorScrollTo` message from the preview server. The `event` field should be `editorScrollTo`. The `filepath` field is the absolute path of the file. The `start` field is the start position of the selection. The `end` field is the end position of the selection.

A `(row, column)` pair is used to represent a position. Both `row` and `column` are 0-based. You can use this information to scroll the editor to the corresponding position.

Example:

```json
{
  "event": "editorScrollTo",
  "filepath": "/home/mgt/proj/typst-preview/docs/dev.typ",
  "start": [
    9,
    2
  ],
  "end": [
    9,
    32
  ]
}
```

== Compile Status Reporting

To implement compile status reporting, the editor extension act on `compileStatus` event. The `event` field should be `compileStatus`. The `kind` field is the compile status. The `kind` field can be one of the following values:

- `Compiling`
- `CompileSuccess`
- `CompileError`

Example:

```json
{
  "event": "compileStatus",
  "kind": "Compiling"
}
```


== References

Messages sent from the editor extension to the preview server, defined in `src/actor/editor.rs`.

```rs
#[serde(tag = "event")]
enum ControlPlaneMessage {
    #[serde(rename = "panelScrollTo")]
    SrcToDocJump(SrcToDocJumpRequest),
    #[serde(rename = "syncMemoryFiles")]
    SyncMemoryFiles(MemoryFiles),
    #[serde(rename = "updateMemoryFiles")]
    UpdateMemoryFiles(MemoryFiles),
    #[serde(rename = "removeMemoryFiles")]
    RemoveMemoryFiles(MemoryFilesShort),
}

pub struct SrcToDocJumpRequest {
    filepath: String,
    line: usize,
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    character: usize,
}

pub struct MemoryFilesShort {
    files: Vec<String>,
}

pub struct MemoryFiles {
    files: HashMap<String, String>,
}
```

Messages sent from the preview server to the editor extension, defined in `src/actor/editor.rs`.

```rs
#[serde(tag = "event")]
enum ControlPlaneResponse {
    #[serde(rename = "editorScrollTo")]
    EditorScrollTo(DocToSrcJumpInfo),
    #[serde(rename = "syncEditorChanges")]
    SyncEditorChanges(()),
}

pub struct DocToSrcJumpInfo {
    filepath: String,
    start: Option<(usize, usize)>, // row, column
    end: Option<(usize, usize)>,
}

#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}
```
