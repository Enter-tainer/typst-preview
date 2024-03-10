import { PreviewMode } from "typst-dom";

import van from "vanjs-core";
import { docMain } from "../doc";
const { div, button, textarea, select, option, span } = van.tags;

export interface WsArgs {
    url: string;
    previewMode: PreviewMode;
    isContentPreview: boolean;
}

const App = () => {
    return div(
        { class: "font-tool-main" },
        div(
            { class: "flex-row" },
            button({ class: "font-tool-btn", textContent: "Copy/Insert As | Show Rule" }),
            button({ class: "font-tool-btn", textContent: "Set Rule" }),
            button({ class: "font-tool-btn", textContent: "Font Args" }),
            button({ class: "font-tool-btn", textContent: "Size Args" }),
        ),
        div(
            { id: "typst-container-main", class: "flex-row", style: "text-align: center; align-items: center" },
            div({ id: "typst-app", textContent: "测试中文和English混排", style: "width: 100%; background: transparent" })
        ),
        textarea(
            { id: "font-tool-textarea", placeholder: "测试中文和English混排" }
        ),
        div(
            div(
                { style: "font-size: 20px", textContent: "Font Filter" }
            ),
            div(
                span(
                    { textContent: "Language" },
                ),
                select(
                    option(
                        { value: "0", textContent: "Chinese (Simplified)", selected: true }
                    ),
                ),
            ),
            div(
                span(
                    { textContent: "Style" },
                ),
                select(
                    option(
                        { value: "0", textContent: "Monospace", selected: true }
                    ),
                ),
                select(
                    option(
                        { value: "0", textContent: "Sans Serif", selected: true }
                    ),
                ),
            ),
        ),
        div(
            div(
                div(
                    { style: "font-size: 20px", textContent: "Font Selection" },
                ),
                button(
                    { textContent: "Configure path to fonts" },
                ),
                button(
                    { textContent: "Font Preview" },
                ),
            ),
            div(
                { textContent: "font1: 思源宋体" },
            ),
            div(
                { textContent: "font2: Linux Libertine" },
            ),
            div(
                { textContent: "more fonts" },
            )
        ),
    )
};

export async function fontToolMain(docArgs: WsArgs) {
    // append css variable --typst-preview-background-color
    const previewBackgroundColor = document.documentElement.style.getPropertyValue(
        '--typst-preview-background-color',
    );
    document.documentElement.style.setProperty(
        '--typst-preview-background-color',
        'transparent',
    );

    van.add(document.querySelector("#font-tool")!, App());

    docArgs.previewMode = PreviewMode.Doc;
    const docDispose = await docMain(docArgs);

    const dispose = () => {
        docDispose();
        document.documentElement.style.setProperty(
            '--typst-preview-background-color',
            previewBackgroundColor,
        );
    };

    return dispose;
}

// <div id="typst-container-main"><div id="typst-app"></div></div>