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

  /// Style fields

  borderColor: string;

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

    /// Style fields
    this.borderColor = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-background')
      || 'rgba(0, 0, 0, 0.5)';

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
    // hide: white unaligned page
    // todo: better solution
    const widthHideFactor = 1e-3;

    const newContainerWidth = this.cachedOffsetWidth;
    this.currentRealScale =
      this.currentRealScale *
      (newContainerWidth / this.currentContainerWidth) + widthHideFactor;
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
    // todo: typst.ts return a ceil width so we miss 1px here
    // after we fix this, we can set the factor to 0.01
    const backgroundHideFactor = 1;

    /// Prepare scale
    // scale derived from svg width and container with.
    const computedScale = this.cachedOffsetWidth
      ? this.cachedOffsetWidth / this.kModule.doc_width
      : 1;
    // respect current scale ratio
    const scale = this.currentScaleRatio * computedScale;

    /// Retrieve original width, height and pages
    const width = e.getAttribute("width")!;
    // const height = e.getAttribute("height")!;
    const nextPages = Array.from(e.children).filter(
      (x) => x.classList.contains("typst-page")
    );

    /// Calculate new width, height
    // 5px height margin, 0px width margin (it is buggy to add width margin)
    const heightMargin = 5 * scale;
    const widthMargin = 0;
    const newWidth = Number.parseFloat(width) + 2 * widthMargin + 1e-5;

    /// Apply new pages
    let accumulatedHeight = 0;
    const firstPage = (nextPages.length ? nextPages[0] : undefined)!;
    let firstRect: SVGRectElement = undefined!;

    for (let i = 0; i < nextPages.length; i++) {
      /// Retrieve page width, height
      const nextPage = nextPages[i];
      const pageWidth = Number.parseFloat(nextPage.getAttribute("data-page-width")!);
      const pageHeight = Number.parseFloat(nextPage.getAttribute("data-page-height")!);

      /// center the page and add margin
      const calculatedPaddedXRough = (newWidth - pageWidth) / 2;
      const calculatedPaddedX = Math.abs(calculatedPaddedXRough) < 1e-3 ? 0 : calculatedPaddedXRough;
      const calculatedPaddedY = accumulatedHeight + (i == 0 ? 0 : heightMargin);
      const translateAttr = `translate(${calculatedPaddedX}, ${calculatedPaddedY})`;

      /// Create inner rectangle
      const innerRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      innerRect.setAttribute("class", "typst-page-inner");
      innerRect.setAttribute("data-page-width", pageWidth.toString());
      innerRect.setAttribute("data-page-height", pageHeight.toString());
      innerRect.setAttribute("width", Math.floor((pageWidth - backgroundHideFactor) * 100).toString());
      innerRect.setAttribute("height", Math.floor((pageHeight - backgroundHideFactor) * 100).toString());
      innerRect.setAttribute("x", "0");
      innerRect.setAttribute("y", "0");
      innerRect.setAttribute("transform", `${translateAttr} scale(0.01)`);
      // white background
      innerRect.setAttribute("fill", "white");

      /// Move page to the correct position
      nextPage.setAttribute("transform", translateAttr);

      /// Insert rectangles
      // todo: this is buggy not preserving order?
      e.insertBefore(innerRect, firstPage);
      if (!firstRect) {
        firstRect = innerRect;
      }

      accumulatedHeight = calculatedPaddedY + pageHeight + (i + 1 === nextPages.length ? 0 : heightMargin);
    }

    /// Apply new width, height
    const newHeight = accumulatedHeight;

    /// Create outer rectangle
    if (firstPage) {
      const rectHeight = Math.ceil(newHeight).toString();

      const outerRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      outerRect.setAttribute("class", "typst-page-outer");
      outerRect.setAttribute("data-page-width", newWidth.toString());
      outerRect.setAttribute("data-page-height", rectHeight);
      outerRect.setAttribute("width", newWidth.toString());
      outerRect.setAttribute("height", rectHeight);
      outerRect.setAttribute("x", "0");
      outerRect.setAttribute("y", "0");
      // white background
      outerRect.setAttribute("fill", this.borderColor);
      e.insertBefore(outerRect, firstRect);
    }

    // hide unaligned width
    const newWidthFloor = newWidth - 1e-5;
    const newHeightFloor = newHeight - 1e-5;
    e.setAttribute("viewBox", `0 0 ${newWidthFloor} ${newHeightFloor}`);
    e.setAttribute("width", `${newWidthFloor}`);
    e.setAttribute("height", `${newHeightFloor}`);
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
