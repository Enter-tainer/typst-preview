import "./typst.css";
import "./styles/toolbar.css";
import "./styles/layout.css";
import "./styles/help-panel.css";

import { wsMain } from './ws';
import { PreviewMode } from './svg-doc';

let previewModePlaceholder = 'preview-arg:previewMode:Doc';
previewModePlaceholder = previewModePlaceholder.replace('preview-arg:previewMode:', '');
let previewMode = PreviewMode[previewModePlaceholder];

let previousDispose = Promise.resolve(() => {});
window.onload = () => nextWs({
    url: "ws://127.0.0.1:23625", 
    previewMode, 
    isContentPreview: false,
});

const vscodeAPI = (typeof acquireVsCodeApi !== 'undefined') && acquireVsCodeApi();
if (vscodeAPI?.postMessage) {
    vscodeAPI.postMessage({ type: 'started' });
}

// Handle messages sent from the extension to the webview
window.addEventListener('message', event => {
    const message = event.data; // The json data that the extension sent
    switch (message.type) {
        case 'reconnect': {
            console.log('reconnect', message);
            nextWs({
                url:  message.url, 
                previewMode: PreviewMode[message.mode], 
                isContentPreview: message.isContentPreview,
            });
            break;
        }
    }
});

function nextWs(nextWsArgs) {
    const previous = previousDispose;
    previousDispose = new Promise(async (resolve) => {
        await previous.then(d => d());
        resetContainer(nextWsArgs);
        resolve(wsMain(nextWsArgs));
    });
}

function resetContainer({ previewMode: mode, isContentPreview }) {
    const app = document.getElementById('typst-container');
    app.classList.remove('mode-slide');
    app.classList.remove('mode-doc');
    app.classList.remove('content-preview');

   if (isContentPreview) {
        app.classList.add('content-preview');
   }
   
   if (mode === PreviewMode.Slide) {
        app.classList.add('mode-slide');
    } else if (mode === PreviewMode.Doc) {
        app.classList.add('mode-doc');
    } else {
        throw new Error(`Unknown preview mode: ${mode}`);
    }
}
