import { TypstPatchAttrs, patchSvgToContainer } from "./typst-patch";
import { installEditorJumpToHandler, removeSourceMappingHandler } from "./typst-debug-info";
import { RenderSession } from "@myriaddreamin/typst.ts/dist/esm/renderer.mjs";
import { CanvasPage, patchOutlineEntry } from "./typst-outline";

export interface ContainerDOMState {
  /// cached `hookedElem.offsetWidth` or `hookedElem.innerWidth`
  width: number;
  /// cached `hookedElem.offsetHeight` or `hookedElem.innerHeight`
  height: number;
  /// cached `hookedElem.getBoundingClientRect()`
  /// We only use `left` and `top` here.
  boundingRect: {
    left: number;
    top: number;
  };
}

export enum RenderMode {
  Svg,
  Canvas,
}

export enum PreviewMode {
  Doc,
  Slide,
}

// let rnd = 0;

class TypstDocumentImpl {
  /// Configuration fields

  /// enable partial rendering
  partialRendering: boolean;
  /// render mode
  renderMode: RenderMode = RenderMode.Svg;
  /// preview mode
  previewMode: PreviewMode = PreviewMode.Doc;
  /// whether this is a content preview
  isContentPreview: boolean;
  /// whether this content preview will mix outline titles
  isMixinOutline: boolean = false;
  /// background color
  backgroundColor: string;
  /// pixel per pt
  pixelPerPt: number = 3;
  /// customized way to retrieving dom state
  retrieveDOMState: () => ContainerDOMState;

  /// State fields

  /// whether svg is updating (in triggerSvgUpdate)
  isRendering: boolean = false;
  /// whether kModule is initialized
  moduleInitialized: boolean = false;
  /// patch queue for updating data.
  patchQueue: [string, string][] = [];
  /// resources to dispose
  private disposeList: (() => void)[] = [];

  /// There are two scales in this class: The real scale is to adjust the size
  /// of `hookedElem` to fit the svg. The virtual scale (scale ratio) is to let
  /// user zoom in/out the svg. For example:
  /// + the default value of virtual scale is 1, which means the svg is totally
  ///   fit in `hookedElem`.
  /// + if user set virtual scale to 0.5, then the svg will be zoomed out to fit
  ///   in half width of `hookedElem`. "real" current scale of `hookedElem`
  currentRealScale: number = 1;
  /// "virtual" current scale of `hookedElem`
  currentScaleRatio: number = 1;
  /// timeout for delayed viewport change
  vpTimeout: any = undefined;
  /// sampled by last render time.
  sampledRenderTime: number = 0;
  /// page to partial render
  partialRenderPage: number = 0;
  /// outline data
  outline: any = undefined;
  /// cursor position in form of [page, x, y]
  cursorPosition?: [number, number, number] = undefined;
  // id: number = rnd++;


  /// Cache fields

  /// cached state of container, default to retrieve state from `this.hookedElem`
  cachedDOMState: ContainerDOMState = {
    width: 0,
    height: 0,
    boundingRect: {
      left: 0,
      top: 0,
    },
  };

  constructor(private hookedElem: HTMLElement, public kModule: RenderSession, options?: Options) {

    /// Apply configuration
    {
      const { renderMode, previewMode, isContentPreview, retrieveDOMState } = options || {};
      this.partialRendering = false;
      if (renderMode !== undefined) {
        this.renderMode = renderMode;
      }
      if (previewMode !== undefined) {
        this.previewMode = previewMode;
      }
      this.isContentPreview = isContentPreview || false;
      this.retrieveDOMState = retrieveDOMState || (() => {
        return {
          width: this.hookedElem.offsetWidth,
          height: this.hookedElem.offsetHeight,
          boundingRect: this.hookedElem.getBoundingClientRect(),
        }
      });
      this.backgroundColor = getComputedStyle(document.documentElement).getPropertyValue(
        '--typst-preview-background-color');
    }
    if (this.isContentPreview) {
      // content preview has very bad performance without partial rendering
      this.partialRendering = true;
    }

    if (this.isContentPreview) {
      this.renderMode = RenderMode.Canvas;
      this.pixelPerPt = 1;
      this.isMixinOutline = true;
    }

    // if init scale == 1
    // hide scrollbar if scale == 1

    this.hookedElem.classList.add("hide-scrollbar-x");
    this.hookedElem.parentElement?.classList.add("hide-scrollbar-x");
    if (this.previewMode === PreviewMode.Slide) {
      this.hookedElem.classList.add("hide-scrollbar-y");
      document.body.classList.add("hide-scrollbar-y");
    }

    if (this.renderMode === RenderMode.Svg) {
      installEditorJumpToHandler(this.kModule, this.hookedElem);
    }
    this.installCtrlWheelHandler();
  }

