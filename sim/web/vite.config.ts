import path from "node:path"

import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { searchForWorkspaceRoot } from "vite"
import { defineConfig } from "vitest/config"

// The reader loads the published contract schema from carddb (one source of truth, no copy).
const contractSchemaDir = path.resolve(
  import.meta.dirname,
  "../../carddb/src/sve_carddb/snapshot/schema",
)

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    fs: { allow: [searchForWorkspaceRoot(process.cwd()), contractSchemaDir] },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "tests/**/*.test.ts"],
  },
})
