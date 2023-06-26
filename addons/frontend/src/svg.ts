// debounce https://stackoverflow.com/questions/23181243/throttling-a-mousemove-event-to-fire-no-more-than-5-times-a-second
// ignore fast events, good for capturing double click
// @param (callback): function to be run when done
// @param (delay): integer in milliseconds
// @param (id): string value of a unique event id
// @doc (event.timeStamp): http://api.jquery.com/event.timeStamp/
// @bug (event.currentTime): https://bugzilla.mozilla.org/show_bug.cgi?id=238041
let ignoredEvent = (function () {
  let last: Record<string, any> = {},
    diff: number,
    time: number;

  return function (callback: () => undefined, delay: number, id: string) {
    time = new Date().getTime();
    id = id || "ignored event";
    diff = last[id] ? time - last[id] : time;

    if (diff > delay) {
      last[id] = time;
      callback();
    }
  };
})();

var overLapping = function (a: Element, b: Element) {
  var aRect = a.getBoundingClientRect();
  var bRect = b.getBoundingClientRect();

  return (
    !(
      aRect.right < bRect.left ||
      aRect.left > bRect.right ||
      aRect.bottom < bRect.top ||
      aRect.top > bRect.bottom
    ) &&
    /// determine overlapping by area
    (Math.abs(aRect.left - bRect.left) + Math.abs(aRect.right - bRect.right)) /
      Math.max(aRect.width, bRect.width) <
      0.5 &&
    (Math.abs(aRect.bottom - bRect.bottom) + Math.abs(aRect.top - bRect.top)) /
      Math.max(aRect.height, bRect.height) <
      0.5
  );
};

var searchIntersections = function (root: Element) {
  let parent = undefined,
    current = root;
  while (current) {
    if (current.classList.contains("typst-group")) {
      parent = current;
      break;
    }
    current = current.parentElement!;
  }
  if (!parent) {
    console.log("no group found");
    return;
  }
  const group = parent;
  const children = group.children;
  const childCount = children.length;

  const res = [];

  for (let i = 0; i < childCount; i++) {
    const child = children[i];
    if (!overLapping(child, root)) {
      continue;
    }
    res.push(child);
  }

  return res;
};

var getRelatedElements = function (event: any) {
  let relatedElements = event.target.relatedElements;
  if (relatedElements === undefined || relatedElements === null) {
    relatedElements = event.target.relatedElements = searchIntersections(
      event.target
    );
  }
  return relatedElements;
};

var linkmove = function (event: MouseEvent) {
  ignoredEvent(
    function () {
      const elements = getRelatedElements(event);
      if (elements === undefined || elements === null) {
        return;
      }
      for (var i = 0; i < elements.length; i++) {
        var elem = elements[i];
        if (elem.classList.contains("hover")) {
          continue;
        }
        elem.classList.add("hover");
      }
    },
    200,
    "mouse-move"
  );
};

var linkleave = function (event: MouseEvent) {
  const elements = getRelatedElements(event);
  if (elements === undefined || elements === null) {
    return;
  }
  for (var i = 0; i < elements.length; i++) {
    var elem = elements[i];
    if (!elem.classList.contains("hover")) {
      continue;
    }
    elem.classList.remove("hover");
  }
};

function findAncestor(el: Element, cls: string) {
  while ((el = el.parentElement!) && !el.classList.contains(cls));
  return el;
}

function parseSourceMappingNode(node: string): [string, number[]] {
  const elements = node.split(",");
  const ty = elements[0];
  const result = elements.slice(1).map((x) => Number.parseInt(x, 16));
  return [ty, result];
}

