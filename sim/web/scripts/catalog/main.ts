import { execFileSync, spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { cp, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import path from "node:path"
import { parseArgs } from "node:util"

import { chromium } from "playwright"
import { build, preview } from "vite"

import { CATALOG_OBSERVER, CatalogMonitor } from "./monitor"
import { realmGroup, type Report, type Run, summarize, table } from "./report"

const { values } = parseArgs({
  options: {
    old: { type: "string" },
    new: { type: "string" },
    out: { type: "string" },
    runs: { type: "string", default: "3" },
    entry: { type: "string", default: "index" },
    "cold-jp-runs": { type: "string" },
    "cold-en-runs": { type: "string" },
    "update-jp-runs": { type: "string" },
    "update-en-runs": { type: "string" },
    scenarios: { type: "string", default: "cold-jp,cold-en,update-jp,update-en" },
    "sample-ms": { type: "string", default: "50" },
    "settle-ms": { type: "string", default: "2000" },
    "cpu-quota": { type: "string", default: "200%" },
    "memory-max": { type: "string", default: "3G" },
    "quota-child": { type: "boolean", default: false },
    smoke: { type: "boolean", default: false },
    help: { type: "boolean", default: false },
  },
})
if (values.help) {
  process.stdout
    .write(`bun run bench:catalog --old /absolute/old-public --new /absolute/new-public --out /absolute/report.json
  --entry index|preview (default index)
  --runs 3; --cold-jp-runs / --cold-en-runs / --update-jp-runs / --update-en-runs N
  --scenarios cold-jp,cold-en,update-jp,update-en
  --cpu-quota 200% --memory-max 3G (systemd user scope, entire process tree)
  --sample-ms 50 --settle-ms 2000
  --smoke --runs 1 --out /absolute/smoke.json (synthetic data only)
`)
  process.exit(0)
}
const positive = (value: string, name: string) => {
  const number = Number(value)
  if (!Number.isSafeInteger(number) || number < 1)
    throw new Error(`${name} must be a positive integer`)
  return number
}
if (values.entry !== "index" && values.entry !== "preview")
  throw new Error("entry must be index or preview")
const sampleMs = positive(values["sample-ms"], "sample-ms")
const settleMs = positive(values["settle-ms"], "settle-ms")
const scenarios = values.scenarios.split(",")
const allowed = ["cold-jp", "cold-en", "update-jp", "update-en"] as const
for (const name of scenarios)
  if (!allowed.includes(name as (typeof allowed)[number]))
    throw new Error(`Unknown scenario: ${name}`)
const counts = new Map(
  scenarios.map((name) => [
    name,
    positive(values[`${name}-runs` as "cold-jp-runs"] ?? values.runs, `${name}-runs`),
  ]),
)
const root = path.resolve(import.meta.dirname, "../..")
function absolute(value: string | undefined, name: string): string {
  if (
    !value ||
    !path.isAbsolute(value) ||
    path.resolve(value) === root ||
    path.resolve(value) === path.resolve(root, "../..")
  )
    throw new Error(`${name} must be an absolute path outside the repository root`)
  return value
}
const out = absolute(values.out, "out")
if (!out.endsWith(".json")) throw new Error("out must end with .json")
if (!/^([1-9][0-9]*)(\.[0-9]+)?%$/.test(values["cpu-quota"])) throw new Error("Invalid cpu-quota")
if (!values["quota-child"]) {
  const child = spawnSync(
    "systemd-run",
    [
      "--user",
      "--scope",
      "--quiet",
      "-p",
      `CPUQuota=${values["cpu-quota"]}`,
      "-p",
      `MemoryMax=${values["memory-max"]}`,
      "nice",
      "-n",
      "9",
      process.execPath,
      import.meta.filename,
      ...process.argv.slice(2),
      "--quota-child",
    ],
    { stdio: "inherit", cwd: root },
  )
  if (child.error) throw child.error
  process.exit(child.status ?? 1)
}

const temporary = await mkdtemp(path.join(tmpdir(), "sve-catalog-bench-"))
let server: Awaited<ReturnType<typeof preview>> | undefined
let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined
try {
  let oldRoot: string
  let newRoot: string
  if (values.smoke) {
    oldRoot = path.join(temporary, "old")
    newRoot = path.join(temporary, "new")
    await cp(path.join(root, "fixtures/snapshot"), oldRoot, { recursive: true })
    await cp(oldRoot, newRoot, { recursive: true })
    const indexPath = path.join(newRoot, "snapshots/versions/index.json")
    const index = JSON.parse(await readFile(indexPath, "utf8")) as {
      revision: number
      current: { manifest_path: string; manifest_sha256: string; data_version: string }
    }
    const manifest = JSON.parse(
      await readFile(path.join(newRoot, index.current.manifest_path), "utf8"),
    ) as Record<string, unknown>
    manifest["data_version"] = "20260930T000000Z-0001"
    const bytes = JSON.stringify(manifest)
    const hash = createHash("sha256").update(bytes).digest("hex")
    index.revision += 1
    index.current = {
      ...index.current,
      data_version: String(manifest["data_version"]),
      manifest_path: `snapshots/manifests/${hash}.json`,
      manifest_sha256: `sha256:${hash}`,
    }
    await writeFile(path.join(newRoot, index.current.manifest_path), bytes)
    await writeFile(indexPath, JSON.stringify(index))
    for (const input of [oldRoot, newRoot]) {
      const source = JSON.parse(
        await readFile(path.join(input, "snapshots/versions/index.json"), "utf8"),
      ) as { current: unknown }
      await mkdir(path.join(input, "snapshots/preview"), { recursive: true })
      await writeFile(
        path.join(input, "snapshots/preview/current.json"),
        JSON.stringify(source.current),
      )
    }
  } else {
    oldRoot = await realpath(absolute(values.old, "old"))
    newRoot = await realpath(absolute(values.new, "new"))
  }
  const pointer =
    values.entry === "preview" ? "snapshots/preview/current.json" : "snapshots/versions/index.json"
  for (const input of [oldRoot, newRoot]) await readFile(path.join(input, pointer))
  process.env["SVE_EXPORT_DIR"] = oldRoot
  process.env["SVE_PREVIEW_DIR"] = newRoot
  delete process.env["SVE_DEV_HTTPS"]
  delete process.env["SVE_DEV_TLS_CERT"]
  delete process.env["SVE_DEV_TLS_KEY"]
  const outputDir = path.join(temporary, "build")
  await build({
    root,
    worker: { rollupOptions: { output: { intro: CATALOG_OBSERVER } } },
    build: {
      outDir: outputDir,
      emptyOutDir: true,
      rollupOptions: { input: path.join(root, "bench/catalog.html") },
    },
  })
  server = await preview({
    root,
    build: { outDir: outputDir },
    preview: { host: "127.0.0.1", port: 5791, strictPort: true },
  })
  browser = await chromium.launch()
  const report: Report = {
    format: 1,
    recordedAt: new Date().toISOString(),
    browser: `Chromium ${browser.version()}`,
    revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    dirty:
      execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
    summaries: {},
    conditions: {
      cpuQuota: values["cpu-quota"],
      memoryMax: values["memory-max"],
      sampleMs,
      settleMs,
      entry: values.entry,
      workload: values.smoke
        ? "synthetic; concurrent main + search startup; no UI"
        : "concurrent main + search startup; no UI",
      memoryMethod:
        "CDP Runtime.getHeapUsage: usedSize + backingStorageSize across page and dedicated Workers; sampled gross proxy; incomplete rounds excluded from summaries",
    },
    runs: [],
  }
  for (const scenario of scenarios) {
    const edition = scenario.endsWith("jp") ? "jp" : "en"
    for (let i = 0; i < (counts.get(scenario) ?? 0); i++) {
      process.stdout.write(`${scenario} ${String(i + 1)}/${String(counts.get(scenario))}\n`)
      const context = await browser.newContext()
      try {
        let useNew = !scenario.startsWith("update")
        // Both versions keep the same browser origin and cache namespace during replacement.
        await context.route("**/cdn/**", async (route) => {
          const url = route.request().url()
          if (useNew) {
            const response = await route.fetch({ url: url.replace("/cdn/", "/cdn-preview/") })
            await route.fulfill({ response })
          } else await route.continue()
        })
        const page = await context.newPage()
        page.setDefaultTimeout(180_000)
        const monitor = await CatalogMonitor.create(page)
        await page.goto(`http://127.0.0.1:5791/bench/catalog.html?entry=${values.entry}`)
        await page.waitForFunction("!!globalThis.catalogBenchmark")
        monitor.start(sampleMs)
        const load = async (update: boolean) => {
          let timer: ReturnType<typeof setTimeout> | undefined
          try {
            return await Promise.race([
              new Promise<never>((_, reject) => {
                timer = setTimeout(() => {
                  reject(new Error("Catalog load timed out"))
                }, 180_000)
              }),
              page.evaluate<Omit<Run, "scenario" | "measures" | "tasks" | "memory">>(
                `globalThis.catalogBenchmark.run(${JSON.stringify(edition)}, ${String(update)})`,
              ),
            ])
          } finally {
            if (timer) clearTimeout(timer)
          }
        }
        const finish = async (since: number) => {
          const deadline = Date.now() + 180_000
          while (
            !monitor.measures.some(
              (entry) =>
                entry.startMs >= since && entry.name === "sve:catalog:background:SearchIndex",
            )
          ) {
            if (Date.now() > deadline) throw new Error("Background catalog did not complete")
            await page.waitForTimeout(50)
          }
          await page.waitForTimeout(settleMs)
          await monitor.sample()
        }
        try {
          let started = Date.now()
          const initial = await load(false)
          await finish(started)
          let result = initial
          if (scenario.startsWith("update")) {
            useNew = true
            started = Date.now()
            // The baseline sample includes the still-active old main and Worker catalogs.
            await monitor.sample()
            result = await load(true)
            if (initial.manifestHash === result.manifestHash)
              throw new Error("Update requires different manifests")
            await finish(started)
            result.oldManifestHash = initial.manifestHash
          }
          await monitor.stop()
          const run: Run = {
            scenario,
            ...result,
            measures: monitor.measures.filter((entry) => entry.startMs >= started),
            tasks: monitor.tasks.filter(
              (entry) => entry.startMs >= started && entry.durationMs > 50,
            ),
            memory: monitor.memory.filter((entry) => entry.startMs >= started),
          }
          for (const [realm, stage] of [
            ["main", "catalogOf"],
            ["main", "validate"],
            ["decode Worker", "decode:manifest"],
            ["decode Worker", "decode:file"],
            ["search Worker", "decode:manifest"],
            ["search Worker", "decode:file"],
            ["search Worker", "ingest"],
            ["search Worker", "seal"],
            ["search Worker", "SearchIndex"],
          ] as const) {
            if (
              !run.measures.some(
                (entry) =>
                  realmGroup(entry.realm) === realm && entry.name === `sve:catalog:${stage}`,
              )
            )
              throw new Error(`Missing measure: ${realm} ${stage}`)
          }
          if (!run.memory.some((entry) => entry.complete))
            throw new Error("No complete memory sample")
          report.runs.push(run)
          report.summaries[scenario] = summarize(
            report.runs.filter((entry) => entry.scenario === scenario),
          )
          await writeFile(out, JSON.stringify(report, null, 2) + "\n")
        } finally {
          await monitor.stop()
        }
      } finally {
        await context.close()
      }
    }
  }
  const output = table(report)
  process.stdout.write(output + "\n")
  await writeFile(out.replace(/\.json$/, "") + ".txt", output + "\n")
} finally {
  await browser?.close()
  await server?.close()
  await rm(temporary, { recursive: true, force: true })
}
