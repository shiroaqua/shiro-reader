import js from "@eslint/js"
import globals from "globals"
import reactHooks from "eslint-plugin-react-hooks"
import reactRefresh from "eslint-plugin-react-refresh"
import tseslint from "typescript-eslint"
import { defineConfig, globalIgnores } from "eslint/config"

export default defineConfig([
  globalIgnores(["**/dist", "**/node_modules"]),
  {
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },
  {
    files: ["packages/shared/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            "@shiro/desktop-ui",
            "@shiro/desktop-ui/**",
            "@shiro/mobile-ui",
            "@shiro/mobile-ui/**",
            "**/desktop/**",
            "**/mobile/**",
          ],
        },
      ],
    },
  },
  {
    files: ["packages/desktop/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            "@shiro/mobile-ui",
            "@shiro/mobile-ui/**",
            "**/mobile/**",
          ],
        },
      ],
    },
  },
  {
    files: ["packages/mobile/**/*.{ts,tsx}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            "@shiro/desktop-ui",
            "@shiro/desktop-ui/**",
            "**/desktop/**",
          ],
        },
      ],
    },
  },
])
