import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  plugins: [wasm()],
  base: "./",
  build: { target: "esnext", license: { fileName: "third-party-licenses-npm.md" } },
  optimizeDeps: { exclude: ["browser"] },
});
