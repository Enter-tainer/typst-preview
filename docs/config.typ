#import "./book.typ": book-page
#import "./templates/page.typ": page-width, is-dark-theme
#import "@preview/fontawesome:0.1.0": *
#import "@preview/colorful-boxes:1.1.0": *

#show: book-page.with(title: "Configuration")
#show link: underline

= Extension Configuration Options

The following are the available options for configuring the typst-preview extension:

#let pkg_json = json("../addons/vscode/package.json")

#let config_item(key, cfg) = [
   + *#raw(key)*:
      - Type: #raw(cfg.type)
         #if cfg.type == "array" [
            - Items: #raw(cfg.items.type)
            - Description: #eval(cfg.items.description, mode: "markup")
         ]
      - Description: #eval(cfg.description, mode: "markup")
      #if cfg.at("enum", default: none) != none [
         - Valid values: #for (i, item) in cfg.enum.enumerate() [
            - #raw(item): #eval(cfg.enumDescriptions.at(i), mode: "markup")
         ] 
      ]
      #if type(cfg.default) == "string" {
         if cfg.default != "" [
            - Default: #raw(cfg.default)
         ] else [
            - Default: `""`
         ]
      } else if type(cfg.default) == "array" [
         - Default: [#cfg.default.join(",")]
      ] else [
         - Default: #cfg.default
      ]
]


#for (key, cfg) in pkg_json.contributes.configuration.properties {
   config_item(key, cfg)
}
