import { patchRoot } from "./svg-patch";

export class SvgDocument {
  currentScale: number;
  imageContainerWidth: number;
  patchQueue: [string, string][];
  svgUpdating: boolean;

  constructor(private hookedElem: HTMLElement) {
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
    this.hookedElem.addEventListener("wheel", (event) => {
      if (event.ctrlKey) {
        event.preventDefault();

        if (window.onresize !== null) {
          // is auto resizing
          window.onresize = null;
        }

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

        // Apply new scale
        this.hookedElem.style.transformOrigin = "0 0";
        this.hookedElem.style.transform = `scale(${this.currentScale})`;
      }
    });
  }

  rescale() {
    const newImageContainerWidth = this.hookedElem.offsetWidth;
    this.currentScale =
      this.currentScale * (newImageContainerWidth / this.imageContainerWidth);
    this.imageContainerWidth = newImageContainerWidth;

    this.hookedElem.style.transformOrigin = "0px 0px";
    this.hookedElem.style.transform = `scale(${this.currentScale})`;
  }

  initScale() {
    this.imageContainerWidth = this.hookedElem.offsetWidth;
    const svgWidth = Number.parseFloat(
      this.hookedElem.firstElementChild!.getAttribute("width") || "1"
    );
    this.currentScale = this.imageContainerWidth / svgWidth;

    this.rescale();
  }

  private postprocessChanges() {
    const docRoot = this.hookedElem.firstElementChild as SVGElement;
    if (docRoot) {
      let srcElement = (this.hookedElem.lastElementChild || undefined) as
        | HTMLDivElement
        | undefined;
      if (
        !srcElement ||
        !srcElement.classList.contains("typst-source-mapping")
      ) {
        srcElement = undefined;
      }
      window.initTypstSvg(docRoot, srcElement);

      this.initScale();
    }
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

        t3 = performance.now();
        break;
      case "diff-v0":
        const elem = document.createElement("div");
        elem.innerHTML = svgUpdateEvent[1];
        const svgElement = elem.firstElementChild as SVGElement;
        t1 = performance.now();
        patchRoot(this.hookedElem.firstElementChild as SVGElement, svgElement);
        t2 = performance.now();

        t3 = performance.now();
        break;
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
      if (this.patchQueue.length === 0) {
        this.svgUpdating = false;
        this.postprocessChanges();
        return;
      }
      try {
        while (this.patchQueue.length > 0) {
          this.processQueue(this.patchQueue.shift()!);
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
}
