/// window.initTypstSvg = function (docRoot: SVGElement) {

interface Window {
  initTypstSvg(docRoot: SVGElement): void;
  handleTypstLocation(elem: Element, page: number, x: number, y: number);
}
