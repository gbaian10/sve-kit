// @vitest-environment node
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"
import { runInNewContext } from "node:vm"

import { build } from "vite"
import { expect, it } from "vitest"

import { BUILD_ID } from "../src/data/build-id"
import { buildIdentity } from "../vite.config"

it("changes with shipped sources and locked inputs, not with tests or missing files", () => {
  const root = mkdtempSync(path.join(tmpdir(), "build-id-"))
  const src = path.join(root, "src")
  const lock = path.join(root, "bun.lock")
  mkdirSync(path.join(src, "test-utils"), { recursive: true })
  writeFileSync(path.join(src, "engine.ts"), "export const a = 1\n")
  writeFileSync(lock, "lock 1\n")
  const inputs = [src, lock, path.join(root, "missing.js")]
  const initial = buildIdentity(inputs, root)
  expect(initial).toMatch(/^[0-9a-f]{16}$/)
  writeFileSync(path.join(src, "engine.test.ts"), "test\n")
  writeFileSync(path.join(src, "test-utils", "cache.ts"), "helper\n")
  writeFileSync(path.join(src, "test-setup.ts"), "setup\n")
  expect(buildIdentity(inputs, root)).toBe(initial)
  writeFileSync(path.join(src, "engine.ts"), "export const a = 2\n")
  const edited = buildIdentity(inputs, root)
  expect(edited).not.toBe(initial)
  writeFileSync(lock, "lock 2\n")
  expect(buildIdentity(inputs, root)).not.toBe(edited)
  rmSync(root, { recursive: true, force: true })
})

it("injects the same identity into an executable production search Worker", async () => {
  const root = mkdtempSync(path.join(tmpdir(), "worker-build-id-"))
  const entry = path.join(root, "main.ts")
  writeFileSync(entry, 'new Worker(new URL("./identity-worker.ts", import.meta.url))\n')
  writeFileSync(
    path.join(root, "identity-worker.ts"),
    `import ${JSON.stringify(path.resolve(import.meta.dirname, "../src/data/search/worker.ts"))}\n` +
      `import { BUILD_ID } from ${JSON.stringify(path.resolve(import.meta.dirname, "../src/data/build-id.ts"))}\n` +
      "self.postMessage(BUILD_ID)\n",
  )
  try {
    const result = await build({
      configFile: path.resolve(import.meta.dirname, "../vite.config.ts"),
      logLevel: "silent",
      build: { write: false, minify: false, rollupOptions: { input: entry } },
    })
    if ("on" in result) throw new Error("unexpected watch build")
    const outputs = Array.isArray(result) ? result : [result]
    const worker = outputs
      .flatMap((output) => output.output)
      .find((file) => file.type === "asset" && /identity-worker.*\.js$/.test(file.fileName))
    if (!worker || worker.type !== "asset") throw new Error("missing worker bundle")
    const messages: unknown[] = []
    runInNewContext(String(worker.source), {
      TextDecoder,
      TextEncoder,
      self: {
        addEventListener: () => undefined,
        postMessage: (value: unknown) => messages.push(value),
      },
    })
    expect(messages).toEqual([BUILD_ID])
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}, 30_000)