  reset() {
    this.kModule.reset();
    this.moduleInitialized = false;
  }

  private installCtrlWheelHandler() {
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

        // retrieve dom state before any operation
        this.cachedDOMState = this.retrieveDOMState();

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

        // hide scrollbar if scale == 1
        if (Math.abs(this.currentScaleRatio - 1) < 1e-5) {
          this.hookedElem.classList.add("hide-scrollbar-x");
          document.body.classList.add("hide-scrollbar-x");
          if (this.previewMode === PreviewMode.Slide) {
            this.hookedElem.classList.add("hide-scrollbar-y");
            document.body.classList.add("hide-scrollbar-y");
          }
        } else {
          this.hookedElem.classList.remove("hide-scrollbar-x");
          document.body.classList.remove("hide-scrollbar-x");
          if (this.previewMode === PreviewMode.Slide) {
            this.hookedElem.classList.remove("hide-scrollbar-y");
            document.body.classList.remove("hide-scrollbar-y");
          }
        }

        // reserve space to scroll down
        const svg = this.hookedElem.firstElementChild! as SVGElement;
        if (svg) {
          const scaleRatio = this.getSvgScaleRatio();

          const dataHeight = Number.parseFloat(svg.getAttribute("data-height")!);
          const scaledHeight = Math.ceil(dataHeight * scaleRatio);

          // we increase the height by 2 times.
          // The `2` is only a magic number that is large enough.
          this.hookedElem.style.height = `${scaledHeight * 2}px`;
        }

        // make sure the cursor is still on the same position
        window.scrollBy(scrollX, scrollY);
        // toggle scale change event
        this.addViewportChange();

        return false;
      }
    };

    const vscodeAPI = typeof acquireVsCodeApi !== "undefined";
    if (vscodeAPI) {
      window.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
      this.disposeList.push(() => {
        window.removeEventListener("wheel", wheelEventHandler);
      });
    } else {
      document.body.addEventListener("wheel", wheelEventHandler, {
        passive: false,
      });
      this.disposeList.push(() => {
        document.body.removeEventListener("wheel", wheelEventHandler);
      });
    }
  }

  /// Get current scale from html to svg
  // Note: one should retrieve dom state before rescale
  getSvgScaleRatio() {
    const svg = this.hookedElem.firstElementChild as SVGElement;
    if (!svg) {
      return 0;
    }

    const container = this.cachedDOMState;

    const svgWidth = Number.parseFloat(
      svg.getAttribute("data-width") || svg.getAttribute("width") || "1"
    );
    const svgHeight = Number.parseFloat(
      svg.getAttribute("data-height") || svg.getAttribute("height") || "1"
    );
    this.currentRealScale =
      this.previewMode === PreviewMode.Slide ?
        Math.min(container.width / svgWidth, container.height / svgHeight) :
        container.width / svgWidth;

    return this.currentRealScale * this.currentScaleRatio;
  }

  private rescale() {
    switch (this.renderMode) {
      case RenderMode.Svg: {
        this.rescaleSvg();
        break;
      }
      case RenderMode.Canvas: {
        this.rescaleCanvas();
        break;
      }
      default: {
        throw new Error(`unknown render mode ${this.renderMode}`);
      }
    }
  }

  private rescaleCanvas() {
    // get dom state from cache, so we are free from layout reflowing
    // Note: one should retrieve dom state before rescale
    const container = this.cachedDOMState;

    // get dom state from cache, so we are free from layout reflowing
    const docDiv = this.hookedElem.firstElementChild! as HTMLDivElement;
    if (!docDiv) {
      return;
    }

    let isFirst = true;

    const rescale = (canvasContainer: HTMLElement) => {
      // console.log(ch);
      if (isFirst) {
        isFirst = false;
        canvasContainer.style.marginTop = `0px`;
      } else {
        canvasContainer.style.marginTop = `${this.isContentPreview ? 6 : 5}px`;
      }
      let elem = canvasContainer.firstElementChild as HTMLDivElement;

      const canvasWidth = Number.parseFloat(elem.getAttribute("data-page-width")!);
      const canvasHeight = Number.parseFloat(elem.getAttribute("data-page-height")!);

      this.currentRealScale =
        this.previewMode === PreviewMode.Slide ?
          Math.min(container.width / canvasWidth, container.height / canvasHeight) :
          container.width / canvasWidth;
      const scale = this.currentRealScale * this.currentScaleRatio;

      // apply scale
      const appliedScale = (scale / this.pixelPerPt).toString();


      // set data applied width and height to memoize change
      if (elem.getAttribute("data-applied-scale") !== appliedScale) {
        elem.setAttribute("data-applied-scale", appliedScale);
        // apply translate
        const scaledWidth = Math.ceil(canvasWidth * scale);
        const scaledHeight = Math.ceil(canvasHeight * scale);

        elem.style.width = `${scaledWidth}px`;
        elem.style.height = `${scaledHeight}px`;
        elem.style.transform = `scale(${appliedScale})`;

        if (this.previewMode === PreviewMode.Slide) {

          const widthAdjust = Math.max((container.width - scaledWidth) / 2, 0);
          const heightAdjust = Math.max((container.height - scaledHeight) / 2, 0);
          docDiv.style.transform = `translate(${widthAdjust}px, ${heightAdjust}px)`;
        }
      }
    }

    if (this.isContentPreview) {
      isFirst = false;
      const rescaleChildren = (elem: HTMLElement) => {
        for (const ch of elem.children) {
          let canvasContainer = ch as HTMLElement;
          if (canvasContainer.classList.contains('typst-page')) {
            rescale(canvasContainer);
          }
          if (canvasContainer.classList.contains('typst-outline')) {
            rescaleChildren(canvasContainer);
          }
        }
      }

      rescaleChildren(docDiv);
    } else {
      for (const ch of docDiv.children) {
        let canvasContainer = ch as HTMLDivElement;
        if (!canvasContainer.classList.contains('typst-page')) {
          continue;
        }
        rescale(canvasContainer);
      }
    }
  }


  // Note: one should retrieve dom state before rescale
  private rescaleSvg() {
    // get dom state from cache, so we are free from layout reflowing
    const svg = this.hookedElem.firstElementChild! as SVGElement;

    const scale = this.getSvgScaleRatio();
    if (scale === 0) {
      console.warn("determine scale as 0, skip rescale");
      return;
    }

    // get dom state from cache, so we are free from layout reflowing
    const container = this.cachedDOMState;

    // apply scale
    const dataWidth = Number.parseFloat(svg.getAttribute("data-width")!);
    const dataHeight = Number.parseFloat(svg.getAttribute("data-height")!);
    const appliedWidth = (dataWidth * scale).toString();
    const appliedHeight = (dataHeight * scale).toString();
    const scaledWidth = Math.ceil(dataWidth * scale);
    const scaledHeight = Math.ceil(dataHeight * scale);

    // set data applied width and height to memoize change
    if (svg.getAttribute("data-applied-width") !== appliedWidth) {
      svg.setAttribute("data-applied-width", appliedWidth);
      svg.setAttribute("width", `${scaledWidth}`);
    }
    if (svg.getAttribute("data-applied-height") !== appliedHeight) {
      svg.setAttribute("data-applied-height", appliedHeight);
      svg.setAttribute("height", `${scaledHeight}`);
    }

    const widthAdjust = Math.max((container.width - scaledWidth) / 2, 0);
    let transformAttr = '';
    if (this.previewMode === PreviewMode.Slide) {
      const heightAdjust = Math.max((container.height - scaledHeight) / 2, 0);
      transformAttr = `translate(${widthAdjust}px, ${heightAdjust}px)`;
    } else {
      transformAttr = `translate(${widthAdjust}px, 0px)`;
    }
    if (this.hookedElem.style.transform !== transformAttr) {
      this.hookedElem.style.transform = transformAttr;
    }

    // change height of the container back from `installCtrlWheelHandler` hack
    if (this.hookedElem.style.height) {
      this.hookedElem.style.removeProperty("height");
    }
  }

  private decorateSvgElement(svg: SVGElement, mode: PreviewMode) {
    const container = this.cachedDOMState;

    // the <rect> could only have integer width and height
    // so we scale it by 100 to make it more accurate
    const INNER_RECT_UNIT = 100;
    const INNER_RECT_SCALE = 'scale(0.01)';

    /// Caclulate width
    let maxWidth = 0;

    const nextPages = (() => {
      /// Retrieve original pages
      const filteredNextPages = Array.from(svg.children).filter(
        (x) => x.classList.contains("typst-page")
      );

      if (mode === PreviewMode.Doc) {
        return filteredNextPages;
      } else if (mode === PreviewMode.Slide) {
        // already fetched pages info
        const pageOffset = this.partialRenderPage;
        return [filteredNextPages[pageOffset]];
      } else {
        throw new Error(`unknown preview mode ${mode}`);
      }
    })().map((elem) => {
      const width = Number.parseFloat(elem.getAttribute("data-page-width")!);
      const height = Number.parseFloat(elem.getAttribute("data-page-height")!);
      maxWidth = Math.max(maxWidth, width);
      return {
        elem,
        width,
        height,
      }
    });

    /// Adjust width
    if (maxWidth < 1e-5) {
      maxWidth = 1;
    }
    // const width = e.getAttribute("width")!;
    // const height = e.getAttribute("height")!;

    /// Prepare scale
    // scale derived from svg width and container with.
    const computedScale = container.width
      ? container.width / maxWidth
      : 1;
    // respect current scale ratio
    const scale = 1 / (this.currentScaleRatio * computedScale);
    const fontSize = 12 * scale;

    /// Calculate new width, height
    // 5pt height margin, 0pt width margin (it is buggy to add width margin)
    const heightMargin = this.isContentPreview ? (6 * scale) : (5 * scale);
    const widthMargin = 0;
    const newWidth = maxWidth + 2 * widthMargin;

    /// Apply new pages
    let accumulatedHeight = 0;
    const firstPage = (nextPages.length ? nextPages[0] : undefined)!;
    let firstRect: SVGRectElement = undefined!;

    for (let i = 0; i < nextPages.length; i++) {
      /// Retrieve page width, height
      const nextPage = nextPages[i];
      const { width: pageWidth, height: pageHeight } = nextPage;

      /// center the page and add margin
      const calculatedPaddedX = (newWidth - pageWidth) / 2;
      const calculatedPaddedY = accumulatedHeight + (i == 0 ? 0 : heightMargin);
      const translateAttr = `translate(${calculatedPaddedX}, ${calculatedPaddedY})`;

      /// Create inner rectangle
      const innerRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      innerRect.setAttribute("class", "typst-page-inner");
      innerRect.setAttribute("data-page-width", pageWidth.toString());
      innerRect.setAttribute("data-page-height", pageHeight.toString());
      innerRect.setAttribute("width", Math.floor((pageWidth * INNER_RECT_UNIT)).toString());
      innerRect.setAttribute("height", Math.floor((pageHeight * INNER_RECT_UNIT)).toString());
      innerRect.setAttribute("x", "0");
      innerRect.setAttribute("y", "0");
      innerRect.setAttribute("transform", `${translateAttr} ${INNER_RECT_SCALE}`);
      // white background
      innerRect.setAttribute("fill", "white");
      // It is quite ugly
      // innerRect.setAttribute("stroke", "black");
      // innerRect.setAttribute("stroke-width", (2 * INNER_RECT_UNIT * scale).toString());
      // innerRect.setAttribute("stroke-opacity", "0.4");

      /// Move page to the correct position
      nextPage.elem.setAttribute("transform", translateAttr);

      /// Insert rectangles
      // todo: this is buggy not preserving order?
      svg.insertBefore(innerRect, firstPage.elem);
      if (!firstRect) {
        firstRect = innerRect;
      }

      let pageHeightEnd = pageHeight + (i + 1 === nextPages.length ? 0 : heightMargin);

      if (this.isContentPreview) {
        // --typst-preview-toolbar-fg-color
        // create page number indicator
        // console.log('create page number indicator', scale);
        const pageNumberIndicator = document.createElementNS("http://www.w3.org/2000/svg", "text");
        pageNumberIndicator.setAttribute("class", "typst-preview-svg-page-number");
        pageNumberIndicator.setAttribute("x", "0");
        pageNumberIndicator.setAttribute("y", "0");
        const pnPaddedX = calculatedPaddedX + pageWidth / 2;
        const pnPaddedY = calculatedPaddedY + pageHeight + heightMargin + fontSize / 2;
        pageNumberIndicator.setAttribute("transform", `translate(${pnPaddedX}, ${pnPaddedY})`);
        pageNumberIndicator.setAttribute("font-size", fontSize.toString());
        pageNumberIndicator.textContent = `${i + 1}`;
        svg.append(pageNumberIndicator);

        pageHeightEnd += fontSize;

      } else {
        if (this.cursorPosition && this.cursorPosition[0] === i + 1) {
          const [_, x, y] = this.cursorPosition;
          const cursor = document.createElementNS("http://www.w3.org/2000/svg", "circle");
          cursor.setAttribute("cx", (x * INNER_RECT_UNIT).toString());
          cursor.setAttribute("cy", (y * INNER_RECT_UNIT).toString());
          cursor.setAttribute("r", (5 * scale * INNER_RECT_UNIT).toString());
          cursor.setAttribute("fill", "#86C166CC");
          cursor.setAttribute("transform", `${translateAttr} ${INNER_RECT_SCALE}`);
          svg.appendChild(cursor);
        }
      }

      accumulatedHeight = calculatedPaddedY + pageHeightEnd;
    }

    if (this.isContentPreview) {
      accumulatedHeight += fontSize; // always add a bottom margin for last page number
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
      outerRect.setAttribute("fill", this.backgroundColor);
      svg.insertBefore(outerRect, firstRect);
    }

    /// Update svg width, height information
    svg.setAttribute("viewBox", `0 0 ${newWidth} ${newHeight}`);
    svg.setAttribute("width", `${Math.ceil(newWidth)}`);
    svg.setAttribute("height", `${Math.ceil(newHeight)}`);
    svg.setAttribute("data-width", `${newWidth}`);
    svg.setAttribute("data-height", `${newHeight}`);
  }

  private get docWidth() {
    const svg = this.hookedElem.firstElementChild!;

    if (svg) {
      let svgWidth = Number.parseFloat(
        svg.getAttribute("data-width")! || svg.getAttribute("width")! || "1"
      );
      if (svgWidth < 1e-5) {
        svgWidth = 1
      }
      return svgWidth;
    }

    return this.kModule.docWidth;
  }


  private statSvgFromDom() {
    const { width: containerWidth, boundingRect: containerBRect } = this.cachedDOMState;
    // scale derived from svg width and container with.
    // svg.setAttribute("data-width", `${newWidth}`);

    const computedRevScale = containerWidth ? this.docWidth / containerWidth : 1;
    // respect current scale ratio
    const revScale = computedRevScale / this.currentScaleRatio;
    const left = (window.screenLeft - containerBRect.left) * revScale;
    const top = (window.screenTop - containerBRect.top) * revScale;
    const width = window.innerWidth * revScale;
    const height = window.innerHeight * revScale;

    return { revScale, left, top, width, height };
  }

  private toggleViewportChange() {
    switch (this.renderMode) {
      case RenderMode.Svg: {
        return this.toggleSvgViewportChange();
      }
      case RenderMode.Canvas: {
        return this.toggleCanvasViewportChange();
      }
      default: {
        throw new Error(`unknown render mode ${this.renderMode}`);
      }
    }
  }

  private async toggleCanvasViewportChange() {
    // console.log('toggleCanvasViewportChange!!!!!!', this.id, this.isRendering);
    const pagesInfo: CanvasPage[] = this.kModule.retrievePagesInfo().map((x, index) => {
      return {
        tag: 'canvas',
        index,
        width: x.width,
        height: x.height,
        container: undefined as any as HTMLDivElement,
        elem: undefined as any as HTMLDivElement,
      }
    });

    if (!this.hookedElem.firstElementChild) {
      this.hookedElem.innerHTML = `<div class="typst-doc" data-render-mode="canvas"></div>`;
    }
    const docDiv = this.hookedElem.firstElementChild! as HTMLDivElement;

    if (this.isMixinOutline && this.outline) {
      console.log('render with outline', this.outline);
      patchOutlineEntry(docDiv as any, pagesInfo, this.outline.items);
      for (const ch of docDiv.children) {
        if (!ch.classList.contains('typst-page')) {
          continue;
        }
        const pageNumber = Number.parseInt(ch.getAttribute('data-page-number')!);
        if (pageNumber >= pagesInfo.length) {
          // todo: cache key shifted
          docDiv.removeChild(ch);
          continue;
        }
        pagesInfo[pageNumber].container = ch as HTMLDivElement;
        pagesInfo[pageNumber].elem = ch.firstElementChild as HTMLDivElement;
      }
    } else {
      for (const ch of docDiv.children) {
        if (!ch.classList.contains('typst-page')) {
          continue;
        }
        const pageNumber = Number.parseInt(ch.getAttribute('data-page-number')!);
        if (pageNumber >= pagesInfo.length) {
          // todo: cache key shifted
          docDiv.removeChild(ch);
          continue;
        }
        pagesInfo[pageNumber].container = ch as HTMLDivElement;
        pagesInfo[pageNumber].elem = ch.firstElementChild as HTMLDivElement;
      }
    }

    for (const pageInfo of pagesInfo) {
      if (!pageInfo.elem) {
        pageInfo.elem = document.createElement('div');
        pageInfo.elem.setAttribute('class', 'typst-page-canvas');
        pageInfo.elem.style.transformOrigin = '0 0';
        pageInfo.elem.style.transform = `scale(${1 / this.pixelPerPt})`;
        pageInfo.elem.setAttribute('data-page-number', pageInfo.index.toString());

        const canvas = document.createElement('canvas');
        canvas.width = pageInfo.width * this.pixelPerPt;
        canvas.height = pageInfo.height * this.pixelPerPt;
        pageInfo.elem.appendChild(canvas);

        pageInfo.container = document.createElement('div');
        // todo: reuse by key
        pageInfo.container.setAttribute(TypstPatchAttrs.Tid, `canvas:` + pageInfo.index);
        pageInfo.container.setAttribute('class', 'typst-page canvas-mode');
        pageInfo.container.setAttribute('data-page-number', pageInfo.index.toString());
        pageInfo.container.appendChild(pageInfo.elem);

        if (this.isContentPreview) {
          const pageNumberIndicator = document.createElement('div');
          pageNumberIndicator.setAttribute('class', 'typst-preview-canvas-page-number');
          pageNumberIndicator.textContent = `${pageInfo.index + 1}`;
          pageInfo.container.appendChild(pageNumberIndicator);

          pageInfo.container.style.cursor = 'pointer';
          pageInfo.container.style.pointerEvents = 'visible';
          pageInfo.container.style.overflow = 'hidden';
          pageInfo.container.addEventListener('click', () => {
            // console.log('click', pageInfo.index);
            window.typstWebsocket.send(`outline-sync,${pageInfo.index + 1}`);
          });
        }
      }

      if (!pageInfo.container.parentElement) {
        if (pageInfo.inserter) {
          pageInfo.inserter(pageInfo);
        } else {
          if (pageInfo.index === 0) {
            docDiv.prepend(pageInfo.container);
          } else {
            pagesInfo[pageInfo.index - 1].container.after(pageInfo.container)
          }
        }
      }
    }

    const t2 = performance.now();

    // todo: priority in window
    // await Promise.all(pagesInfo.map(async (pageInfo) => {
    this.kModule.backgroundColor = '#ffffff';
    this.kModule.pixelPerPt = this.pixelPerPt;
    if (docDiv.getAttribute('data-rendering') === 'true') {
      throw new Error('rendering in progress, possibly a race condition');
    }
    docDiv.setAttribute('data-rendering', 'true');
    for (const pageInfo of pagesInfo) {
      const canvas = pageInfo.elem.firstElementChild as HTMLCanvasElement;
      // const tt1 = performance.now();

      const pw = (pageInfo.width);
      const ph = (pageInfo.height);
      const pws = pageInfo.width.toFixed(3);
      const phs = pageInfo.height.toFixed(3);

      let cached = true;

      if (pageInfo.elem.getAttribute('data-page-width') !== pws) {
        pageInfo.elem.setAttribute('data-page-width', pws);
        cached = false;
        canvas.width = pw * this.pixelPerPt;
      }

      if (pageInfo.elem.getAttribute('data-page-height') !== phs) {
        pageInfo.elem.setAttribute('data-page-height', phs);
        cached = false;
        canvas.height = ph * this.pixelPerPt;
      }

      const cacheKey = pageInfo.elem.getAttribute('data-cache-key') || undefined;
      const result = await this.kModule.renderCanvas({
        canvas: canvas.getContext('2d')!,
        pageOffset: pageInfo.index,
        cacheKey: cached ? cacheKey : undefined,
        dataSelection: {
          body: true,
        }
      });
      if (cacheKey !== result.cacheKey) {
        // console.log('renderCanvas', pageInfo.index, performance.now() - tt1, result);
        // todo: cache key changed
        // canvas.width = pageInfo.width * this.pixelPerPt;
        // canvas.height = pageInfo.height * this.pixelPerPt;
        pageInfo.elem.setAttribute('data-page-width', pws);
        pageInfo.elem.setAttribute('data-page-height', phs);
        canvas.setAttribute('data-cache-key', result.cacheKey);
        pageInfo.elem.setAttribute('data-cache-key', result.cacheKey);
      }
    }
    docDiv.removeAttribute('data-rendering');

    // }));

    const t3 = performance.now();

    return [t2, t3];
  }

  private async toggleSvgViewportChange() {
    let patchStr: string;
    const mode = this.previewMode;
    if (mode === PreviewMode.Doc) {
      patchStr = this.fetchSvgDataByDocMode();
    } else if (mode === PreviewMode.Slide) {
      patchStr = this.fetchSvgDataBySlideMode();
    } else {
      throw new Error(`unknown preview mode ${mode}`);
    }

    const t2 = performance.now();
    await patchSvgToContainer(this.hookedElem, patchStr, elem => this.decorateSvgElement(elem, mode));
    const t3 = performance.now();

    return [t2, t3];
  }

  private fetchSvgDataBySlideMode() {
    const pagesInfo = this.kModule.retrievePagesInfo();

    if (pagesInfo.length === 0) {
      // svg warning
      return `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
  <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-size="20">No page found</text>
</svg>`;
    }

    if (this.partialRenderPage >= pagesInfo.length) {
      this.partialRenderPage = pagesInfo.length - 1;
    }

    const pageOffset = this.partialRenderPage;
    let lo = { x: 0, y: 0 },
      hi = { x: 0, y: 0 };
    for (let i = 0; i < pageOffset; i++) {
      const pageInfo = pagesInfo[i];
      lo.y += pageInfo.height;
    }
    const page = pagesInfo[pageOffset];
    hi.y = lo.y + page.height;
    hi.x = page.width;

    console.log('render_in_window for slide mode', lo.x, lo.y, hi.x, hi.y);

    // with a bit padding to avoid edge error
    lo.x += 1e-1;
    lo.y += 1e-1;
    hi.x -= 1e-1;
    hi.y -= 1e-1;

    return this.kModule.renderSvgDiff({
      window: {
        lo,
        hi,
      },
    });
  }

  private fetchSvgDataByDocMode() {
    const { revScale, left, top, width, height } = this.statSvgFromDom();

    let patchStr: string;
    // with 1px padding to avoid edge error
    if (this.partialRendering) {
      console.log(
        'render_in_window with partial rendering enabled',
        revScale,
        left,
        top,
        width,
        height,
      );

      /// Adjust top and bottom
      const ch = this.hookedElem.firstElementChild?.children;
      let topEstimate = top - height - 1,
        bottomEstimate = top + height * 2 + 1;
      if (ch) {
        const pages = Array.from(ch).filter(x => x.classList.contains('typst-page'));
        let minTop = 1e33,
          maxBottom = -1e33,
          accumulatedHeight = 0;
        const translateRegex = /translate\(([-0-9.]+), ([-0-9.]+)\)/;
        for (const page of pages) {
          const pageHeight = Number.parseFloat(page.getAttribute('data-page-height')!);
          const translate = page.getAttribute('transform')!;
          const translateMatch = translate.match(translateRegex)!;
          const translateY = Number.parseFloat(translateMatch[2]);
          if (translateY + pageHeight > topEstimate) {
            minTop = Math.min(minTop, accumulatedHeight);
          }
          if (translateY < bottomEstimate) {
            maxBottom = Math.max(maxBottom, accumulatedHeight + pageHeight);
          }
          accumulatedHeight += pageHeight;
        }

        if (pages.length != 0) {
          topEstimate = minTop;
          bottomEstimate = maxBottom;
        } else {
          topEstimate = 0;
          bottomEstimate = 1e33;
        }
      }
      // translate
      patchStr = this.kModule.render_in_window(
        // lo.x, lo.y
        left - 1,
        topEstimate,
        // hi.x, hi.y
        left + width + 1,
        bottomEstimate,
      );
    } else {
      console.log('render_in_window with partial rendering disabled', 0, 0, 1e33, 1e33);
      patchStr = this.kModule.render_in_window(0, 0, 1e33, 1e33);
    }

    return patchStr;
  }

  private async processQueue(svgUpdateEvent: [string, string]) {
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
        const [_t2, _t3] = await this.toggleViewportChange();
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
        const [_t2, _t3] = await this.toggleViewportChange();
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
    this.sampledRenderTime = t3 - t0;
    console.log(
      [
        d("parse", t0, t1),
        d("check-diff", t1, t2),
        d("patch", t2, t3),
        d("total", t0, t3),
      ].join(", ")
    );
  }

  private triggerUpdate() {
    if (this.isRendering) {
      return;
    }

    this.isRendering = true;
    const doUpdate = async () => {
      this.cachedDOMState = this.retrieveDOMState();

      if (this.patchQueue.length === 0) {
        this.isRendering = false;
        this.postprocessChanges();
        return;
      }
      try {
        // console.log('patchQueue', JSON.stringify(this.patchQueue.map(x => x[0])));
        while (this.patchQueue.length > 0) {
          await this.processQueue(this.patchQueue.shift()!);
          this.rescale();
        }

        requestAnimationFrame(doUpdate);
      } catch (e) {
        console.error(e);
        this.isRendering = false;
        this.postprocessChanges();
      }
    };
    requestAnimationFrame(doUpdate);
  }

  private postprocessChanges() {
    switch (this.renderMode) {
      case RenderMode.Svg: {
        const docRoot = this.hookedElem.firstElementChild as SVGElement;
        if (docRoot) {
          window.initTypstSvg(docRoot);
          this.rescale();
        }
        break;
      }
      case RenderMode.Canvas: { break; }
      default: {
        throw new Error(`unknown render mode ${this.renderMode}`);
      }
    }

    // todo: abstract this
    if (this.previewMode === PreviewMode.Slide) {
      document.querySelectorAll('.typst-page-number-indicator').forEach(x => {
        x.textContent = `${this.kModule.retrievePagesInfo().length}`;
      });
    }
  }

  addChangement(change: [string, string]) {
    if (change[0] === "new") {
      this.patchQueue.splice(0, this.patchQueue.length);
    }

    const pushChange = () => {
      this.vpTimeout = undefined;
      this.patchQueue.push(change);
      this.triggerUpdate();
    };

    if (this.vpTimeout !== undefined) {
      clearTimeout(this.vpTimeout);
    }

    if (change[0] === "viewport-change" && this.isRendering) {
      // delay viewport change a bit
      this.vpTimeout = setTimeout(pushChange, this.sampledRenderTime || 100);
    } else {
      pushChange();
    }
  }

  addViewportChange() {
    this.addChangement(["viewport-change", ""]);
  }

  dispose() {
    if (this.hookedElem) {
      removeSourceMappingHandler(this.hookedElem);
    }
    this.disposeList.forEach(x => x());
  }
}

