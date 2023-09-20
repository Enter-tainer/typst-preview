/// The ElementChildren represents an object of a list of nodes.
export interface ElementChildren {
  tagName: string;
  getAttribute(name: string): string | null;
  cloneNode(deep: boolean): unknown;
}

/// Semantic attributes attached to SVG elements.
const enum TypstSvgAttrs {
  /// The data-tid attribute is used to identify the element.
  /// It is used to compare two elements.
  ///
  /// At most time, the data-tid is exactly their content hash.
  /// A disambiguation suffix is added when the content hash is not unique.
  Tid = "data-tid",

  /// The data-reuse attribute is used to this element is reused from specified element.
  /// The attribute content is the data-tid of the element.
  ReuseFrom = "data-reuse-from",
}

/// Predicate that a xml element is a `<g>` element.
function isGElem(node: Element): node is SVGGElement {
  return node.tagName === "g";
}

/// Compare two elements by their data-tid attribute.
function equalElem(prev: SVGGElement, next: SVGGElement) {
  const prevDataTid = prev.getAttribute(TypstSvgAttrs.Tid);
  const nextDataTid = next.getAttribute(TypstSvgAttrs.Tid);
  return prevDataTid && prevDataTid === nextDataTid;
}

/// Begin of View Interpretation
///
/// ### View
///
/// The view is defined as a structure to describe the state of a sequence,
/// While the view instructions describe the change of view.
///   That is, Given V, V', V becomes V' after applying view instructions in order.
/// We call V' the target view, and V' the origin view.
///
/// ### View Instruction
///
/// Introduced the target/origin view, there are two type of instructions to note:

/// The target view instructions are generated by
///   comparing with the target view and origin view.
/// The instruction sequence specify how we can generate a view with conditions:
/// + It is generated from a empty sequence.
/// + It can utilize a set of elements as resources.
///
/// Example1: resource:[] -> <append t1> -> [t1]
/// Example2: resource:[o1] -> <reuse o1> -> [o1]
/// Example3: resource:[o1] -> <reuse o1> <reuse o1> -> [o1, o1]
/// Example4: resource:[o1, o2] -> <reuse o1> <append t1> -> [o1, t1]
///
/// To remove unused resources, An extra remove inst can remove a specify element
///
/// Example5: resource:[o1, o2] -> <reuse o1> <append t1> <remove o2> -> [o1, t1] and remove o2
export type TargetViewInstruction<T> =
  | ["append", T]
  | ["reuse", number]
  | ["remove", number];

/// The recursive patch operation must be applied to this two element.
export type PatchPair<T> = [T /* origin */, T /* target */];

/// Interpreted result for transforming origin sequence to target sequence.
export type ViewTransform<U> = [TargetViewInstruction<U>[], PatchPair<U>[]];

