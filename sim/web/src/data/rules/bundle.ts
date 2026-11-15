import type {
  RuleNode,
  RulesFile,
  RulesIndex,
  RulesTranslationFile,
  TranslationEntry,
} from "./model"

// The rules page imports this module dynamically, so the whole rules text (about 60 KB gzip) is
// one chunk fetched only on /rules and shipped with the site build, not with the card snapshot.
const ORIGINALS = import.meta.glob<RulesFile | RulesIndex>(
  "../../../../../authored/rules/comprehensive/jp/*/*.json",
  { import: "default", eager: true },
)
const TRANSLATIONS = import.meta.glob<RulesTranslationFile>(
  "../../../../../authored/translations/comprehensive-rules/zh-TW/*/*.json",
  { import: "default", eager: true },
)

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

function newest(versions: readonly string[]): string {
  const parts = (version: string) => version.split(".").map(Number)
  return [...versions].sort((a, b) => {
    const [x, y] = [parts(a), parts(b)]
    for (let i = 0; i < Math.max(x.length, y.length); i++)
      if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0)
    return 0
  })[versions.length - 1] as string
}

function build(): RulesBundle {
  const version = newest([...new Set(Object.keys(ORIGINALS).map(versionOf))])
  const files = new Map<string, RulesFile>()
  let index: RulesIndex | undefined
  for (const [path, data] of Object.entries(ORIGINALS)) {
    if (versionOf(path) !== version) continue
    if (data.kind === "comprehensive-rules-index") index = data
    else files.set(fileOf(path), data)
  }
  if (!index) throw new Error(`comprehensive rules ${version}: index.json missing`)
  const nodes = index.contents.flatMap((item) => files.get(item.file)?.nodes ?? [])
  const translations = new Map<string, TranslationEntry>()
  for (const [path, file] of Object.entries(TRANSLATIONS))
    if (versionOf(path) === version)
      for (const [key, entry] of Object.entries(file.entries)) translations.set(key, entry)
  return { index, nodes, byKey: new Map(nodes.map((node) => [node.key, node])), translations }
}

export const RULES: RulesBundle = build()
