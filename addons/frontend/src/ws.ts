import { PreviewMode, SvgDocument } from "./svg-doc";
import {
    rendererBuildInfo,
    createTypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs"; 0
import renderModule from "@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm?url";
// @ts-ignore
// import { RenderSession as RenderSession2 } from "@myriaddreamin/typst-ts-renderer/pkg/wasm-pack-shim.mjs";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { WebSocketSubject, webSocket } from 'rxjs/webSocket';
import { Subject, buffer, debounceTime, tap } from "rxjs";

// for debug propose
// queryObjects((window as any).TypstRenderSession);
(window as any).TypstRenderSession = RenderSession;
// (window as any).TypstRenderSessionKernel = RenderSession2;

const enc = new TextEncoder();
const dec = new TextDecoder();
const NOT_AVAIABLE = "current not avalible";
const COMMA = enc.encode(",");
export interface WsArgs {
    url: string;
    previewMode: PreviewMode;
    isContentPreview: boolean;
}

export async function wsMain({ url, previewMode, isContentPreview }: WsArgs) {
    if (!url) {
        const hookedElem = document.getElementById("typst-app");
        if (hookedElem) {
            if (isContentPreview) {
                hookedElem.innerHTML = `<span style="margin: 0px 5px">No Content</span>`;
            } else {
                hookedElem.innerHTML = "";
            }
        }
        return () => { };
    }

    let disposed = false;
    let subject: WebSocketSubject<ArrayBuffer> | undefined = undefined;
    const listeners: [any, string, any][] = [];
    function addWindowEventListener<K extends keyof WindowEventMap>(
        type: K, listener: (this: Window, ev: WindowEventMap[K]) => any): void;
    function addWindowEventListener(event: string, listener: any) {
        window.addEventListener(event, listener);
        listeners.push([window, event, listener]);
    }


    function createSvgDocument(wasmDocRef: RenderSession) {
        const hookedElem = document.getElementById("typst-app")!;
        if (hookedElem.firstElementChild?.tagName !== "svg") {
            hookedElem.innerHTML = "";
        }
        const resizeTarget = document.getElementById('typst-container-main')!;

        const svgDoc = new SvgDocument(hookedElem!, wasmDocRef, {
            previewMode,
            isContentPreview,
            // set rescale target to `body`
            retrieveDOMState() {
                return {
                    // reserving 1px to hide width border
                    width: resizeTarget.clientWidth + 1,
                    // reserving 1px to hide width border
                    height: resizeTarget.offsetHeight,
                    boundingRect: resizeTarget.getBoundingClientRect(),
                };
            },
        });

        // drag (panal resizing) -> rescaling
        // window.onresize = () => svgDoc.rescale();
        addWindowEventListener("resize", () => svgDoc.addViewportChange());
        addWindowEventListener("scroll", () => svgDoc.addViewportChange());

        if (previewMode === PreviewMode.Slide) {
            {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.addEventListener("input", () => {
                        if (inpPageSelector.value.length === 0) {
                            return;
                        }
                        const page = Number.parseInt(inpPageSelector.value);
                        svgDoc.setPartialPageNumber(page);
                    });
                }
            }

            const focusInput = () => {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.focus();
                }
            }

            const blurInput = () => {
                const inpPageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                if (inpPageSelector) {
                    inpPageSelector.blur();
                }
            }

            const updateDiff = (diff: number) => () => {
                const pageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;

                if (pageSelector) {
                    console.log("updateDiff", diff);
                    const v = pageSelector.value;
                    if (v.length === 0) {
                        return;
                    }
                    const page = Number.parseInt(v) + diff;
                    if (page <= 0) {
                        return;
                    }
                    if (svgDoc.setPartialPageNumber(page)) {
                        pageSelector.value = page.toString();
                        blurInput();
                    }
                }
            }

            const updatePrev = updateDiff(-1);
            const updateNext = updateDiff(1);

            const pagePrevSelector = document.getElementById("typst-page-prev-selector");
            if (pagePrevSelector) {
                pagePrevSelector.addEventListener("click", updatePrev);
            }
            const pageNextSelector = document.getElementById("typst-page-next-selector");
            if (pageNextSelector) {
                pageNextSelector.addEventListener("click", updateNext);
            }

            const toggleHelp = () => {
                const help = document.getElementById("typst-help-panel");
                console.log("toggleHelp", help);
                if (help) {
                    help.classList.toggle("hidden");
                }
            };

            const removeHelp = () => {
                const help = document.getElementById("typst-help-panel");
                if (help) {
                    help.classList.add("hidden");
                }
            }

            const helpButton = document.getElementById("typst-top-help-button");
            helpButton?.addEventListener('click', toggleHelp);

            window.addEventListener("keydown", (e) => {
                let handled = true;
                switch (e.key) {
                    case "ArrowLeft":
                    case "ArrowUp":
                        blurInput();
                        removeHelp();
                        updatePrev();
                        break;
                    case " ":
                    case "ArrowRight":
                    case "ArrowDown":
                        blurInput();
                        removeHelp();
                        updateNext();
                        break;
                    case "h":
                        blurInput();
                        toggleHelp();
                        break;
                    case "g":
                        removeHelp();
                        focusInput();
                        break;
                    case "Escape":
                        removeHelp();
                        blurInput();
                        handled = false;
                        break;
                    default:
                        handled = false;
                }

                if (handled) {
                    e.preventDefault();
                }
            });
        }

        return svgDoc;
    }

    function setupSocket(svgDoc: SvgDocument): () => void {
        // todo: reconnect setTimeout(() => setupSocket(svgDoc), 1000);
        subject = webSocket<ArrayBuffer>({
            url,
            binaryType: "arraybuffer",
            serializer: t => t,
            deserializer: (event) => event.data,
            openObserver: {
                next: (e) => {
                    const sock = e.target;
                    console.log('WebSocket connection opened', sock);
                    window.typstWebsocket = sock as any;
                    svgDoc.reset();
                    window.typstWebsocket.send("current");
                }
            },
            closeObserver: {
                next: (e) => {
                    console.log('WebSocket connection closed', e);
                    subject?.unsubscribe();
                    if (!disposed) {
                        setTimeout(() => setupSocket(svgDoc), 1000);
                    }
                }
            }
        });
        const dispose = () => {
            disposed = true;
            svgDoc.dispose();
            for (const [target, evt, listener] of listeners.splice(0, listeners.length)) {
                target.removeEventListener(evt, listener);
            }
            subject?.complete();
        };

        // window.typstWebsocket = new WebSocket("ws://127.0.0.1:23625");

        const batchMessageChannel = new Subject<ArrayBuffer>();

        subject.subscribe({
            next: (data) => batchMessageChannel.next(data), // Called whenever there is a message from the server.
            error: err => console.log("WebSocket Error: ", err), // Called if at any point WebSocket API signals some kind of error.
            complete: () => console.log('complete') // Called when connection is closed (for whatever reason).
        });

        batchMessageChannel
            .pipe(buffer(batchMessageChannel.pipe(debounceTime(0))))
            .pipe(tap(dataList => { console.log(`batch ${dataList.length} messages`) }))
            .subscribe((dataList) => {
                dataList.map(processMessage)
            });

        function processMessage(data: ArrayBuffer) {
            if (!(data instanceof ArrayBuffer)) {
                if (data === NOT_AVAIABLE) {
                    return;
                }

                console.error("WebSocket data is not a ArrayBuffer", data);
                return;
            }

            const buffer = data;
            const messageData = new Uint8Array(buffer);
            console.log('recv', messageData);

            const message_idx = messageData.indexOf(COMMA[0]);
            const message = [
                dec.decode(messageData.slice(0, message_idx).buffer),
                messageData.slice(message_idx + 1),
            ];
            // console.log(message[0], message[1].length);
            if (isContentPreview) {
                if ((message[0] === "jump" || message[0] === "partial-rendering" || message[0] === "cursor")) {
                    return;
                }
            }

            if (message[0] === "jump") {
                // todo: aware height padding
                const [page, x, y] = dec
                    .decode((message[1] as any).buffer)
                    .split(" ")
                    .map(Number);

                let pageToJump = page;

                if (previewMode === PreviewMode.Slide) {
                    const pageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
                    if (svgDoc.setPartialPageNumber(page)) {
                        if (pageSelector) {
                            pageSelector.value = page.toString();
                        }
                        // pageToJump = 1;
                        // todo: hint location
                        return;
                    } else {
                        return;
                    }
                }

                const rootElem =
                    document.getElementById("typst-app")?.firstElementChild;
                if (rootElem) {
                    /// Note: when it is really scrolled, it will trigger `svgDoc.addViewportChange`
                    /// via `window.onscroll` event
                    window.handleTypstLocation(rootElem, pageToJump, x, y);
                }
                return;
            } else if (message[0] === "cursor") {
                // todo: aware height padding
                const [page, x, y] = dec
                    .decode((message[1] as any).buffer)
                    .split(" ")
                    .map(Number);
                console.log("cursor", page, x, y);
                svgDoc.setCursor(page, x, y);
                svgDoc.addViewportChange(); // todo: synthesizing cursor event
                return;
            } else if (message[0] === "partial-rendering") {
                console.log("Experimental feature: partial rendering enabled");
                svgDoc.setPartialRendering(true);
                return;
            } else if (message[0] === "outline") {
                console.log("Experimental feature: outline rendering");
                return;
            }

            svgDoc.addChangement(message as any);
        };

        // 当WebSocket连接关闭时
        // window.typstWebsocket.addEventListener("close", () => {
        //     console.log("WebSocket connection closed");
        // });

        // 当发生错误时
        // window.typstWebsocket.addEventListener("error", (error) => {
        //     console.error("WebSocket Error: ", error);
        // }

        // window.typstWebsocket.addEventListener("open", () => {
        //     console.log("WebSocket connection opened");
        //     svgDoc.reset();
        //     window.typstWebsocket.send("current");
        // });

        // window.typstWebsocket.addEventListener("close", () => {
        //     setTimeout(() => setupSocket(svgDoc), 1000);
        // });

        // 当收到WebSocket数据时
        // window.typstWebsocket.addEventListener("message", event => processMessage(event.data));

        return dispose;
    }

    let plugin = createTypstRenderer();
    await plugin.init({ getModule: () => renderModule });

    return new Promise<() => void>((resolveDispose) =>
        plugin.runWithSession(kModule /* module kernel from wasm */ => {
            return new Promise(async (kernelDispose) => {
                console.log("plugin initialized, build info:", await rendererBuildInfo());

                const wsDispose = setupSocket(createSvgDocument(kModule));

                // todo: plugin init and setup socket at the same time
                resolveDispose(() => {
                    // dispose ws first
                    wsDispose();
                    // dispose kernel then
                    kernelDispose(undefined);
                });
            })
        }));
};
