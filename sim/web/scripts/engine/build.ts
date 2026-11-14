// Builds the battle engine (sim/battle-host) to WebAssembly and generates its JavaScript glue
// into src/battle/engine/generated/. The output is a clean build product and never committed.
import { spawnSync } from "node:child_process"
import { readFileSync, rmSync } from "node:fs"
import path from "node:path"

const repo = path.resolve(import.meta.dirname, "../../../..")
export const GENERATED = path.resolve(import.meta.dirname, "../../src/battle/engine/generated")
const TARGET = "wasm32-unknown-unknown"

/** The wasm-bindgen crate version locked for the build; the CLI must match it exactly. */
export function lockedBindgenVersion(lock: string): string {
  const match = /\[\[package\]\]\nname = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)
  if (match?.[1] === undefined) throw new Error("Cargo.lock has no wasm-bindgen package")
  return match[1]
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

function main(): void {
  const version = lockedBindgenVersion(readFileSync(path.join(repo, "Cargo.lock"), "utf8"))
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
}

if (import.meta.main) main()
