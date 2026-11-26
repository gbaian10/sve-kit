// Builds the battle engine (sim/battle-host) to WebAssembly and generates its JavaScript glue
// into src/battle/engine/generated/. With SVE_BATTLE_EXPORT naming a verified public export, it
// also writes the battle inputs document the Worker loads. The output is a clean build product
// and never committed.
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

import type { CardRef } from "../../src/battle/contract"
import type { BattleBuild } from "../../src/battle/engine/protocol"

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

interface ProgramProjection {
  descriptor: BattleBuild["programs"]
  printings: { card_no: string; printing_id: string }[]
  engine_identity: unknown
}

const sha256 = (bytes: string | Uint8Array) => createHash("sha256").update(bytes).digest("hex")

// The practice mirror uses one copy per rules name. The reader resolves its native locators
// to permanent printing IDs in the build's verified JP snapshot.
const PRACTICE_MAIN = [
  "BP01-151",
  "BP01-152",
  "BP01-154",
  "BP01-155",
  "BP01-157",
  "BP01-158",
  "BP01-159",
  "BP01-163",
  "BP01-164",
  "BP01-166",
  "BP01-167",
  "BP01-171",
  "BP01-173",
  "BP01-174",
  "BP01-176",
  "BP01-177",
  "BP02-106",
  "BP02-107",
  "BP02-109",
  "BP02-110",
  "BP02-113",
  "BP02-115",
  "BP02-117",
  "BP02-118",
  "BP03-107",
  "BP03-109",
  "BP03-110",
  "BP03-111",
  "BP03-112",
  "BP03-114",
  "BP03-115",
  "BP03-118",
  "BP03-119",
  "BP04-116",
  "BP04-117",
  "BP04-119",
  "BP04-121",
  "BP04-123",
  "BP04-124",
  "BP04-127",
]

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

/** Keep the native projection beside the engine; fetch the immutable program at runtime. */
function writeBattleInputs(exportRoot: string, engine: Uint8Array): void {
  const work = mkdtempSync(path.join(tmpdir(), "sve-battle-inputs-"))
  try {
    const output = path.join(work, "reader")
    run("uv", [
      "--directory",
      "carddb",
      "run",
      "--locked",
      "sve-carddb",
      "snapshot",
      "battle-inputs",
      "--export-dir",
      path.resolve(exportRoot),
      "--region",
      "jp",
      "--engine-rules",
      path.join(repo, "authored/rules/engine/index.yaml"),
      "--output",
      output,
      "--published",
    ])
    const read = (name: string) => readFileSync(path.join(output, name), "utf8")
    const references = JSON.parse(read("references.json")) as {
      region: "jp"
      wording: { manifest_sha256: string }
      faces: { card_no: string; native_face: number; card: CardRef }[]
    }
    const { descriptor, printings, engine_identity } = JSON.parse(
      read("program-projection.json"),
    ) as ProgramProjection
    const { key, path: programPath, sha256: programHash, bytes } = descriptor
    const manifest = references.wording.manifest_sha256
    const rules = read("rules.json")
    const input = {
      snapshot: read("snapshot.jsonl"),
      keywords: readFileSync(path.join(repo, "authored/rules/keywords.yaml"), "utf8"),
      rules,
      references,
      printings,
      engine_identity,
      identity: {
        snapshot: { id: "public-export", sha256: manifest.replace(/^sha256:/, "") },
        programs: { id: key, sha256: programHash.replace(/^sha256:/, "") },
        engine: { id: "sve-battle-host", sha256: sha256(engine) },
        rules: { id: "engine-rules", sha256: sha256(rules) },
        region: references.region,
      },
    }
    const front = new Map(
      references.faces
        .filter((face) => face.native_face === 0)
        .map(({ card_no, card }) => [card_no, card]),
    )
    const printing = (number: string): CardRef => {
      const card = front.get(number)
      if (card === undefined) throw new Error(`practice printing is missing: ${number}`)
      return card
    }
    const deck = {
      construction: { kind: "class", value: "ニュートラル" } as const,
      main: PRACTICE_MAIN.map((number) => printing(number).printing_id),
      evolve: [],
    }
    const build: BattleBuild = {
      manifest,
      programs: { key, path: programPath, sha256: programHash, bytes },
      leaders: { p1: printing("BP10-LD05"), p2: printing("BP05-LD02") },
      options: { decks: [deck, deck], control: ["player", "policy"], max_submissions: 300 },
    }
    writeBuild(build)
    writeFileSync(path.join(GENERATED, "battle-inputs.json"), JSON.stringify(input))
  } finally {
    rmSync(work, { recursive: true, force: true })
  }
}

function writeBuild(build: BattleBuild | null): void {
  writeFileSync(
    path.join(GENERATED, "battle-build.ts"),
    `import type { BattleBuild } from "../protocol"
const build: BattleBuild | null = ${JSON.stringify(build)}
export default build
`,
  )
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
  if (exportRoot) {
    writeBattleInputs(exportRoot, readFileSync(path.join(GENERATED, "engine_bg.wasm")))
  } else {
    writeBuild(null)
    writeFileSync(path.join(GENERATED, "battle-inputs.json"), "null")
  }
}

if (import.meta.main) main()
