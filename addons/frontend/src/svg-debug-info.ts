import { triggerRipple } from "./svg-animation";

const enum SourceMappingType {
  Text = 0,
  Group = 1,
  Image = 2,
  Shape = 3,
  Page = 4,
}

// one-of following classes must be present:
// - typst-page
// - typst-group
// - typst-text
// - typst-image
// - typst-shape
const CssClassToType = [
  ["typst-text", SourceMappingType.Text],
  ["typst-group", SourceMappingType.Group],
  ["typst-image", SourceMappingType.Image],
  ["typst-shape", SourceMappingType.Shape],
  ["typst-page", SourceMappingType.Page],
] as const;

function castToSourceMappingElement(
  elem: Element
): [SourceMappingType, Element] | undefined {
  if (elem.classList.length === 0) {
    return undefined;
  }
  for (const [cls, ty] of CssClassToType) {
    if (elem.classList.contains(cls)) {
      return [ty, elem];
    }
  }
  return undefined;
}

function castToNestSourceMappingElement(
  elem: Element
): [SourceMappingType, Element] | undefined {
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
): [SourceMappingType, Element][] {
  return Array.from(elem.children)
    .map(castToNestSourceMappingElement)
    .filter((x) => x) as [SourceMappingType, Element][];
}

export function removeSourceMappingHandler(docRoot: HTMLElement) {
  const prevSourceMappingHandler = (docRoot as any).sourceMappingHandler;
  if (prevSourceMappingHandler) {
    docRoot.removeEventListener("click", prevSourceMappingHandler);
    // console.log("remove removeSourceMappingHandler");
  }
}

export function installEditorJumpToHandler(_svgDoc: any, docRoot: HTMLElement) {
  void castChildrenToSourceMappingElement;

  const findSourceLocation = (elem: Element) => {
    const visitChain: [SourceMappingType, Element][] = [];
    while (elem) {
      let srcElem = castToSourceMappingElement(elem);
      if (srcElem) {
        visitChain.push(srcElem);
      }
      if (elem === docRoot) {
        break;
      }
      elem = elem.parentElement!;
    }

    console.log(visitChain);
    return undefined;
  };

  removeSourceMappingHandler(docRoot);
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
