import "./typst.css";
import { SvgDocument } from "./svg-doc";
import { createTypstSvgRenderer } from "@myriaddreamin/typst.ts";
import renderModule from "@myriaddreamin/typst-ts-renderer/typst_ts_renderer_bg.wasm?url";

const enc = new TextEncoder("utf-8");
const dec = new TextDecoder("utf-8");
const NOT_AVAIABLE = "current not avalible";
const COMMA = enc.encode(",");

window.onload = function () {
  const hookedElem = document.getElementById("imageContainer");
  const svgDoc = new SvgDocument(hookedElem);

  // drag (panal resizing) -> rescaling
  window.onresize = () => svgDoc.rescale();

  let socketOpen = false;
  let plugin = createTypstSvgRenderer();
  let currentModule = undefined;
  console.log(plugin);

  function setupSocket() {
    window.typstWebsocket = new WebSocket("ws://127.0.0.1:23625");
    // socket.binaryType = "arraybuffer";
    window.typstWebsocket.addEventListener("open", () => {
      socketOpen = true;
      console.log("WebSocket connection opened");
      currentModule.reset();
      window.typstWebsocket.send("current");
    });

    window.typstWebsocket.addEventListener("close", () => {
      socketOpen = false;
      setTimeout(setupSocket, 1000);
    });

    // 当收到WebSocket数据时
    window.typstWebsocket.addEventListener("message", (event) => {
      const data = event.data;
      if (!(data instanceof Blob)) {
        if (data === NOT_AVAIABLE) {
          return;
        }

        console.error("WebSocket data is not a Blob", data);
        return;
      }

      data.arrayBuffer().then((buffer) => {
        const messageData = new Uint8Array(buffer);
        console.log(messageData);

        const message_idx = messageData.indexOf(COMMA[0]);
        const message = [
          dec.decode(messageData.slice(0, message_idx).buffer),
          messageData.slice(message_idx + 1),
        ];
        console.log(message);

        if (message[0] === "jump") {
          const [page, x, y] = dec
            .decode(message[1].buffer)
            .split(" ")
            .map(Number);
          const rootElem =
            document.getElementById("imageContainer")?.firstElementChild;
          if (rootElem) {
            window.handleTypstLocation(rootElem, page, x, y);
          }
          return;
        }

        svgDoc.addChangement(message);
      });
    });

    // 当WebSocket连接关闭时
    window.typstWebsocket.addEventListener("close", () => {
      console.log("WebSocket connection closed");
    });

    // 当发生错误时
    window.typstWebsocket.addEventListener("error", (error) => {
      console.error("WebSocket Error: ", error);
    });
  }

  plugin
    .init({
      getModule: () => renderModule,
    })
    .then(() => {
      return plugin.createModule();
    })
    .then((m) => {
      svgDoc.setModule((currentModule = m));
    })
    .then(async () => {
      // todo: plugin init and setup socket at the same time
      setupSocket();
      console.log(plugin);
    });
};
