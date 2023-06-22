function equal(prev: SVGGElement, next: SVGGElement) {
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

function replaceChildrenFineGranuality(prev: SVGGElement, next: SVGGElement) {
  for (let i = 0; i < prev.children.length; i++) {
    const prevChild = prev.children[i];
    const nextChild = next.children[i];
    console.log("replacing", prevChild, nextChild);
  }

  return false;
}

function replaceChildren(prev: SVGGElement, next: SVGGElement) {
  if (!replaceChildrenFineGranuality(prev, next)) {
    console.log("hard replace", prev, next);
    prev.replaceWith(next);
  }
}

function patchAndSucceed(prev: SVGGElement, next: SVGGElement) {
  console.log("patchAndSucceed", prev, next);
  if (equal(prev, next)) {
    return true;
  } else {
    next.removeAttribute("data-reuse-from");
    replaceChildren(prev, next);
    return false;
  }
}

export function patchRoot(prev: SVGGElement, next: SVGGElement) {
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

  const toPatch: [SVGGElement, SVGGElement][] = [];

  for (let i = 0; i < next.children.length; i++) {
    const nextChild = next.children[i];
    if (nextChild.tagName !== "g") {
      continue;
    }

    const nextDataTid = nextChild.getAttribute("data-tid");
    if (!nextDataTid) {
      throw new Error("not data tid for reusing g element for " + nextDataTid);
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
    toPatch.push([
      prev.children[prevIdx] as SVGGElement,
      nextChild as SVGGElement,
    ]);
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
        switch (targetView[j][0]) {
          case "append":
            prevView.push(["insert", off, targetView[j][1]]);
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
