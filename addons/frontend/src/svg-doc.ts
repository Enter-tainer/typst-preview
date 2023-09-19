import { patchRoot } from "./svg-patch";
import { installEditorJumpToHandler } from "./svg-debug-info";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer";

export class SvgDocument {
  /// State fields

  /// whether svg is updating (in triggerSvgUpdate)
  private svgUpdating: boolean;
  /// whether kModule is initialized
  private moduleInitialized: boolean;
  /// current width of `hookedElem`
  private currentContainerWidth: number;
  /// patch queue for updating svg.
  private patchQueue: [string, string][];
  /// enable partial rendering
  private partialRendering: boolean;

  /// There are two scales in this class: The real scale is to adjust the size
  /// of `hookedElem` to fit the svg. The virtual scale (scale ratio) is to let
  /// user zoom in/out the svg. For example:
  /// + the default value of virtual scale is 1, which means the svg is totally
  ///   fit in `hookedElem`.
  /// + if user set virtual scale to 0.5, then the svg will be zoomed out to fit
  ///   in half width of `hookedElem`. "real" current scale of `hookedElem`
  private currentRealScale: number;
  /// "virtual" current scale of `hookedElem`
  private currentScaleRatio: number;

  /// Cache fields

  /// cached `hookedElem.offsetWidth`
  private cachedOffsetWidth: number;
  /// cached `hookedElem.getBoundingClientRect()`
  private cachedBoundingRect: DOMRect;

  constructor(private hookedElem: HTMLElement, public kModule: RenderSession) {
    /// Cache fields
    this.cachedOffsetWidth = 0;
    this.cachedBoundingRect = hookedElem.getBoundingClientRect();

    /// State fields
    this.svgUpdating = false;
    this.moduleInitialized = false;
    this.currentRealScale = 1;
    this.currentContainerWidth = hookedElem.offsetWidth;
    this.patchQueue = [];
    this.partialRendering = false;
    this.currentScaleRatio = 1;

    /// for ctrl-wheel rescaling
    this.hookedElem.style.transformOrigin = "0px 0px";

    installEditorJumpToHandler(this.kModule, this.hookedElem);
    this.installCtrlWheelHandler();
  }

  reset() {
    this.kModule.reset();
    this.moduleInitialized = false;
  }

  installCtrlWheelHandler() {
    // Ctrl+scroll rescaling
    // will disable auto resizing
    // fixed factors, same as pdf.js
    const factors = [
      0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9,
      2.1, 2.4, 2.7, 3, 3.3, 3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
    ];
    const wheelEventHandler = (event: WheelEvent) => {
      if (event.ctrlKey) {
        event.preventDefault();

        if (window.onresize !== null) {
          // is auto resizing
          window.onresize = null;
        }
        const prevScaleRatio = this.currentScaleRatio;
        // Get wheel scroll direction and calculate new scale
        if (event.deltaY < 0) {
          // enlarge
          if (this.currentScaleRatio >= factors.at(-1)!) {
            // already large than max factor
            return;
          } else {
            this.currentScaleRatio = factors
              .filter((x) => x > this.currentScaleRatio)
              .at(0)!;
          }
        } else if (event.deltaY > 0) {
          // reduce
          if (this.currentScaleRatio <= factors.at(0)!) {
            return;
          } else {
            this.currentScaleRatio = factors
              .filter((x) => x < this.currentScaleRatio)
              .at(-1)!;
          }
        } else {
          // no y-axis scroll
          return;
        }
        const scrollFactor = this.currentScaleRatio / prevScaleRatio;
        const scrollX = event.pageX * (scrollFactor - 1);
        const scrollY = event.pageY * (scrollFactor - 1);

        // Apply new scale
        const scale = this.currentRealScale * this.currentScaleRatio;
        this.hookedElem.style.transform = `scale(${scale})`;

        // make sure the cursor is still on the same position
        window.scrollBy(scrollX, scrollY);

        /// Note: even if `window.scrollBy` can trigger viewport change event,
        /// we still manually trigger it for explicitness.
        this.addViewportChange();

        return false;
      }
    };
    const vscodeAPI = typeof acquireVsCodeApi !== "undefined";
    if (vscodeAPI) {
      window.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
    } else {
      document.body.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
    }
  }

