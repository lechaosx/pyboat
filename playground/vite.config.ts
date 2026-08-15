import { defineConfig, searchForWorkspaceRoot } from "vite";

export default defineConfig({
  base: "./",
  build: { target: "esnext" },
  server: {
    fs: { allow: [searchForWorkspaceRoot(process.cwd())] },
  },
});