/// Interpret the transform between origin sequence and target sequence.
export function interpretTargetView<T extends ElementChildren, U extends T = T>(
  originChildren: T[],
  targetChildren: T[],
  tIsU = (_x: T): _x is U => true
): ViewTransform<U> {
  const availableOwnedResource = new Map<string, [T, number[]]>();
  const targetView: TargetViewInstruction<U>[] = [];

  for (let i = 0; i < originChildren.length; i++) {
    const prevChild = originChildren[i];
    if (!tIsU(prevChild)) {
      continue;
    }

    const data_tid = prevChild.getAttribute(TypstSvgAttrs.Tid);
    if (!data_tid) {
      targetView.push(["remove", i]);
      continue;
    }

    if (!availableOwnedResource.has(data_tid)) {
      availableOwnedResource.set(data_tid, [prevChild, []]);
    }
    availableOwnedResource.get(data_tid)![1].push(i);
  }

  const toPatch: [U, U][] = [];

  for (let i = 0; i < targetChildren.length; i++) {
    const nextChild = targetChildren[i];
    if (!tIsU(nextChild)) {
      continue;
    }

    const nextDataTid = nextChild.getAttribute(TypstSvgAttrs.Tid);
    if (!nextDataTid) {
      throw new Error("not data tid for reusing g element for " + nextDataTid);
    }

    const reuseTargetTid = nextChild.getAttribute(TypstSvgAttrs.ReuseFrom);
    if (!reuseTargetTid) {
      targetView.push(["append", nextChild]);
      continue;
    }
    if (!availableOwnedResource.has(reuseTargetTid)) {
      throw new Error("no available resource for reuse " + reuseTargetTid);
    }

    const rsrc = availableOwnedResource.get(reuseTargetTid)!;
    const prevIdx = rsrc[1].shift();

    /// no available resource
    if (prevIdx === undefined) {
      /// clean one is reused directly
      if (nextDataTid === reuseTargetTid) {
        const clonedNode = rsrc[0].cloneNode(true) as U;
        toPatch.push([clonedNode, nextChild]);
        targetView.push(["append", clonedNode]);
      } else {
        targetView.push(["append", nextChild]);
      }
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

/// The origin view instructions are semantic-preserved to the target ones.
/// The major differences between the origin ones and target ones are that:
/// + It can be easily applied to a DOM sequence
/// + It has better performance.
///
/// Example1: dom:[o0, o1] -> <insert at 1, t1> -> [o0, t1, o1]
/// Example2: dom:[o0, o1, o2, o3] -> <swap_in at 2, 0> -> [o1, o0, o2, o3]
/// Example3: dom:[o0, o1, o2, o3, o4] -> <swap_in at 3, 0> -> [o1, o2, o0, o3]
/// Example4: dom:[o0, o1, o2] -> <remove at 1> -> [o0, o2]
export type OriginViewInstruction<T> =
  | ["insert", number, T]
  | ["swap_in", number, number]
  | ["remove", number];

/// Change a sequence of target view instructions to the origin ones.
/// Currently, it applies a greedy strategy.
/// + First, it applies all remove instructions.
/// + Then, it applies the swap ones.
/// + Finally, it inserts the extra elements.
///
/// Some better strategy would help and be implemented in future.
export function changeViewPerspective<
  T extends ElementChildren,
  U extends T = T
>(
  originChildren: T[],
  targetView: TargetViewInstruction<U>[],
  tIsU = (_x: T): _x is U => true
): OriginViewInstruction<U>[] {
  const originView: OriginViewInstruction<U>[] = [];

  /// see remove instructions
  let removeIndices: number[] = [];
  for (let inst of targetView) {
    if (inst[0] === "remove") {
      removeIndices.push(inst[1]);
    }
  }
  removeIndices = removeIndices.sort((a, b) => a - b);
  const removeShift: (number | undefined)[] = [];

  /// apply remove instructions and get effect
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
  /// get shift considering remove effects
  const getShift = (off: number) => {
    if (off >= removeShift.length || removeShift[off] === undefined) {
      throw new Error(`invalid offset ${off} for getShift ${removeShift}`);
    }
    return removeShift[off]!;
  };

  /// variables used by `interpretOriginView`
  /// scanning the target view
  let targetViewCursor = 0;
  /// the append effect
  let appendOffset = 0;
  /// converted append instructions.
  const swapIns: number[] = [];
  /// converted append instructions.
  const inserts: ["insert", number, U][] = [];

  /// apply append and reuse instructions till the offset of origin sequence.
  const interpretOriginView = (off: number) => {
    // console.log(off, getShift(off));
    off = getShift(off);
    while (targetViewCursor < targetView.length) {
      let done = false;
      const inst = targetView[targetViewCursor];
      switch (inst[0]) {
        case "append":
          inserts.push(["insert", appendOffset, inst[1]]);
          appendOffset++;
          break;
        case "reuse":
          const target_off = getShift(inst[1]);
          swapIns.push(target_off);
          appendOffset++;
          break;
        // case "remove":
        default:
          break;
      }

      targetViewCursor++;
      if (done) {
        break;
      }
    }
  };

  /// scanning the origin view
  for (let off = 0; off < originChildren.length; off++) {
    const prevChild = originChildren[off];

    if (removeShift[off] === undefined) {
      continue;
    }

    // keep position of unpredictable elements
    if (!tIsU(prevChild)) {
      const target_off = getShift(off);
      swapIns.push(target_off);
      appendOffset++;
      continue;
    }

    interpretOriginView(off);
  }
  interpretOriginView(originChildren.length);

  const simulated: number[] = [];
  for (let i = 0; i < swapIns.length; i++) {
    simulated.push(i);
  }
  for (let i = 0; i < swapIns.length; i++) {
    const off = swapIns[i];
    for (let j = 0; j < simulated.length; j++) {
      if (simulated[j] === off) {
        // console.log("swap_in", j, i, simulated);
        simulated.splice(j, 1);
        if (i <= j) {
          simulated.splice(i, 0, off);
        } else {
          simulated.splice(i + 1, 0, off);
        }
        if (j !== i) {
          originView.push(["swap_in", i, j]);
          // console.log("swap_in then", j, i, simulated);
        }
        break;
      }
    }
  }

  return [...originView, ...inserts];
}

function runOriginViewInstructions(
  prev: Element,
  originView: OriginViewInstruction<Node>[]
) {
  // console.log("interpreted origin view", originView);
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

/// End of View Interpretation
/// Begin of Recursive Svg Patch

/// Patch the `prev <svg>` in the DOM according to `next <svg>` from the backend.
export function patchRoot(prev: SVGElement, next: SVGElement) {
  /// Patch attributes
  patchAttributes(prev, next);
  /// Patch global svg resources
  patchSvgHeader(prev, next);

  /// Hard replace elements that is not a `<g>` element.
  const frozen = preReplaceNonSVGElements(prev, next, 3);
  /// Patch `<g>` children, call `reuseOrPatchElem` to patch.
  patchChildren(prev, next);
  postReplaceNonSVGElements(prev, 3, frozen);
  return;

  function patchSvgHeader(prev: SVGElement, next: SVGElement) {
    for (let i = 0; i < 3; i++) {
      const prevChild = prev.children[i];
      const nextChild = next.children[i];
      // console.log("prev", prevChild);
      // console.log("next", nextChild);
      if (prevChild.tagName === "defs") {
        if (prevChild.getAttribute("class") === "glyph") {
          // console.log("append glyphs:", nextChild.children, "to", prevChild);
          prevChild.append(...nextChild.children);
        } else if (prevChild.getAttribute("class") === "clip-path") {
          // console.log("clip path: replace");
          // todo: gc
          prevChild.append(...nextChild.children);
        }
      } else if (
        prevChild.tagName === "style" &&
        nextChild.getAttribute("data-reuse") !== "1"
      ) {
        // console.log("replace extra style", prevChild, nextChild);

        // todo: gc
        if (nextChild.textContent) {
          // todo: looks slow
          // https://stackoverflow.com/questions/3326494/parsing-css-in-javascript-jquery
          var doc = document.implementation.createHTMLDocument(""),
            styleElement = document.createElement("style");

          styleElement.textContent = nextChild.textContent;
          // the style will only be parsed once it is added to a document
          doc.body.appendChild(styleElement);

          const currentSvgSheet = (prevChild as HTMLStyleElement).sheet!;
          const rulesToInsert = styleElement.sheet?.cssRules || [];

          // console.log("rules to insert", currentSvgSheet, rulesToInsert);
          for (const rule of rulesToInsert) {
            currentSvgSheet.insertRule(rule.cssText);
          }
        }
      }
    }
  }
}

/// apply attribute patches to the `prev <svg or g>` element
function patchAttributes(prev: Element, next: Element) {
  const prevAttrs = prev.attributes;
  const nextAttrs = next.attributes;
  if (prevAttrs.length === nextAttrs.length) {
    let same = true;
    for (let i = 0; i < prevAttrs.length; i++) {
      const prevAttr = prevAttrs[i];
      const nextAttr = nextAttrs.getNamedItem(prevAttr.name);
      if (nextAttr === null || prevAttr.value !== nextAttr.value) {
        same = false;
        break;
      }
    }

    if (same) {
      // console.log("same attributes, skip");
      return;
    }
  }
  // console.log("different attributes, replace");

  const removedAttrs = [];

  for (let i = 0; i < prevAttrs.length; i++) {
    removedAttrs.push(prevAttrs[i].name);
  }

  for (const attr of removedAttrs) {
    prev.removeAttribute(attr);
  }

  for (let i = 0; i < nextAttrs.length; i++) {
    prev.setAttribute(nextAttrs[i].name, nextAttrs[i].value);
  }
}

/// apply patches to the children sequence of `prev <svg or g>` in the DOM
function patchChildren(prev: Element, next: Element) {
  const [targetView, toPatch] = interpretTargetView<SVGGElement>(
    prev.children as unknown as SVGGElement[],
    next.children as unknown as SVGGElement[],
    isGElem
  );

  for (let [prevChild, nextChild] of toPatch) {
    reuseOrPatchElem(prevChild, nextChild);
  }

  // console.log("interpreted target view", targetView);

  const originView = changeViewPerspective(
    prev.children as unknown as SVGGElement[],
    targetView,
    isGElem
  );

  runOriginViewInstructions(prev, originView);
}

/// Replace the `prev` element with `next` element.
/// Return true if the `prev` element is reused.
/// Return false if the `prev` element is replaced.
function reuseOrPatchElem(prev: SVGGElement, next: SVGGElement) {
  const canReuse = equalElem(prev, next);

  /// Even if the element is reused, we still need to replace its attributes.
  next.removeAttribute(TypstSvgAttrs.ReuseFrom);
  patchAttributes(prev, next);

  if (canReuse) {
    return true /* reused */;
  }

  /// Hard replace elements that is not a `<g>` element.
  const frozen = preReplaceNonSVGElements(prev, next, 0);
  /// Patch `<g>` children, will call `reuseOrPatchElem` again.
  patchChildren(prev, next);
  postReplaceNonSVGElements(prev, 0, frozen);
  return false /* reused */;
}

interface FrozenReplacement {
  inserts: Element[][];
}

function preReplaceNonSVGElements(prev: Element, next: Element, since: number): FrozenReplacement {
  const removedIndecies = [];
  const frozenReplacement: FrozenReplacement = {
    inserts: [],
  };
  for (let i = since; i < prev.children.length; i++) {
    const prevChild = prev.children[i];
    if (!isGElem(prevChild)) {
      removedIndecies.push(i);
    }
  }

  for (const index of removedIndecies.reverse()) {
    prev.children[index].remove();
  }

  let elements: Element[] = [];
  for (let i = since; i < next.children.length; i++) {
    const nextChild = next.children[i];
    if (!isGElem(nextChild)) {
      elements.push(nextChild);
    } else {
      frozenReplacement.inserts.push(elements);
      elements = [];
    }
  }

  frozenReplacement.inserts.push(elements);

  return frozenReplacement;
}

function postReplaceNonSVGElements(prev: Element, since: number, frozen: FrozenReplacement) {

  /// Retrive the `<g>` elements from the `prev` element.
  const gElements = Array.from(prev.children).slice(since).filter(isGElem);
  if (gElements.length + 1 !== frozen.inserts.length) {
    throw new Error("invalid frozen replacement");
  }

  /// Insert the separated elements to the `prev` element.
  for (let i = 0; i < gElements.length; i++) {
    const prevChild = gElements[i];
    for (const elem of frozen.inserts[i]) {
      prev.insertBefore(elem, prevChild);
    }
  }

  /// Append the last elements to the `prev` element.
  for (const elem of frozen.inserts[gElements.length]) {
    prev.append(elem);
  }
}

/// End of Recursive Svg Patch
