import { createHash } from "node:crypto"
import {
  createReadStream,
  existsSync,
  readdirSync,
  readFileSync,
  realpathSync,
  statSync,
} from "node:fs"
import { readFile } from "node:fs/promises"
import type { IncomingMessage, ServerResponse } from "node:http"
import path from "node:path"
import { promisify } from "node:util"
import { brotliCompress, constants as zlib } from "node:zlib"

import { codecovVitePlugin } from "@codecov/vite-plugin"
import tailwindcss from "@tailwindcss/vite"
import basicSsl from "@vitejs/plugin-basic-ssl"
import react from "@vitejs/plugin-react"
import { type Plugin, searchForWorkspaceRoot } from "vite"
import { defineConfig } from "vitest/config"

// The reader loads the published contract schema from carddb (one source of truth, no copy).
const contractSchemaDir = path.resolve(
  import.meta.dirname,
  "../../carddb/src/sve_carddb/contracts/schema",
)

const MIME: Record<string, string> = { ".json": "application/json", ".webp": "image/webp" }

// Content-addressed files and versioned image URLs never change at the same URL, like on the CDN;
// pointers such as snapshots/preview/current.json must always be refetched.
export function snapshotCacheControl(relative: string, search: string): string {
  const immutable =
    /^snapshots\/(blobs|manifests)\/[0-9a-f]{64}\.json$/.test(relative) ||
    (relative.startsWith("images/") && /^\?v=[1-9][0-9]*$/.test(search))
  return immutable ? "public, max-age=31536000, immutable" : "no-store"
}

export function snapshotEncoding(accept: string | undefined): "br" | undefined {
  // q=0 means the client refuses that encoding.
  const offered = new Set(
    (accept ?? "")
      .split(",")
      .map((token) => token.trim().split(/\s*;\s*/))
      .filter(([, ...params]) => !params.some((param) => /^q=0(\.0{0,3})?$/.test(param)))
      .map(([name]) => name),
  )
  if (offered.has("br")) return "br"
  return undefined
}

// Quality 9 keeps the first dev load quick; the CDN's quality 11 is only slightly smaller.
const compressBrotli = (data: Buffer) =>
  promisify(brotliCompress)(data, { params: { [zlib.BROTLI_PARAM_QUALITY]: 9 } })
// Dev only: the CDN serves precompressed JSON, so compress each file once instead of per request.
const compressed = new Map<string, Promise<Buffer>>()

// Serves a snapshot root (the layout the CDN will have) under a URL prefix during dev/preview.
export function serveSnapshotRoot(prefix: string, dir: string | undefined): Plugin {
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
    const search = url.includes("?") ? url.slice(url.indexOf("?")) : ""
    res.setHeader("Content-Type", MIME[path.extname(file)] ?? "application/octet-stream")
    res.setHeader("Cache-Control", snapshotCacheControl(relative, search))
    const json = path.extname(file) === ".json"
    // Every JSON representation depends on Accept-Encoding, including the uncompressed one.
    if (json) res.setHeader("Vary", "Accept-Encoding")
    const encoding = json ? snapshotEncoding(req.headers["accept-encoding"]) : undefined
    if (encoding === undefined) {
      createReadStream(file).pipe(res)
      return
    }
    const key = `${file}\0${String(statSync(file).mtimeMs)}`
    let body = compressed.get(key)
    if (body === undefined) {
      body = readFile(file).then(compressBrotli)
      compressed.set(key, body)
    }
    body.then(
      (data) => {
        res.setHeader("Content-Encoding", encoding)
        res.setHeader("Content-Length", String(data.length))
        res.end(data)
      },
      () => {
        compressed.delete(key)
        res.statusCode = 500
        res.end()
      },
    )
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

export const FIXTURE_ROOT = path.join(import.meta.dirname, "fixtures/snapshot")

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
    cdn: root("SVE_EXPORT_DIR") ?? FIXTURE_ROOT,
    ...(preview === undefined ? {} : { preview }),
  }
}

// Tests never ship, so editing one must not discard results cached by the browser.
const isTestFile = (file: string) =>
  /\.test\.[cm]?[jt]sx?$/.test(file) ||
  file.split(path.sep).includes("test-utils") ||
  path.basename(file) === "test-setup.ts"

