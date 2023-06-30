import { triggerRipple } from "./svg-animation";

type SourceMappingNode =
  | ["p", number[]]
  | ["g", number[]]
  | ["u", number[]]
  | ["t", [string]]
  | ["i", [string]]
  | ["s", [string]];

type SourceMappingLocTypes = "t" | "i" | "s";
function isSourceMappingLocNode(ty: string): ty is SourceMappingLocTypes {
  return ["t", "i", "s"].includes(ty);
}

type SourceMappingRefTypes = "p" | "g" | "u";
function isSourceMappingRefNode(ty: string): ty is SourceMappingRefTypes {
  return ["p", "g", "u"].includes(ty);
}

export function parseSourceMappingNode(node: string): SourceMappingNode {
  const elements = node.split(",");
  const ty = elements[0];
  if (isSourceMappingLocNode(ty)) {
    return [ty, [elements[1]]];
  }
  if (!isSourceMappingRefNode(ty)) {
    throw new Error(`unknown type ${ty}`);
  }
  const result = elements.slice(1).map((x) => Number.parseInt(x, 16));
  return [ty, result] as SourceMappingNode;
}

// one-of following classes must be present:
// - typst-page
// - typst-group
// - typst-text
// - typst-image
// - typst-shape
function castToSourceMappingElement(
  elem: Element
): [string, Element] | undefined {
  if (elem.classList.length === 0) {
    return undefined;
  }
  for (const cls of [
    "typst-text",
    "typst-group",
    "typst-image",
    "typst-shape",
    "typst-page",
  ]) {
    if (elem.classList.contains(cls)) {
      return [cls, elem];
    }
  }
  return undefined;
}

function castToNestSourceMappingElement(
  elem: Element
): [string, Element] | undefined {
  while (elem) {
    const result = castToSourceMappingElement(elem);
    if (result) {
      return result;
    }
    let chs = elem.children;
    if (chs.length !== 1) {
      return undefined;
    }
    elem = chs[0];
  }

  return undefined;
}

function castChildrenToSourceMappingElement(
  elem: Element
): [string, Element][] {
  return Array.from(elem.children)
    .map(castToNestSourceMappingElement)
    .filter((x) => x) as [string, Element][];
}

export function initSourceMapping(
  docRoot: SVGElement,
  dataPages: SourceMappingNode[],
  dataSourceMapping: SourceMappingNode[]
) {
  // console.log(dataPages, dataSourceMapping);

  const findSourceLocation = (elem: Element) => {
    const visitChain: [string, Element][] = [];
    while (elem) {
      let srcElem = castToSourceMappingElement(elem);
      if (srcElem) {
        visitChain.push(srcElem);
      }
      if (elem === docRoot) {
        visitChain.push(["typst-root", elem]);
        break;
      }
      elem = elem.parentElement!;
    }

    // console.log(visitChain);

    if (elem !== docRoot) {
      return;
    }

    let parentElements: [string, Element][] = [];
    const root = visitChain.pop()!;
    if (root[0] !== "typst-root") {
      return;
    }
    parentElements = castChildrenToSourceMappingElement(elem);
    if (!parentElements) {
      return;
    }

    let locInfo: SourceMappingNode[] = dataPages;

    visitChain.reverse();
    for (const [ty, elem] of visitChain) {
      const childrenElements = castChildrenToSourceMappingElement(elem);

      if (locInfo.length !== parentElements.length) {
        console.error("length mismatch", locInfo, parentElements);
        break;
      }

      const idx = parentElements.findIndex((x) => x[0] === ty && x[1] === elem);
      if (idx === -1) {
        console.error("not found", ty, elem, " in ", locInfo);
        break;
      }

      const locInfoItem = locInfo[idx];

      switch (ty) {
        case "typst-page":
          if (locInfoItem[0] !== "p") {
            console.error("type mismatch", locInfo, ty, elem);
            return;
          }
          break;
        case "typst-group":
          if (locInfoItem[0] !== "g") {
            console.error("type mismatch", locInfo, ty, elem);
            return;
          }
          break;
        case "typst-text":
          if (locInfoItem[0] !== "t") {
            console.error("type mismatch", locInfo, ty, elem);
            return;
          }

          return locInfoItem;
        case "typst-image":
          if (locInfoItem[0] !== "i") {
            console.error("type mismatch", locInfo, ty, elem);
            return;
          }

          return locInfoItem;
        case "typst-shape":
          if (locInfoItem[0] !== "s") {
            console.error("type mismatch", locInfo, locInfoItem, ty, elem);
            return;
          }

          return locInfoItem;
        default:
          console.error("unknown type", ty, elem);
          return;
      }

      parentElements = childrenElements;
      locInfo = locInfoItem[1].map((x) => {
        if (x >= dataSourceMapping.length) {
          console.error("invalid index", x, dataSourceMapping);
          return ["u", []];
        }
        return dataSourceMapping[x];
      });

      // console.log(
      //   ty,
      //   locInfo,
      //   parentElements
      // );
    }
  };

  const prevSourceMappingHandler = (docRoot as any).sourceMappingHandler;
  if (prevSourceMappingHandler) {
    docRoot.removeEventListener("click", prevSourceMappingHandler);
  }
  const sourceMappingHandler = ((docRoot as any).sourceMappingHandler = (
    event: MouseEvent
  ) => {
    let elem = event.target! as Element;

    const sourceLoc = findSourceLocation(elem);
    if (!sourceLoc) {
      return;
    }
    console.log("source location", sourceLoc);

    const triggerWindow = document.body || document.firstElementChild;
    const basePos = triggerWindow.getBoundingClientRect();

    // const vw = window.innerWidth || 0;
    const left = event.clientX - basePos.left;
    const top = event.clientY - basePos.top;

    triggerRipple(
      triggerWindow,
      left,
      top,
      "typst-debug-react-ripple",
      "typst-debug-react-ripple-effect .4s linear"
    );

    window.typstWebsocket.send(`srclocation ${sourceLoc[1][0]}`);
    return;
  });

  docRoot.addEventListener("click", sourceMappingHandler);
}
