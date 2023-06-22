import "./typst.css";

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

  function equal(prev, next) {
    if (prev.tagName === "g") {
      // compareAndReplaceRoot(prev, next);
      if (next.tagName === "g") {
        // data tid
        const prevDataTid = prev.getAttribute("data-tid");
        const nextDataTid = next.getAttribute("data-tid");
        if (prevDataTid && nextDataTid && prevDataTid === nextDataTid) {
          return true;
        }
      }
    }

    return false;
  }

  function replaceChildrenFineGranuality(prev, next) {
    for (let i = 0; i < prev.children.length; i++) {
      const prevChild = prev.children[i];
      const nextChild = next.children[i];
      console.log("replacing", prevChild, nextChild);
    }

    return false;
  }

  function replaceChildren(prev, next) {
    if (!replaceChildrenFineGranuality(prev, next)) {
      console.log("hard replace", prev, next);
      prev.replaceWith(next);
    }
  }

  function patchAndSucceed(prev, next) {
    console.log("patchAndSucceed", prev, next);
    if (equal(prev, next)) {
      return true;
    } else {
      next.removeAttribute("data-reuse-from");
      replaceChildren(prev, next);
      return false;
    }
  }

  function patchRoot(prev, next) {
    const availableOwnedResource = new Map();

    for (let i = 0; i < 3; i++) {
      const prevChild = prev.children[i];
      const nextChild = next.children[i];
      console.log("prev", prevChild);
      console.log("next", nextChild);
      if (prevChild.tagName === "defs") {
        if (prevChild.getAttribute("id") === "glyph") {
          console.log("append glyphs:", nextChild.children, "to", prevChild);
          prevChild.append(...nextChild.children);
        } else if (prevChild.getAttribute("id") === "clip-path") {
          console.log("clip path: replace");
          prevChild.replaceChildren(...nextChild.children);
        }
      } else if (
        prevChild.tagName === "style" &&
        nextChild.getAttribute("data-reuse") !== "1"
      ) {
        console.log("replace extra style");
        prevChild.replaceChildren(...nextChild.children);
      }
    }

    for (let i = 0; i < prev.children.length; i++) {
      const prevChild = prev.children[i];
      if (prevChild.tagName !== "g") {
        continue;
      }
      const data_tid = prevChild.getAttribute("data-tid");
      if (data_tid) {
        if (!availableOwnedResource.has(data_tid)) {
          availableOwnedResource.set(data_tid, [prevChild, []]);
        }
        availableOwnedResource.get(data_tid)[1].push(i);
      }
    }

    // console.log(availableOwnedResource);

    const targetView = [];

    const toPatch = [];

    for (let i = 0; i < next.children.length; i++) {
      const nextChild = next.children[i];
      if (nextChild.tagName !== "g") {
        continue;
      }

      const nextDataTid = nextChild.getAttribute("data-tid");
      if (!nextDataTid) {
        throw new Error(
          "not data tid for reusing g element for " + reuseTargetTid
        );
      }

      const reuseTargetTid = nextChild.getAttribute("data-reuse-from");
      if (!reuseTargetTid) {
        targetView.push(["append", nextChild]);
        continue;
      }
      if (!availableOwnedResource.has(reuseTargetTid)) {
        throw new Error("no available resource for reuse " + reuseTargetTid);
      }

      const rsrc = availableOwnedResource.get(reuseTargetTid);
      const prevIdx = rsrc[1].pop();

      /// no available resource
      if (prevIdx === undefined) {
        targetView.push(["append", nextChild]);
        continue;
      }

      /// clean one is reused directly
      if (nextDataTid === reuseTargetTid) {
        targetView.push(["reuse", prevIdx]);
        continue;
      }

      /// dirty one should be patched and reused
      toPatch.push([prev.children[prevIdx], nextChild]);
      targetView.push(["reuse", prevIdx]);
    }

    for (let [prevChild, nextChild] of toPatch) {
      patchAndSucceed(prevChild, nextChild);
    }

    console.log("interpreted target view", targetView);

    const prevView = [];
    let j = 0;
    for (let fg = 0; fg < prev.children.length; fg++) {
      const prevChild = prev.children[fg];
      if (prevChild.tagName !== "g") {
        continue;
      }
      for (let off = fg; off < prev.children.length; off++) {
        const prevChild = prev.children[off];
        if (prevChild.tagName !== "g") {
          break;
        }
        while (j < targetView.length) {
          let done = false;
          switch (j) {
            case "append":
              prevView.push(["insert", off, nextChild]);
              done = true;
              break;
            case "reuse":
              const target_off = targetView[j][1];
              if (target_off > off) {
                prevView.push(["swap_in", off, target_off]);
              } else if (target_off === off) {
                done = true;
              } else {
                console.log(targetView, prevView, off, j);
                throw new Error("reuse offset is less than prev offset");
              }
              break;
          }

          j++;
          if (done) {
            break;
          }
        }
      }
      break;
    }

    console.log("interpreted previous view", prevView);
    for (const [op, off, fr] of prevView) {
      switch (op) {
        case "insert":
          prev.insertBefore(fr, prev.children[off]);
          break;
        case "swap_in":
          prev.insertBefore(prev.children[fr], prev.children[off]);
          break;
        default:
          throw new Error("unknown op " + op);
      }
    }
  }

  function setupSocket() {
    socket = new WebSocket("ws://127.0.0.1:23625");
    // socket.binaryType = "arraybuffer";
    socket.addEventListener("open", () => {
      socketOpen = true;
      console.log("WebSocket connection opened");
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