/** Content hash of every non-test file under the given files and directories; missing ones are skipped. */
export function buildIdentity(inputs: readonly string[], base: string): string {
  const files = inputs.flatMap((input) => {
    if (!existsSync(input)) return []
    if (!statSync(input).isDirectory()) return [input]
    return readdirSync(input, { recursive: true, withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => path.join(entry.parentPath, entry.name))
  })
  const hash = createHash("sha256")
  for (const file of files.filter((file) => !isTestFile(file)).sort()) {
    hash.update(path.relative(base, file)).update("\0").update(readFileSync(file)).update("\0")
  }
  return hash.digest("hex").slice(0, 16)
}

const BUILD_ID_MODULE = path.join(import.meta.dirname, "src/data/build-id.ts")
const BUILD_INPUTS = [
  path.join(import.meta.dirname, "src"),
  contractSchemaDir,
  ...[
    "bun.lock",
    "package.json",
    "vite.config.ts",
    "tsconfig.json",
    "tsconfig.base.json",
    "tsconfig.app.json",
    "tsconfig.node.json",
  ].map((file) => path.join(import.meta.dirname, file)),
]

// Cached decoded snapshots are valid only for the code that verified them. A define would be fixed
// for the whole dev session, so dev recomputes the hash on each edit and reloads this one module.
function buildIdPlugin(inputs: readonly string[]): Plugin {
  const compute = () => buildIdentity(inputs, import.meta.dirname)
  let id = compute()
  const tracked = (file: string) =>
    !isTestFile(file) && inputs.some((input) => file === input || file.startsWith(input + path.sep))
  return {
    name: "sve-build-id",
    enforce: "pre",
    transform(code, moduleId) {
      const [file, query = ""] = moduleId.split("?")
      // `?raw` and `?url` imports read the file itself (the i18n usage test does); only the module
      // that runs gets the value. HMR's `?t=` reloads still need it.
      if (file !== BUILD_ID_MODULE || /(?:^|&)(?:raw|url)(?:[=&]|$)/.test(query)) return undefined
      return { code: code.replace("= __SVE_BUILD_ID__", `= ${JSON.stringify(id)}`), map: null }
    },
    configureServer(server) {
      server.watcher.add([...inputs])
      server.watcher.on("all", (_event, file) => {
        if (!tracked(file)) return
        id = compute()
        for (const environment of Object.values(server.environments))
          for (const stale of environment.moduleGraph.getModulesByFile(BUILD_ID_MODULE) ?? [])
            environment.moduleGraph.invalidateModule(stale)
      })
    },
  }
}

const THEME_BOOT_TAG = '<script src="/theme-boot.js"></script>'

// The theme script blocks the first paint, and a fixed file name is revalidated on every load; inline
// it so a reload does not wait one round trip for it. public/theme-boot.js stays the tested source.
export function inlineThemeBoot(): Plugin {
  return {
    name: "sve-inline-theme-boot",
    transformIndexHtml(html) {
      if (!html.includes(THEME_BOOT_TAG))
        throw new Error("index.html no longer loads theme-boot.js")
      const source = readFileSync(path.join(import.meta.dirname, "public/theme-boot.js"), "utf8")
      return html.replace(THEME_BOOT_TAG, () => `<script>${source}</script>`)
    },
  }
}

const { cdn: cdnDir, preview: previewDir } = snapshotRoots(process.env)
const devHttps = process.env["SVE_DEV_HTTPS"] === "1"
const tlsCertPath = process.env["SVE_DEV_TLS_CERT"]
const tlsKeyPath = process.env["SVE_DEV_TLS_KEY"]
const codecovToken = process.env["CODECOV_TOKEN"]
let tlsCertificate: { cert: Buffer; key: Buffer } | undefined

if (devHttps && (tlsCertPath !== undefined || tlsKeyPath !== undefined)) {
  if (tlsCertPath === undefined || tlsKeyPath === undefined) {
    throw new Error("SVE_DEV_TLS_CERT and SVE_DEV_TLS_KEY must be set together")
  }
  if (!path.isAbsolute(tlsCertPath) || !path.isAbsolute(tlsKeyPath)) {
    throw new Error("SVE_DEV_TLS_CERT and SVE_DEV_TLS_KEY must be absolute paths")
  }
  try {
    tlsCertificate = {
      cert: readFileSync(tlsCertPath),
      key: readFileSync(tlsKeyPath),
    }
  } catch (cause) {
    throw new Error("Unable to read SVE_DEV_TLS_CERT or SVE_DEV_TLS_KEY", { cause })
  }
}

export default defineConfig({
  build: { license: { fileName: "third-party-licenses.md" } },
  // Only these two flags reach browser code: dev starts on the preview root when a preview is
  // configured and /cdn is still the fixture (src/app/dev-root.ts).
  define: {
    "import.meta.env.SVE_PREVIEW_CONFIGURED": JSON.stringify(previewDir === undefined ? "" : "1"),
    "import.meta.env.SVE_CDN_IS_FIXTURE": JSON.stringify(cdnDir === FIXTURE_ROOT ? "1" : ""),
  },
  plugins: [
    buildIdPlugin(BUILD_INPUTS),
    inlineThemeBoot(),
    // LAN requests need a secure context for snapshot verification and offline caching.
    ...(devHttps && tlsCertificate === undefined ? [basicSsl()] : []),
    react(),
    tailwindcss(),
    serveSnapshotRoot("/cdn", cdnDir),
    serveSnapshotRoot("/cdn-preview", previewDir),
    // Only the CI bundle analysis step has a token, so local builds and deploys never write the stats file into dist/.
    // The plugin declares Vite 4-6 only; re-check it after a Vite or plugin upgrade.
    ...(codecovToken
      ? [
          codecovVitePlugin({
            enableBundleAnalysis: true,
            bundleName: "sve-web",
            uploadToken: codecovToken,
            telemetry: false,
          }),
        ]
      : []),
  ],
  server: {
    ...(tlsCertificate === undefined ? {} : { https: tlsCertificate }),
    fs: {
      allow: [
        searchForWorkspaceRoot(process.cwd()),
        contractSchemaDir,
        path.resolve(import.meta.dirname, "../../tests/fixtures"),
      ],
    },
  },
  preview: {
    ...(tlsCertificate === undefined ? {} : { https: tlsCertificate }),
  },
  worker: {
    plugins: () => [buildIdPlugin(BUILD_INPUTS)],
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
