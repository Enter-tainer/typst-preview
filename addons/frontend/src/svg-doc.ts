import { patchRoot } from "./svg-patch";
import { removeSourceMappingHandler } from "./svg-debug-info";

export class SvgDocument {
  currentScale: number;
  cachedOffsetWidth: number;
  cachedBoundingRect: DOMRect;
  imageContainerWidth: number;
  patchQueue: [string, string][];
  svgUpdating: boolean;
  holdingSrcElement: HTMLDivElement | undefined;
  svgModule: any;

  constructor(private hookedElem: HTMLElement) {
    this.cachedOffsetWidth = 0;
    this.cachedBoundingRect = hookedElem.getBoundingClientRect();

    this.currentScale = 1;
    this.imageContainerWidth = hookedElem.offsetWidth;
    this.patchQueue = [];
    this.svgUpdating = false;

    // Ctrl+scroll rescaling
    // will disable auto resizing
    // fixed factors, same as pdf.js
    const factors = [
      0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.3, 1.5, 1.7, 1.9,
      2.1, 2.4, 2.7, 3, 3.3, 3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7, 7.7, 8.5, 9.4, 10,
    ];
    this.hookedElem.style.transformOrigin = "0px 0px";
    const wheelEventHandler = (event: WheelEvent) => {
      if (event.ctrlKey) {
        event.preventDefault();

        if (window.onresize !== null) {
          // is auto resizing
          window.onresize = null;
        }
        const prevScale = this.currentScale;
        // Get wheel scroll direction and calculate new scale
        if (event.deltaY < 0) {
          // enlarge
          if (this.currentScale >= factors.at(-1)!) {
            // already large than max factor
            return;
          } else {
            this.currentScale = factors
              .filter((x) => x > this.currentScale)
              .at(0)!;
          }
        } else if (event.deltaY > 0) {
          // reduce
          if (this.currentScale <= factors.at(0)!) {
            return;
          } else {
            this.currentScale = factors
              .filter((x) => x < this.currentScale)
              .at(-1)!;
          }
        } else {
          // no y-axis scroll
          return;
        }
        const scrollFactor = this.currentScale / prevScale;
        const scrollX = event.pageX * (scrollFactor - 1);
        const scrollY = event.pageY * (scrollFactor - 1);

        // Apply new scale
        this.hookedElem.style.transform = `scale(${this.currentScale})`;
        this.addViewportChange();

        // make sure the cursor is still on the same position
        window.scrollBy(scrollX, scrollY);
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

  setModule(svgModule: any) {
    this.svgModule = svgModule;
  }

  rescale() {
    const newImageContainerWidth = this.cachedOffsetWidth;
    this.currentScale =
      this.currentScale * (newImageContainerWidth / this.imageContainerWidth);
    this.imageContainerWidth = newImageContainerWidth;

    const targetScale = `scale(${this.currentScale})`;
    if (this.hookedElem.style.transform !== targetScale) {
      this.hookedElem.style.transform = targetScale;
    }
    console.log("rescale", this.currentScale);
  }

  initScale() {
    this.imageContainerWidth = this.cachedOffsetWidth;
    const svgWidth = Number.parseFloat(
      this.hookedElem.firstElementChild!.getAttribute("width") || "1"
    );
    this.currentScale = this.imageContainerWidth / svgWidth;

    this.rescale();
  }

  private grabSourceMappingElement(svgElement: HTMLElement) {
    let srcElement = (svgElement.lastElementChild || undefined) as
      | HTMLDivElement
      | undefined;
    if (!srcElement || !srcElement.classList.contains("typst-source-mapping")) {
      srcElement = undefined;
    }
    this.holdingSrcElement = srcElement;
  }

  private postprocessChanges() {
    const docRoot = this.hookedElem.firstElementChild as SVGElement;
    if (docRoot) {
      window.initTypstSvg(docRoot, this.holdingSrcElement);
      this.holdingSrcElement = undefined;

      this.initScale();
    }
  }

  private toggleViewportChange() {
    const docRect = this.cachedBoundingRect;
    const scale = this.cachedOffsetWidth
      ? this.svgModule.doc_width / this.cachedOffsetWidth
      : 1;
    const left = (window.screenLeft - docRect.left) * scale;
    const top = (window.screenTop - docRect.top) * scale;
    const width = window.innerWidth * scale;
    const height = window.innerHeight * scale;

    console.log("render_in_window", scale, left, top, width, height);

    // with 1px padding to avoid edge error
    const patchStr = this.svgModule.render_in_window(
      left - 1,
      top - 1 - height,
      left + width + 1,
      top + height * 3 + 1
    );
    const t2 = performance.now();

    if (this.hookedElem.firstElementChild) {
      const elem = document.createElement("div");
      elem.innerHTML = patchStr;
      const svgElement = elem.firstElementChild as SVGElement;
      patchRoot(this.hookedElem.firstElementChild as SVGElement, svgElement);
      this.grabSourceMappingElement(elem);
    } else {
      this.hookedElem.innerHTML = patchStr;
      this.grabSourceMappingElement(this.hookedElem);
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
      case "new":
        this.hookedElem.innerHTML = svgUpdateEvent[1];
        t1 = t2 = performance.now();

        this.grabSourceMappingElement(this.hookedElem);

        t3 = performance.now();
        break;
      case "diff-v0":
        /// although there is still a race condition, we try to avoid it
        if (this.hookedElem.firstElementChild) {
          removeSourceMappingHandler(
            this.hookedElem.firstElementChild as SVGElement
          );
        }

        const elem = document.createElement("div");
        elem.innerHTML = svgUpdateEvent[1];
        const svgElement = elem.firstElementChild as SVGElement;
        t1 = performance.now();
        patchRoot(this.hookedElem.firstElementChild as SVGElement, svgElement);
        this.grabSourceMappingElement(elem);
        t2 = performance.now();

        t3 = performance.now();
        break;
      case "diff-v1": {
        this.svgModule.merge_delta(svgUpdateEvent[1]);

        t1 = performance.now();
        // todo: trigger viewport change once
        const [_t2, _t3] = this.toggleViewportChange();
        t2 = _t2;
        t3 = _t3;
        break;
      }
      case "viewport-change": {
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

    console.log(
      `parse ${(t1 - t0).toFixed(2)} ms, replace ${(t2 - t1).toFixed(
        2
      )} ms, postprocess ${(t3 - t2).toFixed(2)} ms, total ${(t3 - t0).toFixed(
        2
      )} ms`
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
