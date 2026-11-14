// Generates the battle contract's TypeScript types and validator from the JSON Schema that
// sim/battle-contract derives from its Rust types. Nothing generated is committed.
//
//   generate   write the generated files, leaving up-to-date ones untouched
//   check      fail unless a fresh generation equals the files on disk
//   roundtrip  Rust → JSON → validator → Rust over every contract fixture
import { spawnSync } from "node:child_process"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import path from "node:path"
import { pathToFileURL } from "node:url"

import type { AnySchema } from "ajv"
import Ajv2020 from "ajv/dist/2020"
import standaloneCode from "ajv/dist/standalone"
import { compile, type JSONSchema } from "json-schema-to-typescript"

const WEB = path.resolve(import.meta.dirname, "../..")
const REPO = path.resolve(WEB, "../..")
const OUT = path.join(WEB, "src/battle/contract/generated")
const BANNER = "// Generated from sim/battle-contract by `bun run generate:contract`. Do not edit."

// One validator per boundary message, plus the two large parts that fixtures cover on their own.
const ROOTS = ["HostMessage", "ClientMessage", "BattleObservation", "Question"] as const
type Root = (typeof ROOTS)[number]
type Validators = Record<`validate${Root}`, (value: unknown) => boolean>

/** Run the contract crate's tool and return what it prints. */
function tool(command: string, input = ""): string {
  const result = spawnSync(
    "cargo",
    ["run", "--quiet", "--locked", "--package", "sve-battle-contract", "--", command],
    {
      cwd: REPO,
      input,
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024,
      stdio: ["pipe", "pipe", "inherit"],
    },
  )
  if (result.status !== 0) throw new Error(`sve-battle-contract ${command} failed`)
  return result.stdout
}

/**
 * A description beside a `$ref` makes the type generator emit one copy of the referenced type
 * per property; the reference alone names the shared type.
 */
function referencesOnly(node: unknown): unknown {
  if (Array.isArray(node)) return node.map(referencesOnly)
  if (typeof node !== "object" || node === null) return node
  const entries = Object.entries(node)
  const reference = entries.find(([key]) => key === "$ref")
  if (reference !== undefined) return Object.fromEntries([reference])
  return Object.fromEntries(entries.map(([key, value]) => [key, referencesOnly(value)]))
}

/** Documentation strings are for readers of the types; the validator would only carry their weight. */
function withoutDescriptions(node: unknown): unknown {
  if (Array.isArray(node)) return node.map(withoutDescriptions)
  if (typeof node !== "object" || node === null) return node
  return Object.fromEntries(
    Object.entries(node)
      // A property that happened to be named "description" would hold a schema, not a string.
      .filter(([key, value]) => key !== "description" || typeof value !== "string")
      .map(([key, value]) => [key, withoutDescriptions(value)]),
  )
}

async function generated(): Promise<Map<string, string>> {
  const schema: unknown = JSON.parse(tool("schema"))
  const types = await compile(referencesOnly(schema) as JSONSchema, "BattleContract", {
    additionalProperties: false,
    bannerComment: BANNER,
    style: { printWidth: 100, semi: false },
  })
  // Standalone code needs no Ajv at run time; the validator ships only to tests and dev builds.
  const ajv = new Ajv2020({
    code: { source: true, esm: true },
    inlineRefs: false,
    messages: false,
    strict: true,
  })
  ajv.addSchema(withoutDescriptions(schema) as AnySchema, "contract")
  const validators = standaloneCode(
    ajv,
    Object.fromEntries(ROOTS.map((root) => [`validate${root}`, `contract#/$defs/${root}`])),
  )
  const declarations = [
    BANNER,
    `import type { ${ROOTS.join(", ")} } from "./types"`,
    ...ROOTS.map(
      (root) => `export declare function validate${root}(value: unknown): value is ${root}`,
    ),
    "",
  ].join("\n")
  return new Map([
    ["types.ts", types],
    ["validate.js", `${BANNER}\n${validators}\n`],
    ["validate.d.ts", declarations],
  ])
}

function onDisk(name: string): string | undefined {
  const file = path.join(OUT, name)
  return existsSync(file) ? readFileSync(file, "utf8") : undefined
}

async function generate(): Promise<void> {
  mkdirSync(OUT, { recursive: true })
  for (const [name, content] of await generated()) {
    // An unchanged file keeps its timestamp, so repeated runs do not invalidate caches.
    if (onDisk(name) !== content) writeFileSync(path.join(OUT, name), content)
  }
}

async function check(): Promise<void> {
  const stale = [...(await generated())]
    .filter(([name, content]) => onDisk(name) !== content)
    .map(([name]) => name)
  if (stale.length > 0)
    throw new Error(`generation is not reproducible or is out of date: ${stale.join(", ")}`)
}

async function roundtrip(): Promise<void> {
  const validators = (await import(pathToFileURL(path.join(OUT, "validate.js")).href)) as Validators
  const returned = tool("emit")
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => {
      // Parsing and printing again passes every number through a JavaScript number.
      const { root, name, message } = JSON.parse(line) as {
        root: Root
        name: string
        message: unknown
      }
      if (!validators[`validate${root}`](message))
        throw new Error(`the validator refuses ${name}, which the Rust encoder produced`)
      return JSON.stringify({ root, name, message })
    })
  tool("verify", `${returned.join("\n")}\n`)
}

const commands: Record<string, () => Promise<void>> = { generate, check, roundtrip }
const command = commands[process.argv[2] ?? ""]
if (command === undefined) throw new Error("usage: main.ts generate | check | roundtrip")
await command()
