import { createReadStream, readFileSync, realpathSync, statSync } from "node:fs"
import type { IncomingMessage, ServerResponse } from "node:http"
import path from "node:path"

import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { type Plugin, searchForWorkspaceRoot } from "vite"
import { defineConfig } from "vitest/config"

// The reader loads the published contract schema from carddb (one source of truth, no copy).
const contractSchemaDir = path.resolve(
  import.meta.dirname,
  "../../carddb/src/sve_carddb/contracts/schema",
)

const MIME: Record<string, string> = { ".json": "application/json", ".webp": "image/webp" }

// Serves a snapshot root (the layout the CDN will have) under a URL prefix during dev/preview.
function serveSnapshotRoot(prefix: string, dir: string | undefined): Plugin {
  const root = dir === undefined ? undefined : realpathSync(path.resolve(dir))
  const notFound = (res: ServerResponse) => {
    res.statusCode = 404
    res.end()
  }
  const handler = (req: IncomingMessage, res: ServerResponse, next: () => void) => {
    const url = req.url ?? ""
    if (!url.startsWith(`${prefix}/`)) {
      next()
      return
    }
    // An unconfigured root answers 404 like the CDN would, instead of the SPA fallback's HTML.
    if (root === undefined) {
      notFound(res)
      return
    }
    let relative: string
    try {
      relative = decodeURIComponent(url.slice(prefix.length + 1).split("?")[0] ?? "")
    } catch {
      notFound(res)
      return
    }
    if (
      relative.split("/").some((segment) => segment === "" || segment === "." || segment === "..")
    ) {
      notFound(res)
      return
    }
    // Resolve symlinks before the boundary check, so a link inside the root cannot reach outside it.
    let file: string
    try {
      file = realpathSync(path.join(root, relative))
    } catch {
      notFound(res)
      return
    }
    if (!file.startsWith(root + path.sep) || !statSync(file).isFile()) {
      notFound(res)
      return
    }
    res.setHeader("Content-Type", MIME[path.extname(file)] ?? "application/octet-stream")
    res.setHeader("Cache-Control", "no-store")
    createReadStream(file).pipe(res)
  }
  return {
    name: `sve-serve-${prefix.slice(1)}`,
    configureServer(server) {
      server.middlewares.use(handler)
    },
    configurePreviewServer(server) {
      server.middlewares.use(handler)
    },
  }
}

// Local snapshot roots: SVE_EXPORT_DIR (else the committed fixture) at /cdn, SVE_PREVIEW_DIR at /cdn-preview.
export function snapshotRoots(environment: NodeJS.ProcessEnv): { cdn: string; preview?: string } {
  const root = (name: string): string | undefined => {
    const value = environment[name]
    if (value === undefined || value === "") return undefined
    if (!path.isAbsolute(value)) {
      throw new Error(`${name} must be an absolute path when set`)
    }
    return value
  }
  const preview = root("SVE_PREVIEW_DIR")
  return {
    cdn: root("SVE_EXPORT_DIR") ?? path.join(import.meta.dirname, "fixtures/snapshot"),
    ...(preview === undefined ? {} : { preview }),
  }
}

const { cdn: cdnDir, preview: previewDir } = snapshotRoots(process.env)

export default defineConfig({
  resolve: {
    alias: {
      "#snapshot-validators": path.resolve(
        import.meta.dirname,
        "node_modules/.cache/sve-schema/validators.js",
      ),
      "#snapshot-conformance": path.resolve(
        import.meta.dirname,
        "node_modules/.cache/sve-schema/conformance.cjs",
      ),
    },
  },
  build: { license: { fileName: "third-party-licenses.md" } },
  // Lets the dev badge hide its root switch when no preview root is configured.
  define: {
    "import.meta.env.SVE_PREVIEW_CONFIGURED": JSON.stringify(previewDir === undefined ? "" : "1"),
  },
  plugins: [
    {
      name: "snapshot-validator-license",
      apply: "build",
      generateBundle() {
        // Helpers are already bundled into generated standalone code, so package discovery misses them.
        this.emitFile({
          type: "asset",
          fileName: "snapshot-validator-LICENSE.md",
          source: readFileSync(
            new URL("./node_modules/.cache/sve-schema/licenses.md", import.meta.url),
            "utf8",
          ),
        })
      },
    },
    react(),
    tailwindcss(),
    serveSnapshotRoot("/cdn", cdnDir),
    serveSnapshotRoot("/cdn-preview", previewDir),
  ],
  server: {
    fs: { allow: [searchForWorkspaceRoot(process.cwd()), contractSchemaDir] },
  },
  worker: {
    rollupOptions: {
      output: {
        entryFileNames: (chunk) =>
          chunk.name === "image-sw" ? "image-sw.js" : "assets/[name]-[hash].js",
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "tests/**/*.test.ts", "scripts/**/*.test.ts"],
  },
})
