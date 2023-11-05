import "./typst.css";
import { PreviewMode, SvgDocument } from "./svg-doc";
import {
    rendererBuildInfo,
    createTypstRenderer,
} from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import renderModule from "@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm?url";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { webSocket } from 'rxjs/webSocket';
import { Subject, buffer, debounceTime, tap } from "rxjs";

const enc = new TextEncoder();
const dec = new TextDecoder();
const NOT_AVAIABLE = "current not avalible";
const COMMA = enc.encode(",");

function createSvgDocument(wasmDocRef: RenderSession, previewMode: PreviewMode) {
    const hookedElem = document.getElementById("typst-app");
    const resizeTarget = document.documentElement;

    const svgDoc = new SvgDocument(hookedElem!, wasmDocRef, {
        previewMode,
        // set rescale target to `body`
        retrieveDOMState() {
            return {
                // reserving 1px to hide width border
                width: resizeTarget.clientWidth + 1,
                boundingRect: resizeTarget.getBoundingClientRect(),
            };
        },
    });

    // drag (panal resizing) -> rescaling
    // window.onresize = () => svgDoc.rescale();
    window.addEventListener("resize", () => svgDoc.addViewportChange());
    window.addEventListener("scroll", () => svgDoc.addViewportChange());

    if (previewMode === PreviewMode.Slide) {
        const pageSelector = document.getElementById("typst-page-selector") as HTMLSelectElement | undefined;
        if (pageSelector) {
            pageSelector.addEventListener("input", () => {
                if (pageSelector.value.length === 0) {
                    return;
                }
                const page = Number.parseInt(pageSelector.value);
                svgDoc.setPartialPageNumber(page);
            });

            const updateDiff = (diff: number) => () => {
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

            window.addEventListener("keydown", (e) => {
                if (e.key === "ArrowLeft") {
                    updatePrev();
                } else if (e.key === "ArrowRight") {
                    updateNext();
                } else if (e.key === " ") {
                    updateNext();
                }
            });
        }

    }

    return svgDoc;
}

export function wsMain(previewMode: PreviewMode) {
    function setupSocket(svgDoc: SvgDocument) {
        // todo: reconnect setTimeout(() => setupSocket(svgDoc), 1000);
        const subject = webSocket<ArrayBuffer>({
            url: "ws://127.0.0.1:23625",
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
                    subject.unsubscribe();
                    setTimeout(() => setupSocket(svgDoc), 1000);
                }
            }
        });
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
    }

    let plugin = createTypstRenderer();
    plugin
        .init({
            getModule: () => renderModule,
        })
        .then(() => plugin.runWithSession(kModule /* module kernel from wasm */ => {
            return new Promise(async (dispose) => {
                console.log("plugin initialized, build info:", await rendererBuildInfo());

                // todo: plugin init and setup socket at the same time
                setupSocket(createSvgDocument(kModule, previewMode));

                // never dispose session
                void (dispose);
            })
        }));
};