interface Options {
  renderMode?: RenderMode,
  previewMode?: PreviewMode,
  isContentPreview?: boolean,
  retrieveDOMState?: () => ContainerDOMState,
}

export class TypstDocument {
  private impl: TypstDocumentImpl;

  constructor(hookedElem: HTMLElement, public kModule: RenderSession, options?: Options) {
    this.impl = new TypstDocumentImpl(hookedElem, kModule, options);
  }

  dispose() {
    this.impl.dispose();
  }

  reset() {
    this.impl.reset();
  }

  addChangement(change: [string, string]) {
    this.impl.addChangement(change);
  }

  addViewportChange() {
    this.impl.addViewportChange();
  }

  setPartialRendering(partialRendering: boolean) {
    this.impl.partialRendering = partialRendering;
  }

  setCursor(page: number, x: number, y: number) {
    this.impl.cursorPosition = [page, x, y];
  }

  setPartialPageNumber(page: number): boolean {
    if (page <= 0 || page > this.kModule.retrievePagesInfo().length) {
      return false;
    }
    this.impl.partialRenderPage = page - 1;
    this.addViewportChange();
    return true;
  }

  getPartialPageNumber(): number {
    return this.impl.partialRenderPage + 1;
  }

  setOutineData(outline: any) {
    this.impl.outline = outline;
    this.addViewportChange();
  }
}
