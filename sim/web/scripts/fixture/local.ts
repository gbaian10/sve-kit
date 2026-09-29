import path from "node:path"

import type { Card, Face, Family, Printing, Qa, Text, Vocabulary } from "./cards"

/** One line of the private test card list (`SVE_TEST_SNAPSHOT`); only the fields used here. */
export interface LocalRecord {
  readonly number: string
  readonly faces: readonly LocalFace[]
  readonly release_date: string | null
  readonly qa: readonly {
    readonly id: string
    readonly date: string
    readonly question: string
    readonly answer: string
  }[]
}

interface LocalFace {
  readonly name: string
  readonly card_class: string
  readonly card_type: string
  readonly traits: readonly string[] | null
  readonly rarity: string | null
  readonly product: string | null
  readonly cost: string | null
  readonly power: string | null
  readonly hp: string | null
  readonly text: string | null
  readonly flavor: string | null
  readonly illustrator: string | null
  readonly image: string
}

export interface LocalOptions {
  /** Keep only these set prefixes (e.g. BP01); empty keeps every set. */
  readonly sets?: readonly string[]
  readonly limit?: number
  /** Resolves a face's image to a local file, or null when it is not on disk. */
  readonly imagePath: (setCode: string, imageUrl: string) => string | null
}

export interface LocalConversion {
  readonly cards: readonly Card[]
  readonly families: Record<string, Family>
  readonly vocabulary: Vocabulary
}

const CLASS_CODES: Record<string, string | null> = {
  エルフ: "elf",
  ロイヤル: "royal",
  ウィッチ: "witch",
  ドラゴン: "dragon",
  ナイトメア: "nightmare",
  ビショップ: "bishop",
  ネメシス: "nemesis",
  ニュートラル: null,
  "-": null,
}
const TYPE_CODES: Record<string, string> = {
  フォロワー: "follower",
  "フォロワー・エボルヴ": "evolved",
  "フォロワー・アドバンス": "advanced",
  "フォロワー・トークン": "follower_token",
  スペル: "spell",
  "スペル・エボルヴ": "spell_evolved",
  "スペル・アドバンス": "spell_advanced",
  "スペル・トークン": "spell_token",
  アミュレット: "amulet",
  "アミュレット・エボルヴ": "amulet_evolved",
  "アミュレット・トークン": "amulet_token",
  "クレスト・トークン": "crest_token",
  "イクイップメント・トークン": "equipment_token",
  リーダー: "leader",
  EP: "ep",
  SEP: "sep",
}
const RARITY_CODES: Record<string, string> = {
  BR: "bronze",
  SR: "silver",
  GR: "gold",
  LG: "legend",
  UR: "ur",
  SP: "sp",
  SL: "sl",
  SSP: "ssp",
  PR: "pr",
}
const PREMIUM = "プレミアム"

function familyKind(prefix: string): Family["kind"] {
  if (prefix.startsWith("BP")) return "booster"
  if (prefix === "PR") return "promo"
  if (/^(?:E?CP)/.test(prefix)) return "collaboration"
  if (/^(?:C?SD|DSD|ETD|EBD)/.test(prefix)) return "deck"
  if (/^(?:PCS|SCS|LCS)/.test(prefix)) return "special"
  return "other"
}

function numberOrNull(value: string | null): number | null {
  return value !== null && /^-?\d+$/.test(value) ? Number(value) : null
}

function rarityOf(raw: string | null): { code: string | null; premium: boolean } {
  const premium = raw !== null && raw.includes(PREMIUM)
  const base = (raw ?? "").replace(`・${PREMIUM}`, "").replace(PREMIUM, "")
  return { code: base === "" || base === "-" ? null : (RARITY_CODES[base] ?? null), premium }
}

/**
 * Real Japanese card data as generator input: one JP printing per card number, no translations,
 * no engine data; Q&A is shared between every card it names. Vocabulary is whatever the records
 * use, labelled with their own Japanese words.
 */
