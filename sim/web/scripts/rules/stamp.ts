// Fills the missing source hashes of comprehensive-rules translation files from their originals:
//   bun run rules:stamp <repo>/authored/translations/comprehensive-rules/zh-TW/1.29.0/ch01.json …
// Existing hashes are kept; entries whose original changed since translation are listed as stale.
// Prints keys and counts only, never rules wording.
import { readFileSync, writeFileSync } from "node:fs"
import path from "node:path"

import type { RulesFile, RulesTranslationFile } from "../../src/data/rules/model"
import { stampTranslation, translationProblems } from "../../src/data/rules/source-hash"

const REPO = path.resolve(import.meta.dirname, "../../../..")

/** `…/comprehensive-rules/<language>/<version>/<file>` pairs with the original of the same name. */
function originalPath(translation: string): string {
  const file = path.basename(translation)
  const version = path.basename(path.dirname(translation))
  return path.join(REPO, "authored/rules/comprehensive/jp", version, file)
}

const files = process.argv.slice(2)
if (files.length === 0) {
  process.stderr.write("usage: bun run rules:stamp <translation.json>…\n")
  process.exit(2)
}
let failed = false
for (const argument of files) {
  const target = path.resolve(argument)
  const original = JSON.parse(readFileSync(originalPath(target), "utf8")) as RulesFile
  const translation = JSON.parse(readFileSync(target, "utf8")) as RulesTranslationFile
  const { file, stamped, stale } = stampTranslation(original.nodes, translation)
  if (stamped.length > 0) writeFileSync(target, `${JSON.stringify(file, null, 2)}\n`)
  const other = translationProblems(original.nodes, file).filter(
    (problem) => problem.kind !== "stale",
  )
  process.stdout.write(
    `${path.basename(target)}: stamped ${String(stamped.length)}, stale ${String(stale.length)}\n`,
  )
  for (const key of stale) process.stdout.write(`  stale ${key}\n`)
  for (const problem of other)
    process.stdout.write(
      `  ${problem.kind} ${problem.key}${problem.kind === "shape" ? ` (${problem.detail})` : ""}\n`,
    )
  if (stale.length > 0 || other.length > 0) failed = true
}
process.exit(failed ? 1 : 0)