  rescale() {
    const newContainerWidth = this.cachedOffsetWidth;
    this.currentRealScale =
      this.currentRealScale *
      (newContainerWidth / this.currentContainerWidth);
    this.currentContainerWidth = newContainerWidth;

    const scale = this.currentRealScale * this.currentScaleRatio;
    const targetScale = `scale(${scale})`;
    if (this.hookedElem.style.transform !== targetScale) {
      this.hookedElem.style.transform = targetScale;
    }
    // console.log("rescale", scale, this.currentScaleRatio);
  }

  initScale() {
    this.currentContainerWidth = this.cachedOffsetWidth;
    const svgWidth = Number.parseFloat(
      this.hookedElem.firstElementChild!.getAttribute("width") || "1"
    );
    this.currentRealScale = this.currentContainerWidth / svgWidth;

    this.rescale();
  }

  private decorateSvgElement(e: SVGElement) {

    const width = e.getAttribute("width")!;
    const height = e.getAttribute("height")!;

    const nextPages = Array.from(e.children).filter(
      (x) => x.classList.contains("typst-page")
    );

    // 25px height padding
    // 25px width padding
    // scale derived from svg width and container with.
    const computedScale = this.cachedOffsetWidth
      ? this.cachedOffsetWidth / this.kModule.doc_width
      : 1;
    // respect current scale ratio
    const scale = this.currentScaleRatio * computedScale;
    const heightPadding = 5 * scale;
    const widthPadding = 0;
    const newWidth = Number.parseFloat(width) + 2 * widthPadding;
    const newHeight = Number.parseFloat(height) + 2 * heightPadding * nextPages.length;
    e.setAttribute("viewBox", `0 0 ${newWidth} ${newHeight}`);
    e.setAttribute("width", `${newWidth}`);
    e.setAttribute("height", `${newHeight}`);

    let accumulatedHeight = 0;
    const firstPage = (nextPages.length ? nextPages[0] : undefined)!;
    for (let i = 0; i < nextPages.length; i++) {
      const nextPage = nextPages[i];
      const pageWidth = Number.parseFloat(nextPage.getAttribute("data-page-width")!);
      const pageHeight = Number.parseFloat(nextPage.getAttribute("data-page-height")!);

      // center the page
      const calculatedPaddedX = (newWidth - pageWidth) / 2;
      const calculatedPaddedY = accumulatedHeight + (i == 0 ? 0 : heightPadding);
      // padding top and bottom
      // const paddedPageWidth = newWidth;
      const sidePadding = ((i === 0 || i + 1 === nextPages.length) ? 0 : heightPadding);
      const paddedPageHeight = pageHeight + sidePadding + heightPadding;

      const outerRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      outerRect.setAttribute("class", "typst-page-outer");
      outerRect.setAttribute("data-page-width", newWidth.toString());
      outerRect.setAttribute("data-page-height", paddedPageHeight.toString());
      outerRect.setAttribute("width", newWidth.toString());
      outerRect.setAttribute("height", paddedPageHeight.toString());
      outerRect.setAttribute("x", "0");
      outerRect.setAttribute("y", accumulatedHeight.toString());
      // white background
      outerRect.setAttribute("fill", "rgba(0, 0, 0, 0.5)");

      const innerRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      innerRect.setAttribute("class", "typst-page-inner");
      innerRect.setAttribute("data-page-width", pageWidth.toString());
      innerRect.setAttribute("data-page-height", pageHeight.toString());
      innerRect.setAttribute("width", pageWidth.toString());
      innerRect.setAttribute("height", pageHeight.toString());
      innerRect.setAttribute("x", calculatedPaddedX.toString());
      innerRect.setAttribute("y", calculatedPaddedY.toString());
      // white background
      innerRect.setAttribute("fill", "white");

      nextPage.setAttribute("transform", `translate(${calculatedPaddedX}, ${calculatedPaddedY})`);
      // todo: this is buggy not preserving order?
      e.insertBefore(innerRect, firstPage);
      e.insertBefore(outerRect, innerRect);
      accumulatedHeight = calculatedPaddedY + pageHeight + heightPadding;
    }
  }