export function convertLocal(
  records: readonly LocalRecord[],
  options: LocalOptions,
): LocalConversion {
  const wanted = new Set(options.sets ?? [])
  const selected = records
    .filter((record) => wanted.size === 0 || wanted.has(record.number.split("-")[0] ?? ""))
    .slice(0, options.limit ?? records.length)
  const families: Record<string, Family> = {}
  const types: Record<string, Text> = {}
  const rarities: Record<string, Text> = {}
  const traits: Record<string, Text> = {}
  // Unknown labels get one code each in first-seen order, so two labels never share a code.
  const numbered = (prefix: string, into: Record<string, Text>) => {
    const codes = new Map<string, string>()
    return (label: string): string => {
      let code = codes.get(label)
      if (code === undefined) {
        code = `${prefix}_${String(codes.size + 1)}`
        codes.set(label, code)
        into[code] = { ja: label }
      }
      return code
    }
  }
  const traitCode = numbered("trait", traits)
  const unknownTypeCode = numbered("type", types)
  const qaOwners = new Map<string, { qa: Qa; cards: string[] }>()
  const cards: Card[] = []
  selected.forEach((record, index) => {
    const prefix = record.number.split("-")[0] ?? record.number
    const setCode = prefix.toLowerCase()
    const setId = `set:${setCode}`
    const first = record.faces[0]
    if (!first) return
    families[setId] ??= {
      code: setCode,
      publicCode: prefix,
      kind: familyKind(prefix),
      name: { ja: first.product ?? prefix },
      regions: ["jp"],
    }
    const lower = record.number.toLowerCase()
    const faces: Face[] = record.faces.map((face, ordinal) => {
      const known = TYPE_CODES[face.card_type]
      const type = known === undefined ? unknownTypeCode(face.card_type) : known
      if (known !== undefined) types[type] ??= { ja: face.card_type }
      return {
        id: `f:${lower}:${String(ordinal)}`,
        side: ordinal === 0 ? "front" : "back",
        name: { ja: face.name },
        effect: { ja: face.text ?? "" },
        type,
        cost: numberOrNull(face.cost),
        attack: numberOrNull(face.power),
        defense: numberOrNull(face.hp),
        traits: (face.traits ?? []).map(traitCode),
        ...(face.flavor === null || face.flavor === "" ? {} : { flavor: face.flavor }),
        ...(face.illustrator === null || face.illustrator === ""
          ? {}
          : { illustrator: face.illustrator }),
      }
    })
    const rarity = rarityOf(first.rarity)
    if (rarity.code !== null)
      rarities[rarity.code] ??= { ja: (first.rarity ?? "").replace(`・${PREMIUM}`, "") }
    const imagePaths = record.faces.map((face) => options.imagePath(prefix, face.image))
    const printing: Printing = {
      id: `p:${lower}`,
      region: "jp",
      cardNo: record.number,
      intId: index + 1,
      variant: "standard",
      rarity: rarity.code,
      premium: rarity.premium,
      product: `prod:${setCode}-jp`,
      imagePaths,
    }
    const cardId = `c:${lower}`
    const qa: Qa[] = []
    for (const entry of record.qa) {
      const owner = qaOwners.get(entry.id)
      if (owner) {
        owner.cards.push(cardId)
        continue
      }
      const item: Qa = {
        id: `qa:${entry.id.toLowerCase()}`,
        number: entry.id,
        question: { ja: entry.question },
        answer: { ja: entry.answer },
        publishedOn: entry.date,
      }
      qaOwners.set(entry.id, { qa: item, cards: [cardId] })
      qa.push(item)
    }
    cards.push({
      id: cardId,
      set: setId,
      class: CLASS_CODES[first.card_class] ?? null,
      layout: faces.length > 1 ? "double_faced" : "single",
      faces,
      printings: [printing],
      mapping: "unmapped",
      engine: "missing_dsl",
      ...(qa.length > 0 ? { qa } : {}),
    })
  })
  // Attach the full card list to each shared question now that every card has been seen.
  const withQa = cards.map((card) =>
    card.qa === undefined
      ? card
      : {
          ...card,
          qa: card.qa.map((item) => ({
            ...item,
            cards: qaOwners.get(item.number)?.cards ?? [card.id],
          })),
        },
  )
  const classes: Record<string, Text> = {
    elf: { ja: "エルフ", zhHant: "精靈", en: "Forestcraft" },
    royal: { ja: "ロイヤル", zhHant: "皇家", en: "Swordcraft" },
    witch: { ja: "ウィッチ", zhHant: "巫師", en: "Runecraft" },
    dragon: { ja: "ドラゴン", zhHant: "龍族", en: "Dragoncraft" },
    nightmare: { ja: "ナイトメア", zhHant: "夢魘", en: "Abysscraft" },
    bishop: { ja: "ビショップ", zhHant: "主教", en: "Havencraft" },
    nemesis: { ja: "ネメシス", zhHant: "復仇者", en: "Portalcraft" },
  }
  return { cards: withQa, families, vocabulary: { classes, types, rarities, traits } }
}

export interface ProtectedPaths {
  /** The checkout running the tool: official card text must never land inside it. */
  readonly repoRoot: string
  /** Source data; output may live in a subdirectory but must not replace or contain it. */
  readonly dataDir: string
  /** Directory of the card list being read. */
  readonly listDir: string
}

function contains(parent: string, child: string): boolean {
  return (
    child === parent || child.startsWith(parent.endsWith(path.sep) ? parent : parent + path.sep)
  )
}

/**
 * Why the output directory must be refused, or null when it is acceptable. All paths must already be
 * absolute and canonical (symlinks resolved); the caller separately checks the marker file for an
 * existing directory.
 */
export function outputTargetError(target: string, protectedPaths: ProtectedPaths): string | null {
  const { repoRoot, dataDir, listDir } = protectedPaths
  if (contains(repoRoot, target)) return `output ${target} is inside the repository ${repoRoot}`
  if (contains(target, repoRoot)) return `output ${target} contains the repository ${repoRoot}`
  if (contains(target, dataDir))
    return `output ${target} is or contains the data directory ${dataDir}`
  if (contains(target, listDir) || contains(listDir, target))
    return `output ${target} overlaps the card list directory ${listDir}`
  return null
}
