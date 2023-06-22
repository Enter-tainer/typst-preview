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

export interface ElementChildren {
  tagName: string;
  getAttribute(name: string): string | null;
}

export type TargetViewInstruction<T> =
  | ["append", T]
  | ["reuse", number]
  | ["remove", number];
export type OriginViewInstruction<T> =
  | ["insert", number, T]
  | ["swap_in", number, number]
  | ["remove", number];
export type PatchPair<T> = [T /* origin */, T /* target */];

export function interpretTargetView<T extends ElementChildren, U extends T = T>(
  originChildren: T[],
  targetChildren: T[],
  tIsU = (_x: T): _x is U => true
): [TargetViewInstruction<U>[], PatchPair<U>[]] {
  const availableOwnedResource = new Map<string, [T, number[]]>();

  for (let i = 0; i < originChildren.length; i++) {
    const prevChild = originChildren[i];
    if (!tIsU(prevChild)) {
      continue;
    }
    const data_tid = prevChild.getAttribute("data-tid");
    if (data_tid) {
      if (!availableOwnedResource.has(data_tid)) {
        availableOwnedResource.set(data_tid, [prevChild, []]);
      }
      availableOwnedResource.get(data_tid)![1].push(i);
    }
  }

  const targetView: TargetViewInstruction<U>[] = [];

  const toPatch: [U, U][] = [];

  for (let i = 0; i < targetChildren.length; i++) {
    const nextChild = targetChildren[i];
    if (!tIsU(nextChild)) {
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

    const rsrc = availableOwnedResource.get(reuseTargetTid)!;
    const prevIdx = rsrc[1].pop();

    /// no available resource
    if (prevIdx === undefined) {
      /// clean one is reused directly
      if (nextDataTid === reuseTargetTid) {
        throw new Error("todo: identity clone " + reuseTargetTid);
      } else {
        targetView.push(["append", nextChild]);
      }
      continue;
    }

    /// clean one is reused directly
    if (nextDataTid === reuseTargetTid) {
      targetView.push(["reuse", prevIdx]);
      continue;
    }

    /// dirty one should be patched and reused
    toPatch.push([originChildren[prevIdx] as U, nextChild]);
    targetView.push(["reuse", prevIdx]);
  }

  for (let [_, unusedIndices] of availableOwnedResource.values()) {
    for (let unused of unusedIndices) {
      targetView.push(["remove", unused]);
    }
  }

  return [targetView, toPatch];
}

export function changeViewPerspective<
  T extends ElementChildren,
  U extends T = T
>(
  originChildren: T[],
  targetView: TargetViewInstruction<U>[],
  tIsU = (_x: T): _x is U => true
): OriginViewInstruction<U>[] {
  const originView: OriginViewInstruction<U>[] = [];

  let removeIndices: number[] = [];
  for (let inst of targetView) {
    if (inst[0] === "remove") {
      removeIndices.push(inst[1]);
    }
  }
  removeIndices = removeIndices.sort((a, b) => a - b);
  const removeShift: (number | undefined)[] = [];

  {
    let r = 0;
    for (let i = 0; i < removeIndices.length; i++) {
      while (r < removeIndices[i]) {
        removeShift.push(r - i);
        r++;
      }
      removeShift.push(undefined);
      originView.push(["remove", removeIndices[i] - i]);
      r++;
    }
    while (r <= originChildren.length) {
      removeShift.push(r - removeIndices.length);
      r++;
    }
  }
  // console.log(removeIndices, removeShift);
  const getShift = (off: number) => {
    if (off >= removeShift.length || removeShift[off] === undefined) {
      throw new Error(`invalid offset ${off} for getShift ${removeShift}`);
    }
    return removeShift[off]!;
  };

  let j = 0,
    appendOffset = 0;
  const shifted = new Set<number>();
  const appends: ["insert", number, U][] = [];
  const interpretOriginView = (off: number) => {
    // console.log(off, getShift(off));
    off = getShift(off);
    if (shifted.has(off)) {
      return;
    }
    while (j < targetView.length) {
      let done = false;
      const inst = targetView[j];
      switch (inst[0]) {
        case "append":
          appends.push(["insert", appendOffset, inst[1]]);
          appendOffset++;
          break;
        case "reuse":
          const target_off = getShift(inst[1]);
          // console.log(off, inst[1], target_off);
          if (target_off != off) {
            originView.push(["swap_in", off, target_off]);
            shifted.add(target_off);
            appendOffset++;
          } else if (target_off === off) {
            done = true;
            appendOffset++;
          }
          // else { // target_off < off
          //   // originView.push(["swap_in", off, target_off]);
          //   // shifted.add(target_off);
          //   // console.log("targetView", targetView);
          //   // console.log("originView", originView);
          //   // console.log("at", target_off, off, j);
          //   // throw new Error("reuse offset is less than prev offset");
          // }
          break;
        // case "remove":
        default:
          break;
      }

      j++;
      if (done) {
        break;
      }
    }
  };

  for (let off = 0; off < originChildren.length; off++) {
    const prevChild = originChildren[off];
    if (!tIsU(prevChild) || removeShift[off] === undefined) {
      continue;
    }
    interpretOriginView(off);
  }
  interpretOriginView(originChildren.length);

  return [...originView, ...appends];
}

function isSVGGElement(node: Element): node is SVGGElement {
  return node.tagName === "g";
}

export function patchRoot(prev: SVGGElement, next: SVGGElement) {
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

  const [targetView, toPatch] = interpretTargetView<SVGGElement>(
    prev.children as unknown as SVGGElement[],
    next.children as unknown as SVGGElement[],
    isSVGGElement
  );

  for (let [prevChild, nextChild] of toPatch) {
    patchAndSucceed(prevChild, nextChild);
  }

  console.log("interpreted target view", targetView);

  const originView = changeViewPerspective(
    prev.children as unknown as SVGGElement[],
    targetView,
    isSVGGElement
  );

  console.log("interpreted previous view", originView);
  for (const [op, off, fr] of originView) {
    switch (op) {
      case "insert":
        prev.insertBefore(fr, prev.children[off]);
        break;
      case "swap_in":
        prev.insertBefore(prev.children[fr], prev.children[off]);
        break;
      case "remove":
        prev.children[off].remove();
        break;
      default:
        throw new Error("unknown op " + op);
    }
  }
}
