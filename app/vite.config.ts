import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import Icons from "unplugin-icons/vite";

// Tauri expects a fixed dev port and serves the built `dist/` in production.
// Icons come from the MingCute set (Apache-2.0) and are compiled into the
// bundle one by one, so only the icons actually imported ship.
export default defineConfig({
  plugins: [react(), tailwindcss(), Icons({ compiler: "jsx", jsx: "react" })],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: "es2022", sourcemap: false },
  test: { environment: "jsdom", globals: false },
});
