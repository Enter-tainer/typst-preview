import { defineConfig } from "vite";

export default defineConfig({
  build: {
    minify: false,
    rollupOptions: {
      input: {
        app: "index.html",
        main: "src/main.js",
        svg: "src/svg.ts",
      },
      output: {
        assetFileNames: `typst-webview-assets/[name]-[hash][extname]`,
        chunkFileNames: "typst-webview-assets/[name]-[hash].js",
        entryFileNames: "typst-webview-assets/[name]-[hash].js",
      },
    },
  },
});
