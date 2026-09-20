import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1423, strictPort: true },
  build: {
    outDir: "dist-react",
    rolldownOptions: { input: "react.html" },
  },
});
