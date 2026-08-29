import { defineConfig, searchForWorkspaceRoot } from "vite";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  plugins: [wasm()],
  base: "./",
  build: { target: "esnext" },
  optimizeDeps: { exclude: ["browser/playground"] },
  server: {
    fs: { allow: [searchForWorkspaceRoot(process.cwd())] },
  },
});
