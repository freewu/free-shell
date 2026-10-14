import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @see https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development.
  clearScreen: false,
  server: {
    // 1421 instead of Tauri's default 1420: two Tauri apps dev-served on the
    // same machine would otherwise fight over the port. Keep in sync with
    // `build.devUrl` in src-tauri/tauri.conf.json.
    port: 1421,
    strictPort: true,
    host: false,
    watch: {
      // tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
