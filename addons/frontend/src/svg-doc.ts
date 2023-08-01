import { patchRoot } from "./svg-patch";
import { installEditorJumpToHandler } from "./svg-debug-info";

export class SvgDocument {
  /// State fields

  /// whether svg is updating (in triggerSvgUpdate)
  svgUpdating: boolean;
  /// whether kModule is initialized
  moduleInitialized: boolean;
  /// current scale of `hookedElem`
  currentContainerScale: number;
  /// current width of `hookedElem`
  currentContainerWidth: number;
  /// patch queue for updating svg.
  patchQueue: [string, string][];

  /// Cache fields

  /// cached `hookedElem.offsetWidth`
  cachedOffsetWidth: number;
  /// cached `hookedElem.getBoundingClientRect()`
  cachedBoundingRect: DOMRect;

  constructor(private hookedElem: HTMLElement, public kModule: any) {
    /// Cache fields
    this.cachedOffsetWidth = 0;
    this.cachedBoundingRect = hookedElem.getBoundingClientRect();

    /// State fields
    this.svgUpdating = false;
    this.moduleInitialized = false;
    this.currentContainerScale = 1;
    this.currentContainerWidth = hookedElem.offsetWidth;
    this.patchQueue = [];

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
        const prevScale = this.currentContainerScale;
        // Get wheel scroll direction and calculate new scale
        if (event.deltaY < 0) {
          // enlarge
          if (this.currentContainerScale >= factors.at(-1)!) {
            // already large than max factor
            return;
          } else {
            this.currentContainerScale = factors
              .filter((x) => x > this.currentContainerScale)
              .at(0)!;
          }
        } else if (event.deltaY > 0) {
          // reduce
          if (this.currentContainerScale <= factors.at(0)!) {
            return;
          } else {
            this.currentContainerScale = factors
              .filter((x) => x < this.currentContainerScale)
              .at(-1)!;
          }
        } else {
          // no y-axis scroll
          return;
        }
        const scrollFactor = this.currentContainerScale / prevScale;
        const scrollX = event.pageX * (scrollFactor - 1);
        const scrollY = event.pageY * (scrollFactor - 1);

        // Apply new scale
        this.hookedElem.style.transform = `scale(${this.currentContainerScale})`;

        // make sure the cursor is still on the same position
        window.scrollBy(scrollX, scrollY);

        /// Note: even if `window.scrollBy` can trigger viewport change event,
        /// we still manually trigger it for explicitness.
        this.addViewportChange();
      }
    };
    const vscodeAPI = typeof acquireVsCodeApi !== "undefined";
    if (vscodeAPI) {
      window.addEventListener("wheel", wheelEventHandler);
    } else {
      document.body.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
    }
  }

  rescale() {
    const newContainerWidth = this.cachedOffsetWidth;
    this.currentContainerScale =
      this.currentContainerScale *
      (newContainerWidth / this.currentContainerWidth);
    this.currentContainerWidth = newContainerWidth;

    const targetScale = `scale(${this.currentContainerScale})`;
    if (this.hookedElem.style.transform !== targetScale) {
      this.hookedElem.style.transform = targetScale;
    }
    // console.log("rescale", this.currentContainerScale);
  }

  initScale() {
    this.currentContainerWidth = this.cachedOffsetWidth;
    const svgWidth = Number.parseFloat(
      this.hookedElem.firstElementChild!.getAttribute("width") || "1"
    );
    this.currentContainerScale = this.currentContainerWidth / svgWidth;

    this.rescale();
  }

  private toggleViewportChange() {
    const docRect = this.cachedBoundingRect;
    const scale = this.cachedOffsetWidth
      ? this.kModule.doc_width / this.cachedOffsetWidth
      : 1;
    const left = (window.screenLeft - docRect.left) * scale;
    const top = (window.screenTop - docRect.top) * scale;
    const width = window.innerWidth * scale;
    const height = window.innerHeight * scale;

    console.log("render_in_window", scale, left, top, width, height);

    // with 1px padding to avoid edge error
    const patchStr = this.kModule.render_in_window(
      left - 1,
      top - height - 1,
      left + width + 1,
      top + height * 2 + 1
    );
    const t2 = performance.now();

    if (this.hookedElem.firstElementChild) {
      const elem = document.createElement("div");
      elem.innerHTML = patchStr;
      const svgElement = elem.firstElementChild as SVGElement;
      patchRoot(this.hookedElem.firstElementChild as SVGElement, svgElement);
    } else {
      this.hookedElem.innerHTML = patchStr;
    }
    const t3 = performance.now();

    return [t2, t3];
  }

  private processQueue(svgUpdateEvent: [string, string]) {
    let t0 = performance.now();
    let t1 = undefined;
    let t2 = undefined;
    let t3 = undefined;
    switch (svgUpdateEvent[0]) {
      case "diff-v1": {
        this.kModule.merge_delta(svgUpdateEvent[1]);

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
    if (change[0] === "new") {
      this.patchQueue.splice(0, this.patchQueue.length);
    }
    this.patchQueue.push(change);
    this.triggerSvgUpdate();
  }

  addViewportChange() {
    this.addChangement(["viewport-change", ""]);
  }
}
