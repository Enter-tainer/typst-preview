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
  | ["swap_in", number, number];
export type PatchPair<T> = [T /* origin */, T /* target */];

export function interpretTargetView<T extends ElementChildren, U extends T = T>(
  originChildren: T[],
  targetChildren: T[],
  filter = (_x: T): _x is U => true
): [TargetViewInstruction<U>[], PatchPair<U>[]] {
  const availableOwnedResource = new Map<string, [T, number[]]>();

  for (let i = 0; i < originChildren.length; i++) {
    const prevChild = originChildren[i];
    if (!filter(prevChild)) {
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
    if (!filter(nextChild)) {
      continue;
    }

    const nextDataTid = nextChild.getAttribute("data-tid");
    if (!nextDataTid) {
      throw new Error("not data tid for reusing g element for " + nextDataTid);
    }

    const reuseTargetTid = nextChild.getAttribute("data-reuse-from");
    console.log("reuseTargetTid", reuseTargetTid);
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
      targetView.push(["append", nextChild]);
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

export function changeViewPerspective<T extends ElementChildren>(
  originChildren: T[],
  targetView: TargetViewInstruction<T>[]
): OriginViewInstruction<T>[] {
  const originView: OriginViewInstruction<T>[] = [];
  let j = 0;
  for (let fg = 0; fg < originChildren.length; fg++) {
    const prevChild = originChildren[fg];
    if (prevChild.tagName !== "g") {
      continue;
    }
    for (let off = fg; off < originChildren.length; off++) {
      const prevChild = originChildren[off];
      if (prevChild.tagName !== "g") {
        break;
      }
      while (j < targetView.length) {
        let done = false;
        const inst = targetView[j];
        switch (inst[0]) {
          case "append":
            originView.push(["insert", off, inst[1]]);
            done = true;
            break;
          case "reuse":
            const target_off = inst[1];
            if (target_off > off) {
              originView.push(["swap_in", off, target_off]);
            } else if (target_off === off) {
              done = true;
            } else {
              console.log(targetView, originView, off, j);
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

  return originView;
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
    targetView
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
      default:
        throw new Error("unknown op " + op);
    }
  }
}
