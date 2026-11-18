// Builds the battle engine (sim/battle-host) to WebAssembly and generates its JavaScript glue
// into src/battle/engine/generated/. With SVE_BATTLE_EXPORT naming a verified public export, it
// also writes the battle inputs document the Worker loads. The output is a clean build product
// and never committed.
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

const repo = path.resolve(import.meta.dirname, "../../../..")
export const GENERATED = path.resolve(import.meta.dirname, "../../src/battle/engine/generated")
const TARGET = "wasm32-unknown-unknown"

/** The locked wasm-bindgen crate version from `cargo metadata`; the CLI must match it exactly. */
export function lockedBindgenVersion(metadata: string): string {
  const { packages } = JSON.parse(metadata) as { packages: { name: string; version: string }[] }
  const versions = new Set(
    packages.filter((item) => item.name === "wasm-bindgen").map((item) => item.version),
  )
  const [version] = versions
  if (versions.size !== 1 || version === undefined) {
    throw new Error(`expected one locked wasm-bindgen version, found ${String(versions.size)}`)
  }
  return version
}

/** The cdylib path from cargo's JSON messages. */
export function wasmArtifact(messages: string): string {
  const files = messages
    .split("\n")
    .filter((line) => line.startsWith("{"))
    .map(
      (line) =>
        JSON.parse(line) as { reason?: string; target?: { name?: string }; filenames?: string[] },
    )
    .filter(
      (message) =>
        message.reason === "compiler-artifact" && message.target?.name === "sve_battle_host",
    )
    .flatMap((message) => message.filenames ?? [])
    .filter((file) => file.endsWith(".wasm"))
  const [file] = files
  if (files.length !== 1 || file === undefined) {
    throw new Error(`expected one sve_battle_host .wasm artifact, found ${String(files.length)}`)
  }
  return file
}

/** The carddb reader's outputs and the programs, all for one region. */
export interface BattleParts {
  /** Native snapshot (JSON lines), exactly as the reader wrote it. */
  snapshot: string
  /** The reader's reference identities (JSON). */
  references: string
  /** The engine rules the reader bound to that snapshot (JSON). */
  rules: string
  keywords: string
  /** The whole current effect selection (JSON), never a deck's subset. */
  selection: string
}

const sha256 = (bytes: string | Uint8Array) => createHash("sha256").update(bytes).digest("hex")

/** The battle inputs document; its identity names exactly these bytes and this engine build. */
export function battleInputs(parts: BattleParts, engine: Uint8Array): string {
  const references = JSON.parse(parts.references) as {
    region: "jp" | "en"
    wording: { manifest_sha256: string } | null
  }
  const manifest = references.wording?.manifest_sha256.replace(/^sha256:/, "")
  if (manifest === undefined) throw new Error("the reader's references name no public manifest")
  return JSON.stringify({
    snapshot: parts.snapshot,
    keywords: parts.keywords,
    selection: parts.selection,
    rules: parts.rules,
    references,
    identity: {
      snapshot: { id: "public-export", sha256: manifest },
      programs: { id: "current-selection", sha256: sha256(parts.selection) },
      engine: { id: "sve-battle-host", sha256: sha256(engine) },
      rules: { id: "engine-rules", sha256: sha256(parts.rules) },
      region: references.region,
    },
  })
}

function run(command: string, args: string[], capture = false): string {
  const result = spawnSync(command, args, {
    cwd: repo,
    encoding: "utf8",
    stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
    maxBuffer: 64 * 1024 * 1024,
  })
  if (result.error !== undefined) throw result.error
  if (result.status !== 0) throw new Error(`${command} exited with ${String(result.status)}`)
  return result.stdout
}

/** Read one verified public export and the repository's programs into the inputs document. */
function writeBattleInputs(exportRoot: string, engine: Uint8Array): void {
  const work = mkdtempSync(path.join(tmpdir(), "sve-battle-inputs-"))
  const carddb = (...args: string[]) =>
    run("uv", ["--directory", "carddb", "run", "--locked", "sve-carddb", ...args])
  try {
    const reader = path.join(work, "reader")
    carddb(
      "snapshot",
      "battle-inputs",
      "--export-dir",
      path.resolve(exportRoot),
      "--region",
      "jp",
      "--engine-rules",
      path.join(repo, "authored/rules/engine/index.yaml"),
      "--output",
      reader,
    )
    // The programs come from the authored effects over the JP card batch the engine's public
    // inputs pin; the engine rejects them if that batch's facts differ from the export's.
    const { batch_id } = JSON.parse(
      readFileSync(path.join(repo, "tests/engine/card-input.json"), "utf8"),
    ) as { batch_id: string }
    const profile = path.join(work, "profile.json")
    writeFileSync(profile, JSON.stringify({ profile: "public", repo, jp_batch_id: batch_id }))
    const check = path.join(work, "check.json")
    carddb("author", "check", "--inputs", profile, "--output", check)
    const { selection } = JSON.parse(readFileSync(check, "utf8")) as { selection: unknown }
    const read = (name: string) => readFileSync(path.join(reader, name), "utf8")
    const parts = {
      snapshot: read("snapshot.jsonl"),
      references: read("references.json"),
      rules: read("rules.json"),
      keywords: readFileSync(path.join(repo, "authored/rules/keywords.yaml"), "utf8"),
      selection: JSON.stringify(selection),
    }
    writeFileSync(path.join(GENERATED, "battle-inputs.json"), battleInputs(parts, engine))
  } finally {
    rmSync(work, { recursive: true, force: true })
  }
}

function main(): void {
  const version = lockedBindgenVersion(
    run("cargo", ["metadata", "--locked", "--format-version", "1"], true),
  )
  const bindgen = process.env["SVE_WASM_BINDGEN"] ?? "wasm-bindgen"
  let found: string
  try {
    found = run(bindgen, ["--version"], true).trim()
  } catch {
    found = "not found"
  }
  if (found !== `wasm-bindgen ${version}`) {
    throw new Error(
      `wasm-bindgen ${version} is required (found: ${found}). Install it with ` +
        `\`cargo install wasm-bindgen-cli --version ${version} --locked\` or set SVE_WASM_BINDGEN.`,
    )
  }
  const messages = run(
    "cargo",
    [
      "build",
      "--locked",
      "--profile",
      "wasm",
      "--target",
      TARGET,
      "-p",
      "sve-battle-host",
      "--message-format=json-render-diagnostics",
    ],
    true,
  )
  rmSync(GENERATED, { recursive: true, force: true })
  run(bindgen, [
    "--target",
    "web",
    "--remove-name-section",
    "--remove-producers-section",
    "--out-dir",
    GENERATED,
    "--out-name",
    "engine",
    wasmArtifact(messages),
  ])
  const exportRoot = process.env["SVE_BATTLE_EXPORT"]
  if (exportRoot !== undefined) {
    writeBattleInputs(exportRoot, readFileSync(path.join(GENERATED, "engine_bg.wasm")))
  }
}

if (import.meta.main) main()
