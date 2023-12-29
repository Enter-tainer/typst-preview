interface Window {
  initTypstSvg(docRoot: SVGElement): void;
  handleTypstLocation(elem: Element, page: number, x: number, y: number);
  rasterizeTasks: any;
  handleTextRasterized: any;
  async postTextRasterization(root: Element): Promise<void>,
  typstWebsocket: WebSocket;
}
const acquireVsCodeApi: any;
