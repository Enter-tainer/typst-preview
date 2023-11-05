import { wsMain } from './ws';
import { PreviewMode } from './svg-doc';

let previewModePlaceholder = 'preview-arg:previewMode:Doc';
previewModePlaceholder = previewModePlaceholder.replace('preview-arg:previewMode:', '');
let previewMode = PreviewMode[previewModePlaceholder];

window.onload = () => wsMain(previewMode);

const app = document.getElementById('typst-container');
if (previewMode === PreviewMode.Slide) {
    app.classList.add('mode-slide');
} else if (previewMode === PreviewMode.Doc) {
    app.classList.add('mode-doc');
} else {
    throw new Error(`Unknown preview mode: ${previewMode}`);
}
