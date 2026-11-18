import type {
  RuleNode,
  RulesFile,
  RulesIndex,
  RulesTranslationFile,
  TranslationEntry,
} from "./model"

export interface RulesBundle {
  readonly index: RulesIndex
  /** Every node in reading order across all files. */
  readonly nodes: readonly RuleNode[]
  readonly byKey: ReadonlyMap<string, RuleNode>
  /** zh-TW entries by node key; translation files only group them by chapter. */
  readonly translations: ReadonlyMap<string, TranslationEntry>
}

const versionOf = (path: string) => path.split("/").at(-2) ?? ""
const fileOf = (path: string) => path.split("/").at(-1) ?? ""

function newest(versions: readonly string[]): string | undefined {
  const parts = (version: string) => version.split(".").map(Number)
  return versions
    .toSorted((a, b) => {
      const [x, y] = [parts(a), parts(b)]
      for (let i = 0; i < Math.max(x.length, y.length); i++)
        if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0)
      return 0
    })
    .at(-1)
}

/**
 * The newest version among one edition's files (`<version>/<file>.json`), with the translations
 * of that same version.
 */
export function buildRules(
  originals: Readonly<Record<string, RulesFile | RulesIndex>>,
  translationFiles: Readonly<Record<string, RulesTranslationFile>>,
): RulesBundle {
  const version = newest([...new Set(Object.keys(originals).map(versionOf))])
  const files = new Map<string, RulesFile>()
  let index: RulesIndex | undefined
  for (const [path, data] of Object.entries(originals)) {
    if (versionOf(path) !== version) continue
    if (data.kind === "comprehensive-rules-index") index = data
    else files.set(fileOf(path), data)
  }
  if (!index) throw new Error(`comprehensive rules ${String(version)}: index.json missing`)
  const nodes = index.contents.flatMap((item) => files.get(item.file)?.nodes ?? [])
  const translations = new Map<string, TranslationEntry>()
  for (const [path, file] of Object.entries(translationFiles))
    if (versionOf(path) === version)
      for (const [key, entry] of Object.entries(file.entries)) translations.set(key, entry)
  return { index, nodes, byKey: new Map(nodes.map((node) => [node.key, node])), translations }
}
