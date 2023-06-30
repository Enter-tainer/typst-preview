import { triggerRipple } from "./svg-animation";
import { parseSourceMappingNode, initSourceMapping } from "./svg-debug-info.ts";

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

function findAncestor(el: Element, cls: string) {
  while (el && !el.classList.contains(cls)) {
    el = el.parentElement!;
  }
  return el;
}

window.initTypstSvg = function (
  docRoot: SVGElement,
  srcMapping?: HTMLDivElement
) {
  /// initialize pseudo links
  var elements = docRoot.getElementsByClassName("pseudo-link");
  for (var i = 0; i < elements.length; i++) {
    let elem = elements[i] as SVGAElement;
    elem.addEventListener("mousemove", mouseMoveToLink);
    elem.addEventListener("mouseleave", mouseLeaveFromLink);
  }

  /// initialize text layout at client side
  if (false) {
    setTimeout(() => {
      layoutText(docRoot);
    }, 0);
  }

  /// initialize debug info (source mapping)
  if (srcMapping) {
    const dataPages = srcMapping
      .getAttribute("data-pages")!
      .split("|")
      .map(parseSourceMappingNode);
    const dataSourceMapping = srcMapping
      .getAttribute("data-source-mapping")!
      .split("|")
      .map(parseSourceMappingNode);
    srcMapping.remove();
    setTimeout(() => {
      initSourceMapping(docRoot, dataPages, dataSourceMapping);
    }, 0);
  }

  return;

  function mouseMoveToLink(event: MouseEvent) {
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
  }

  function mouseLeaveFromLink(event: MouseEvent) {
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
  }
};

function layoutText (svg: SVGElement) {
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

window.handleTypstLocation = function (
  elem: Element,
  page: number,
  x: number,
  y: number
) {
  const docRoot = findAncestor(elem, "typst-doc");
  if (!docRoot) {
    console.warn("no typst-doc found", elem);
    return;
  }
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

      // window.scrollTo(xOffset, yOffset);
      /// this would be better for preview svg
      // x: basePos.left, y: yOffset, 
      window.scrollTo({ behavior: "smooth", left: basePos.left, top: yOffset });

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
