import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

export default defineConfig({
  clearScreen: false,
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@shiro/ui-shared": resolve(import.meta.dirname, "../shared/src"),
      "@shiro/mobile-ui": resolve(import.meta.dirname, "./src"),
    },
  },
  server: {
    host: "0.0.0.0",
    port: 1421,
    strictPort: true,
  },
})