  private toggleViewportChange() {
    const docRect = this.cachedBoundingRect;
    // scale derived from svg width and container with.
    const computedRevScale = this.cachedOffsetWidth
      ? this.kModule.doc_width / this.cachedOffsetWidth
      : 1;
    // respect current scale ratio
    const revScale = computedRevScale / this.currentScaleRatio;
    const left = (window.screenLeft - docRect.left) * revScale;
    const top = (window.screenTop - docRect.top) * revScale;
    const width = window.innerWidth * revScale;
    const height = window.innerHeight * revScale;


    let patchStr: string;
    // with 1px padding to avoid edge error
    if (this.partialRendering) {
      console.log("render_in_window with partial rendering enabled", revScale, left, top, width, height);
      patchStr = this.kModule.render_in_window(
        left - 1,
        top - height - 1,
        left + width + 1,
        top + height * 2 + 1
      );
    } else {
      console.log("render_in_window with partial rendering disabled", 0, 0, 1e33, 1e33)
      patchStr = this.kModule.render_in_window(0, 0, 1e33, 1e33);
    }

    const t2 = performance.now();

    if (this.hookedElem.firstElementChild) {
      const elem = document.createElement("div");
      elem.innerHTML = patchStr;
      const svgElement = elem.firstElementChild as SVGElement;
      this.decorateSvgElement(svgElement!);
      patchRoot(this.hookedElem.firstElementChild as SVGElement, svgElement);
    } else {
      this.hookedElem.innerHTML = patchStr;
      this.decorateSvgElement(this.hookedElem.firstElementChild! as SVGElement);
    }
    const t3 = performance.now();

    return [t2, t3];
  }

  private processQueue(svgUpdateEvent: [string, string]) {
    let t0 = performance.now();
    let t1 = undefined;
    let t2 = undefined;
    let t3 = undefined;
    const eventName = svgUpdateEvent[0];
    switch (eventName) {
      case "new":
      case "diff-v1": {
        if (eventName === "new") {
          this.reset();
        }
        // todo: remove type cast
        this.kModule.merge_delta(svgUpdateEvent[1] as unknown as Uint8Array);

        t1 = performance.now();
        // todo: trigger viewport change once
        const [_t2, _t3] = this.toggleViewportChange();
        t2 = _t2;
        t3 = _t3;
        this.moduleInitialized = true;
        break;
      }
      case "viewport-change": {
        if (!this.moduleInitialized) {
          console.log("viewport-change before initialization");
          t0 = t1 = t2 = t3 = performance.now();
          break;
        }
        t1 = performance.now();
        const [_t2, _t3] = this.toggleViewportChange();
        t2 = _t2;
        t3 = _t3;
        break;
      }
      default:
        console.log("svgUpdateEvent", svgUpdateEvent);
        t0 = t1 = t2 = t3 = performance.now();
        break;
    }

    /// perf event
    const d = (e: string, x: number, y: number) =>
      `${e} ${(y - x).toFixed(2)} ms`;
    console.log(
      [
        d("parse", t0, t1),
        d("check-diff", t1, t2),
        d("patch", t2, t3),
        d("total", t0, t3),
      ].join(", ")
    );
  }

  private triggerSvgUpdate() {
    if (this.svgUpdating) {
      return;
    }

    this.svgUpdating = true;
    const doSvgUpdate = () => {
      this.cachedOffsetWidth = this.hookedElem.offsetWidth;
      this.cachedBoundingRect = this.hookedElem.getBoundingClientRect();

      if (this.patchQueue.length === 0) {
        this.svgUpdating = false;
        this.postprocessChanges();
        return;
      }
      try {
        while (this.patchQueue.length > 0) {
          this.processQueue(this.patchQueue.shift()!);
        }

        // to hide the rescale behavior at the first time
        const docRoot = this.hookedElem.firstElementChild as SVGElement;
        if (docRoot) {
          this.initScale();
        }

        requestAnimationFrame(doSvgUpdate);
      } catch (e) {
        console.error(e);
        this.svgUpdating = false;
        this.postprocessChanges();
      }
    };
    requestAnimationFrame(doSvgUpdate);
  }

  private postprocessChanges() {
    const docRoot = this.hookedElem.firstElementChild as SVGElement;
    if (docRoot) {
      window.initTypstSvg(docRoot);

      this.initScale();
    }
  }

  addChangement(change: [string, string]) {
    if (!this.partialRendering && change[0] === "viewport-change") {
      return;
    }
    if (change[0] === "new") {
      this.patchQueue.splice(0, this.patchQueue.length);
    }
    this.patchQueue.push(change);
    this.triggerSvgUpdate();
  }

  addViewportChange() {
    this.addChangement(["viewport-change", ""]);
  }

  setPartialRendering(partialRendering: boolean) {
    this.partialRendering = partialRendering;
  }
}