// one-of following classes must be present:
// - typst-page
// - typst-group
// - typst-text
function castToSourceMappingElement(
  elem: Element
): [string, Element] | undefined {
  if (elem.classList.length === 0) {
    return undefined;
  }
  for (const cls of ["typst-text", "typst-group", "typst-page"]) {
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

window.initTypstSvg = function (
  docRoot: SVGElement,
  srcMapping?: HTMLDivElement
) {
  var elements = docRoot.getElementsByClassName("pseudo-link");

  for (var i = 0; i < elements.length; i++) {
    let elem = elements[i] as SVGAElement;
    elem.addEventListener("mousemove", linkmove);
    elem.addEventListener("mouseleave", linkleave);
  }

  if (false) {
    setTimeout(() => {
      layoutText(docRoot);
    }, 0);
  }

  if (srcMapping) {
    setTimeout(() => {
      initSourceMapping(docRoot, srcMapping);
    }, 0);
  }
};

const layoutText = (svg: SVGElement) => {
  const divs = svg.querySelectorAll<HTMLDivElement>(".tsel");
  const canvas = document.createElementNS(
    "http://www.w3.org/1999/xhtml",
    "canvas"
  ) as HTMLCanvasElement;
  const ctx = canvas.getContext("2d")!;

  const layoutBegin = performance.now();

  for (let d of divs) {
    if (d.getAttribute("data-typst-layout-checked")) {
      continue;
    }

    if (d.style.fontSize) {
      const foreignObj = d.parentElement!;
      const innerText = d.innerText;
      const targetWidth =
        Number.parseFloat(foreignObj.getAttribute("width") || "0") || 0;
      const currentX =
        Number.parseFloat(foreignObj.getAttribute("x") || "0") || 0;
      ctx.font = `${d.style.fontSize} sans-serif`;
      const selfWidth = ctx.measureText(innerText).width;

      const scale = targetWidth / selfWidth;

      d.style.transform = `scaleX(${scale})`;
      foreignObj.setAttribute("width", selfWidth.toString());
      foreignObj.setAttribute(
        "x",
        (currentX - (selfWidth - targetWidth) * 0.5).toString()
      );

      d.setAttribute("data-typst-layout-checked", "1");
    }
  }

  console.log(`layoutText used time ${performance.now() - layoutBegin} ms`);
};

function initSourceMapping(docRoot: SVGElement, srcMapping: HTMLDivElement) {
  const dataPages = srcMapping
    .getAttribute("data-pages")!
    .split("|")
    .map(parseSourceMappingNode);
  const dataSourceMapping = srcMapping
    .getAttribute("data-source-mapping")!
    .split("|")
    .map(parseSourceMappingNode);

  console.log(dataPages, dataSourceMapping);

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

    console.log(visitChain);

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

    let locInfo: [string, number[]][] = dataPages;

    visitChain.reverse();
    for (const [ty, elem] of visitChain) {
      const childrenElements = castChildrenToSourceMappingElement(elem);

      if (locInfo.length !== parentElements.length) {
        console.log("length mismatch", locInfo, parentElements);
        break;
      }

      const idx = parentElements.findIndex((x) => x[0] === ty && x[1] === elem);
      if (idx === -1) {
        console.log("not found", ty, elem, " in ", locInfo);
        break;
      }

      switch (ty) {
        case "typst-page":
          if (locInfo[idx][0] !== "p") {
            console.log("type mismatch", locInfo, ty, elem);
            break;
          }
          break;
        case "typst-group":
          if (locInfo[idx][0] !== "g") {
            console.log("type mismatch", locInfo, ty, elem);
            break;
          }
          break;
        case "typst-text":
          if (locInfo[idx][0] !== "t") {
            console.log("type mismatch", locInfo, ty, elem);
            return;
          }

          return locInfo[idx];
        default:
          console.log("unknown type", ty, elem);
          break;
      }

      parentElements = childrenElements;
      locInfo = locInfo[idx][1].map((x) => {
        if (x >= dataSourceMapping.length) {
          console.log("invalid index", x, dataSourceMapping);
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
    srcMapping.removeEventListener("click", prevSourceMappingHandler);
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

    const basePos = docRoot.getBoundingClientRect();

    const vw = window.innerWidth || 0;
    const left = event.clientX - basePos.left + 0.015 * vw;
    const top = event.clientY - basePos.top + 0.015 * vw;

    triggerRipple(
      docRoot,
      left,
      top,
      "typst-debug-react-ripple",
      "typst-debug-react-ripple-effect .4s linear"
    );
    return;
  });

  docRoot.addEventListener("click", sourceMappingHandler);
}

window.handleTypstLocation = function (
  elem: Element,
  page: number,
  x: number,
  y: number
) {
  const docRoot = findAncestor(elem, "typst-doc");
  const children = docRoot.children;
  let nthPage = 0;
  for (let i = 0; i < children.length; i++) {
    if (children[i].tagName === "g") {
      nthPage++;
    }
    if (nthPage == page) {
      const page = children[i];
      const dataWidth =
        Number.parseFloat(page.getAttribute("data-page-width") || "0") || 0;
      const dataHeight =
        Number.parseFloat(page.getAttribute("data-page-height") || "0") || 0;
      const rect = page.getBoundingClientRect();
      const xOffsetInner = Math.max(0, x / dataWidth - 0.05) * rect.width;
      const yOffsetInner = Math.max(0, y / dataHeight - 0.05) * rect.height;
      const xOffsetInnerFix = (x / dataWidth) * rect.width - xOffsetInner;
      const yOffsetInnerFix = (y / dataHeight) * rect.height - yOffsetInner;

      const docRoot = document.body || document.firstElementChild;
      const basePos = docRoot.getBoundingClientRect();

      const xOffset = rect.left - basePos.left + xOffsetInner;
      const yOffset = rect.top - basePos.top + yOffsetInner;
      const left = xOffset + xOffsetInnerFix;
      const top = yOffset + yOffsetInnerFix;

      window.scrollTo(xOffset, yOffset);

      triggerRipple(
        docRoot,
        left,
        top,
        "typst-jump-ripple",
        "typst-jump-ripple-effect .4s linear"
      );
      return;
    }
  }
};

function triggerRipple(
  docRoot: Element,
  left: number,
  top: number,
  className: string,
  animation: string
) {
  const ripple = document.createElement("div");

  ripple.className = className;
  ripple.style.left = left.toString() + "px";
  ripple.style.top = top.toString() + "px";

  docRoot.appendChild(ripple);

  ripple.style.animation = animation;
  ripple.onanimationend = () => {
    docRoot.removeChild(ripple);
  };
}
