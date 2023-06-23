import "./typst.css";
import { patchRoot } from "./svg-patch";

window.onload = function () {
  const imageContainer = document.getElementById("imageContainer");
  let currentScale = 1; // variable for storing scaling factor
  let imageContainerWidth = imageContainer.offsetWidth;

  // drag (panal resizing) -> rescaling
  window.onresize = () => {
    const newImageContainerWidth = imageContainer.offsetWidth;
    currentScale =
      currentScale * (newImageContainerWidth / imageContainerWidth);
    imageContainerWidth = newImageContainerWidth;
    imageContainer.style.transformOrigin = "0px 0px";
    imageContainer.style.transform = `scale(${currentScale * 2})`;
  };

  // Ctrl+scroll rescaling
  // will disable auto resizing
  // fixed factors, same as pdf.js
  const factors = [
    0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9,
    2.1, 2.4, 2.7, 3, 3.3, 3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
  ];
  imageContainer.addEventListener("wheel", function (event) {
    if (event.ctrlKey) {
      event.preventDefault();

      if (window.onresize !== null) {
        // is auto resizing
        window.onresize = null;
      }

      // Get wheel scroll direction and calculate new scale
      if (event.deltaY < 0) {
        // enlarge
        if (currentScale >= factors.at(-1)) {
          // already large than max factor
          return;
        } else {
          currentScale = factors.filter((x) => x > currentScale).at(0);
        }
      } else if (event.deltaY > 0) {
        // reduce
        if (currentScale <= factors.at(0)) {
          return;
        } else {
          currentScale = factors.filter((x) => x < currentScale).at(-1);
        }
      } else {
        // no y-axis scroll
        return;
      }

      // Apply new scale
      imageContainer.style.transformOrigin = "0 0";
      imageContainer.style.transform = `scale(${currentScale * 2})`;
    }
  });

  let isFirstScale = true;
  let processStart;

  let socket;
  let socketOpen = false;

  function setupSocket() {
    socket = new WebSocket("ws://127.0.0.1:23625");
    // socket.binaryType = "arraybuffer";
    socket.addEventListener("open", () => {
      socketOpen = true;
      console.log("WebSocket connection opened");
      socket.send("current");
    });

    socket.addEventListener("close", () => {
      socketOpen = false;
      setTimeout(setupSocket, 1000);
    });

    // 当收到WebSocket数据时
    socket.addEventListener("message", (event) => {
      const data = event.data;
      const message_idx = data.indexOf(",");
      const message = [data.slice(0, message_idx), data.slice(message_idx + 1)];
      console.log(message);

      let t0 = performance.now();
      let t1 = undefined;
      let t2 = undefined;
      switch (message[0]) {
        case "new":
          imageContainer.innerHTML = message[1];
          t1 = t2 = performance.now();
          break;
        case "diff-v0":
          const elem = document.createElement("div");
          elem.innerHTML = message[1];
          const svgElement = elem.firstElementChild;
          t1 = performance.now();
          patchRoot(imageContainer.firstElementChild, svgElement);
          t2 = performance.now();
          break;
        default:
          console.log("data", data);
          break;
      }

      console.log(
        `parse ${(t1 - t0).toFixed(2)} ms, replace ${(t2 - t1).toFixed(
          2
        )} ms, total ${(t2 - t0).toFixed(2)} ms`
      );
      const docRoot = imageContainer.firstElementChild;
      if (docRoot) {
        window.initTypstSvg(docRoot);
      }
    });

    // 当WebSocket连接关闭时
    socket.addEventListener("close", () => {
      console.log("WebSocket connection closed");
    });

    // 当发生错误时
    socket.addEventListener("error", (error) => {
      console.error("WebSocket Error: ", error);
    });
  }
  setupSocket();
};
