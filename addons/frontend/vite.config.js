import { defineConfig } from "vite";

export default defineConfig({
  build: {
    minify: false,
    rollupOptions: {
      output: {
        assetFileNames: `typst-webview-assets/[name]-[hash][extname]`,
        chunkFileNames: "typst-webview-assets/[name]-[hash].js",
        entryFileNames: "typst-webview-assets/[name]-[hash].js",
      },
    },
  },
});
