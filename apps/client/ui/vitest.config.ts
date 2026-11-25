import { resolve } from "node:path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vitest/config"

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@shiro/ui-shared": resolve(import.meta.dirname, "./packages/shared/src"),
      "@shiro/desktop-ui": resolve(
        import.meta.dirname,
        "./packages/desktop/src"
      ),
      "@shiro/mobile-ui": resolve(import.meta.dirname, "./packages/mobile/src"),
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
    include: ["tests/**/*.test.{ts,tsx}"],
    clearMocks: true,
    restoreMocks: true,
  },
})
