import "./typst.css";
import { SvgDocument } from "./svg-doc";

window.onload = function () {
  const hookedElem = document.getElementById("imageContainer");
  const svgDoc = new SvgDocument(hookedElem);

  // drag (panal resizing) -> rescaling
  window.onresize = () => svgDoc.rescale();

  let socketOpen = false;

  function setupSocket() {
    window.typstWebsocket = new WebSocket("ws://127.0.0.1:23625");
    // socket.binaryType = "arraybuffer";
    window.typstWebsocket.addEventListener("open", () => {
      socketOpen = true;
      console.log("WebSocket connection opened");
      window.typstWebsocket.send("current");
    });

    window.typstWebsocket.addEventListener("close", () => {
      socketOpen = false;
      setTimeout(setupSocket, 1000);
    });

    // 当收到WebSocket数据时
    window.typstWebsocket.addEventListener("message", (event) => {
      const data = event.data;
      if ("current not avalible" === data) {
        return;
      }

      const message_idx = data.indexOf(",");
      const message = [data.slice(0, message_idx), data.slice(message_idx + 1)];
      console.log(message);

      svgDoc.addChangement(message);
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
  setupSocket();
};
