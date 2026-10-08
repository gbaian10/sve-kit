import { mkdir, readFile, writeFile } from "node:fs/promises"
import path from "node:path"

import { build } from "vite"

import { compileSchemas } from "./compile"

const webRoot = path.resolve(import.meta.dirname, "../..")
const schemaDir = path.resolve(webRoot, "../../carddb/src/sve_carddb/contracts/schema")
const output = path.join(webRoot, "node_modules/.cache/sve-schema")
const roots: Record<string, object> = {}
for (const version of ["v2"]) {
  roots[version] = JSON.parse(
    await readFile(path.join(schemaDir, version, "contract.schema.json"), "utf8"),
  ) as object
}
await mkdir(output, { recursive: true })
await writeFile(path.join(output, "conformance.cjs"), compileSchemas(roots))
await writeFile(
  path.join(output, "standalone.cjs"),
  compileSchemas(
    roots,
    new Set(["Manifest", "Container", "Config", "Programs", "TextAll", "Index", "printing_image"]),
  ),
)
await writeFile(path.join(output, "entry.js"), 'export { default } from "./standalone.cjs"\n')
// The library build resolves Ajv's CommonJS runtime helpers into CSP-safe ESM as well.
await build({
  configFile: false,
  root: webRoot,
  logLevel: "warn",
  build: {
    target: "es2024",
    outDir: output,
    emptyOutDir: false,
    copyPublicDir: false,
    minify: true,
    license: { fileName: "licenses.md" },
    lib: { entry: path.join(output, "entry.js"), formats: ["es"], fileName: () => "validators.js" },
  },
})
