import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": {
        target: process.env.ASYLUM_BASE_URL ?? "http://127.0.0.1:7788",
        changeOrigin: true,
        ws: true,
      },
    },
  },
  test: {
    environment: "jsdom",
    environmentOptions: {
      jsdom: {
        url: "http://localhost",
      },
    },
    setupFiles: ["./src/test-setup.ts"],
  },
});
