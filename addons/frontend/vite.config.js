import { defineConfig } from "vite";

export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        app: "index.html",
        main: "src/main.js",
        svg: "src/svg.ts",
      },
    },
  },
});
