// Handwritten synthetic cards for the development snapshot. Nothing here is real card text; the
// point is coverage of every shape the pages must render (docs/schema/snapshot-format.md).

type ClassCode = "elf" | "royal" | "witch" | "dragon" | "nightmare" | "bishop"
type TypeCode = "follower" | "spell" | "amulet" | "evolved" | "leader" | "token"
type Rarity = "bronze" | "silver" | "gold" | "legend" | "sp"
export type Region = "jp" | "en"
type MappingState = "confirmed" | "unmapped" | "pending" | "confirmed_none"

export interface Text {
  readonly ja: string
  readonly zhHant?: string
  /** English: an official counterpart when the card has an EN printing, else a machine draft. */
  readonly en?: string
}

export interface Face {
  readonly id: string
  readonly side: "front" | "back"
  readonly name: Text
  readonly effect: Text
  readonly type: string
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  readonly traits: readonly string[]
  readonly flavor?: string
  readonly illustrator?: string
  /** An older wording that the current revision replaced (errata); JP only. */
  readonly previousEffectJa?: string
}

export interface Printing {
  readonly id: string
  readonly region: Region
  readonly cardNo: string
  readonly intId: number
  readonly variant: "standard" | "alt" | "signed"
  readonly rarity: string | null
  readonly premium?: boolean
  /**
   * Source image per face ordinal, for the real-data build; the synthetic build paints colours.
   * A null entry marks that face's image as missing instead of painting a placeholder.
   */
  readonly imagePaths?: readonly (string | null)[]
  readonly product: string
  readonly stamp?: string
  /** Image state for the front face; back faces reuse it. */
  readonly image?: "approved" | "pending" | "withdrawn" | "missing"
}

export interface Qa {
  readonly id: string
  readonly number: string
  readonly question: Text
  readonly answer: Text
  /** Extra revisions of the same question (newest is current). */
  readonly revisions?: number
  /** Every card the question is about; defaults to the card it is attached to. */
  readonly cards?: readonly string[]
  /** First publication date (YYYY-MM-DD); synthetic data uses a fixed demo date. */
  readonly publishedOn?: string
}

interface Errata {
  readonly id: string
  readonly announcedOn: string
  readonly effectiveOn: string
  readonly reasonJa: string
  readonly printings: readonly string[]
}

export interface Card {
  readonly id: string
  readonly set: string
  readonly class: string | null
  readonly layout: "single" | "double_faced"
  readonly faces: readonly Face[]
  readonly printings: readonly Printing[]
  readonly mapping: MappingState
  readonly keywords?: readonly {
    readonly code: string
    readonly relation: "has" | "grants" | "refers" | "counts"
  }[]
  readonly coverage?: "complete" | "partial" | "none"
  readonly related?: readonly {
    readonly to: string
    readonly relation: "evolves_to" | "produces_token" | "mentions" | "advances_to"
  }[]
  readonly qa?: readonly Qa[]
  /** Nicknames people type (search_alias kind `card`), by language. */
  readonly aliases?: readonly { readonly lang: "ja" | "en" | "zh-Hant"; readonly text: string }[]
  readonly errata?: Errata
  readonly ban?: {
    readonly maxCopies: 0 | 1
    readonly effectiveFrom: string
    readonly announcedOn: string
    readonly state: "confirmed" | "announced"
  }
  readonly engine: "missing_dsl" | "draft" | "reviewed" | "engine_passed" | "load_rejected"
  readonly enBlock?: readonly string[]
}

export interface Family {
  readonly code: string
  readonly publicCode: string
  readonly kind: "booster" | "promo" | "deck" | "collaboration" | "special" | "other"
  readonly name: Text
  readonly regions: readonly Region[]
}

export const SETS: Record<string, Family> = {
  "set:bp01": {
    code: "bp01",
    publicCode: "BP01",
    kind: "booster",
    name: { ja: "試作ブースター第1弾", zhHant: "試作補充包第1彈", en: "Prototype Booster 1" },
    regions: ["jp", "en"],
  },
  "set:pr": {
    code: "pr",
    publicCode: "PR",
    kind: "promo",
    name: { ja: "試作プロモ", zhHant: "試作促銷卡", en: "Prototype Promos" },
    regions: ["jp"],
  },
  "set:sd01": {
    code: "sd01",
    publicCode: "SD01",
    kind: "deck",
    name: { ja: "試作スタートデッキ", zhHant: "試作起始牌組", en: "Prototype Starter Deck" },
    regions: ["jp", "en"],
  },
}

export interface Vocabulary {
  readonly classes: Record<string, Text>
  readonly types: Record<string, Text>
  readonly rarities: Record<string, Text>
  readonly traits: Record<string, Text>
}

const CLASSES: Record<ClassCode, Text> = {
  elf: { ja: "エルフ", zhHant: "精靈", en: "Forestcraft" },
  royal: { ja: "ロイヤル", zhHant: "皇家", en: "Swordcraft" },
  witch: { ja: "ウィッチ", zhHant: "巫師", en: "Runecraft" },
  dragon: { ja: "ドラゴン", zhHant: "龍族", en: "Dragoncraft" },
  nightmare: { ja: "ナイトメア", zhHant: "夢魘", en: "Abysscraft" },
  bishop: { ja: "ビショップ", zhHant: "主教", en: "Havencraft" },
}

const TYPES: Record<TypeCode, Text> = {
  follower: { ja: "フォロワー", zhHant: "從者", en: "Follower" },
  spell: { ja: "スペル", zhHant: "法術", en: "Spell" },
  amulet: { ja: "アミュレット", zhHant: "護符", en: "Amulet" },
  evolved: { ja: "進化フォロワー", zhHant: "進化從者", en: "Evolved follower" },
  leader: { ja: "リーダー", zhHant: "主戰者", en: "Leader" },
  token: { ja: "トークン", zhHant: "衍生物", en: "Token" },
}

const RARITIES: Record<Rarity, Text> = {
  bronze: { ja: "ブロンズ", zhHant: "銅", en: "Bronze" },
  silver: { ja: "シルバー", zhHant: "銀", en: "Silver" },
  gold: { ja: "ゴールド", zhHant: "金", en: "Gold" },
  legend: { ja: "レジェンド", zhHant: "傳說", en: "Legendary" },
  sp: { ja: "スペシャル", zhHant: "特別", en: "Special" },
}

const TRAITS: Record<string, Text> = {
  soldier: { ja: "兵士", zhHant: "士兵", en: "Officer" },
  commander: { ja: "指揮官", zhHant: "指揮官", en: "Commander" },
  fairy: { ja: "フェアリー", zhHant: "妖精", en: "Fairy" },
  machine: { ja: "機械", zhHant: "機械", en: "Machina" },
  artifact: { ja: "アーティファクト", zhHant: "神器", en: "Artifact" },
}

export const KEYWORDS: Record<
  string,
  {
    readonly name: Text
    readonly definition: Text
    readonly kind: "printed_keyword" | "mechanic" | "resource"
  }
> = {
  fanfare: {
    kind: "printed_keyword",
    name: { ja: "ファンファーレ", zhHant: "入場曲", en: "Fanfare" },
    definition: {
      ja: "手札から出た時に働く。",
      zhHant: "從手牌打出時發動。",
      en: "Works when played from hand.",
    },
  },
  lastword: {
    kind: "printed_keyword",
    name: { ja: "ラストワード", zhHant: "謝幕曲", en: "Last Words" },
    definition: {
      ja: "破壊された時に働く。",
      zhHant: "被破壞時發動。",
      en: "Works when destroyed.",
    },
  },
  ward: {
    kind: "printed_keyword",
    name: { ja: "守護", zhHant: "守護", en: "Ward" },
    definition: {
      ja: "相手はこのフォロワーを先に攻撃しなければならない。",
      zhHant: "對手必須先攻擊此從者。",
      en: "Opponents must attack this follower first.",
    },
  },
  evolve: {
    kind: "mechanic",
    name: { ja: "進化", zhHant: "進化", en: "Evolve" },
    definition: {
      ja: "EPを支払い進化フォロワーに重ねる。",
      zhHant: "支付 EP 覆蓋進化從者。",
      en: "Pay EP to stack the evolved follower.",
    },
  },
}

export const STAMPS = {
  "stamp:champ26": {
    code: "champ26",
    series: "championship",
    text: "CHAMPIONSHIP 2026",
    kind: "tournament",
    year: 2026,
  },
} as const

export const PROFILE = {
  id: "profile:jp-standard",
  region: "jp" as Region,
  formatCode: "standard",
  name: { ja: "スタンダード", zhHant: "標準", en: "Standard" },
  revisionId: "profrev:jp-standard-1",
  effectiveFrom: "2026-01-01",
}

const bp = (id: string, region: Region, no: number, extra: Partial<Printing> = {}): Printing => ({
  id: `p:${id}`,
  region,
  cardNo: `${region === "jp" ? "BP01" : "BP01EN"}-${String(no).padStart(3, "0")}`,
  intId: (region === "jp" ? 1000 : 5000) + no,
  variant: "standard",
  rarity: "bronze",
  product: region === "jp" ? "prod:bp01-jp" : "prod:bp01-en",
  ...extra,
})

export const CARDS: readonly Card[] = [
  {
    id: "c:bp01-001",
    set: "set:bp01",
    class: "royal",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-001",
        side: "front",
        type: "follower",
        cost: 1,
        attack: 1,
        defense: 1,
        traits: ["soldier"],
        name: { ja: "試作の見習い兵", zhHant: "試作見習兵", en: "Prototype Recruit" },
        effect: {
          ja: "【ファンファーレ】自分のリーダーを1回復する。",
          zhHant: "【入場曲】回復我方主戰者 1 點。",
          en: "Fanfare: Restore 1 defense to your leader.",
        },
      },
    ],
    printings: [bp("bp01-001", "jp", 1), bp("bp01-001-en", "en", 1)],
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-002",
    set: "set:bp01",
    class: "royal",
    layout: "single",
    mapping: "confirmed",
    engine: "reviewed",
    faces: [
      {
        id: "f:bp01-002",
        side: "front",
        type: "follower",
        cost: 3,
        attack: 2,
        defense: 3,
        traits: ["commander"],
        name: { ja: "試作の隊長", zhHant: "試作隊長", en: "Prototype Captain" },
        effect: {
          ja: "自分の他の【兵士】フォロワーは攻撃力+1。",
          zhHant: "我方其他【士兵】從者攻擊力 +1。",
          en: "Your other Officer followers have +1 attack.",
        },
      },
    ],
    printings: [
      bp("bp01-002", "jp", 2, { rarity: "silver" }),
      bp("bp01-002-en", "en", 2, { rarity: "silver" }),
      bp("bp01-002-alt", "jp", 2, {
        variant: "alt",
        rarity: "silver",
        intId: 1102,
        cardNo: "BP01-002a",
      }),
    ],
    related: [{ to: "c:bp01-001", relation: "mentions" }],
    qa: [
      {
        id: "qa:bp01-002-1",
        number: "Q001",
        question: {
          ja: "試作の隊長が2体いる時、【兵士】は攻撃力+2ですか？",
          zhHant: "有兩張試作隊長時，士兵攻擊力是 +2 嗎？",
        },
        answer: { ja: "はい。", zhHant: "是。" },
      },
    ],
    keywords: [{ code: "fanfare", relation: "refers" }],
    coverage: "partial",
  },
  {
    id: "c:bp01-003",
    set: "set:bp01",
    class: "royal",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-003",
        side: "front",
        type: "spell",
        cost: 2,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の号令", zhHant: "試作號令", en: "Prototype Command" },
        effect: {
          ja: "【兵士】フォロワー1体を出す。",
          zhHant: "使 1 張【士兵】從者登場。",
          en: "Put a Prototype Recruit into play.",
        },
      },
    ],
    printings: [bp("bp01-003", "jp", 3), bp("bp01-003-en", "en", 3)],
    related: [{ to: "c:bp01-001", relation: "produces_token" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-010",
    set: "set:bp01",
    class: "elf",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:bp01-010",
        side: "front",
        type: "follower",
        cost: 2,
        attack: 2,
        defense: 1,
        traits: ["fairy"],
        name: { ja: "試作の妖精", zhHant: "試作妖精", en: "Prototype Fairy" },
        effect: {
          ja: "【ラストワード】カードを1枚引く。",
          zhHant: "【謝幕曲】抽 1 張卡。",
          en: "Last Words: Draw a card.",
        },
      },
    ],
    printings: [bp("bp01-010", "jp", 10)],
    keywords: [{ code: "lastword", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-011",
    set: "set:bp01",
    class: "elf",
    layout: "single",
    mapping: "pending",
    engine: "draft",
    faces: [
      {
        id: "f:bp01-011",
        side: "front",
        type: "follower",
        cost: 5,
        attack: 4,
        defense: 5,
        traits: [],
        name: { ja: "試作の森の守り手", zhHant: "試作森林守護者" },
        effect: {
          ja: "【守護】\n【ファンファーレ】自分の場に【フェアリー】がいるなら、このフォロワーは+1/+1する。",
          zhHant: "【守護】\n【入場曲】若我方場上有【妖精】，此從者 +1/+1。",
        },
      },
    ],
    printings: [bp("bp01-011", "jp", 11, { rarity: "gold" })],
    keywords: [
      { code: "ward", relation: "has" },
      { code: "fanfare", relation: "has" },
    ],
    coverage: "partial",
    qa: [
      {
        id: "qa:bp01-011-1",
        number: "Q010",
        question: { ja: "フェアリーが進化していても+1/+1しますか？" },
        answer: { ja: "はい、進化していても【フェアリー】として扱います。" },
        revisions: 2,
      },
      {
        id: "qa:bp01-011-2",
        number: "Q011",
        question: { ja: "相手の場のフェアリーは数えますか？" },
        answer: { ja: "いいえ。" },
      },
    ],
  },
  {
    id: "c:bp01-012",
    set: "set:bp01",
    class: "elf",
    layout: "single",
    mapping: "confirmed_none",
    engine: "reviewed",
    faces: [
      {
        id: "f:bp01-012",
        side: "front",
        type: "amulet",
        cost: 1,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の森の泉", zhHant: "試作森之泉" },
        effect: {
          ja: "自分のターン終了時、自分の場の【フェアリー】1体を+1/+0する。",
          zhHant: "我方回合結束時，我方場上 1 張【妖精】+1/+0。",
        },
      },
    ],
    printings: [bp("bp01-012", "jp", 12)],
  },
  {
    id: "c:bp01-020",
    set: "set:bp01",
    class: "witch",
    layout: "single",
    mapping: "confirmed",
    engine: "load_rejected",
    faces: [
      {
        id: "f:bp01-020",
        side: "front",
        type: "spell",
        cost: 3,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の火球", zhHant: "試作火球", en: "Prototype Fireball" },
        effect: {
          ja: "相手のフォロワー1体に3ダメージ。",
          zhHant: "對對手 1 張從者造成 3 點傷害。",
          en: "Deal 3 damage to an enemy follower.",
        },
        previousEffectJa: "相手のフォロワー1体に2ダメージ。",
      },
    ],
    printings: [bp("bp01-020", "jp", 20), bp("bp01-020-en", "en", 20)],
    errata: {
      id: "errata:bp01-020",
      announcedOn: "2026-03-01",
      effectiveOn: "2026-03-15",
      reasonJa: "ダメージ量の誤植を修正。",
      printings: ["p:bp01-020"],
    },
  },
  {
    id: "c:bp01-021",
    set: "set:bp01",
    class: "witch",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-021",
        side: "front",
        type: "follower",
        cost: 4,
        attack: 3,
        defense: 3,
        traits: [],
        name: { ja: "試作の魔導士", zhHant: "試作魔導士", en: "Prototype Mage" },
        effect: {
          ja: "【ファンファーレ】自分の手札のスペル1枚につき、相手のリーダーに1ダメージ。",
          zhHant: "【入場曲】我方手牌每有 1 張法術，對對手主戰者造成 1 點傷害。",
          en: "Fanfare: Deal 1 damage to the enemy leader for each spell in your hand.",
        },
        previousEffectJa:
          "【ファンファーレ】自分の手札のスペル1枚につき、相手のリーダーに2ダメージ。",
      },
    ],
    printings: [
      bp("bp01-021", "jp", 21, { rarity: "legend" }),
      bp("bp01-021-en", "en", 21, { rarity: "legend" }),
      bp("bp01-021-sp", "jp", 21, {
        variant: "signed",
        rarity: "sp",
        intId: 1121,
        cardNo: "BP01-021S",
        stamp: "stamp:champ26",
      }),
    ],
    errata: {
      id: "errata:bp01-021",
      announcedOn: "2026-09-20",
      effectiveOn: "2026-10-15",
      reasonJa: "環境調整のため。",
      printings: ["p:bp01-021", "p:bp01-021-sp"],
    },
    ban: {
      maxCopies: 1,
      effectiveFrom: "2026-04-01",
      announcedOn: "2026-03-10",
      state: "confirmed",
    },
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-030",
    set: "set:bp01",
    class: "dragon",
    layout: "double_faced",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-030a",
        side: "front",
        type: "follower",
        cost: 6,
        attack: 5,
        defense: 5,
        traits: [],
        name: { ja: "試作の竜騎士", zhHant: "試作龍騎士", en: "Prototype Dragoon" },
        effect: { ja: "【進化】：EP1", zhHant: "【進化】：EP1", en: "Evolve: EP1" },
      },
      {
        id: "f:bp01-030b",
        side: "back",
        type: "evolved",
        cost: null,
        attack: 7,
        defense: 7,
        traits: [],
        name: { ja: "試作の竜騎士", zhHant: "試作龍騎士", en: "Prototype Dragoon" },
        effect: {
          ja: "【進化時】相手のフォロワー1体に4ダメージ。",
          zhHant: "【進化時】對對手 1 張從者造成 4 點傷害。",
          en: "On evolve: Deal 4 damage to an enemy follower.",
        },
      },
    ],
    printings: [
      bp("bp01-030", "jp", 30, { rarity: "gold" }),
      bp("bp01-030-en", "en", 30, { rarity: "gold" }),
    ],
    keywords: [{ code: "evolve", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-031",
    set: "set:bp01",
    class: "dragon",
    layout: "single",
    mapping: "confirmed",
    engine: "reviewed",
    enBlock: ["unconfirmed_mapping"],
    faces: [
      {
        id: "f:bp01-031",
        side: "front",
        type: "follower",
        cost: 8,
        attack: 8,
        defense: 8,
        traits: [],
        name: { ja: "試作の古龍", zhHant: "試作古龍", en: "Prototype Elder Dragon" },
        effect: {
          ja: "守護\nファンファーレ：合成テスト処理で相手フォロワー全員に3点を記録する。",
          zhHant: "守護\n入場：合成測試程序對每位敵方從者記錄3點傷害。",
          en: "Ward\nFanfare: Synthetic test procedure records 3 damage for each enemy follower.",
        },
      },
    ],
    printings: [
      bp("bp01-031", "jp", 31, { rarity: "legend" }),
      bp("bp01-031-en", "en", 31, { rarity: "legend", image: "pending" }),
    ],
    ban: {
      maxCopies: 0,
      effectiveFrom: "2026-11-01",
      announcedOn: "2026-09-25",
      state: "announced",
    },
    keywords: [
      { code: "ward", relation: "has" },
      { code: "fanfare", relation: "has" },
    ],
    coverage: "complete",
  },
  {
    id: "c:bp01-040",
    set: "set:bp01",
    class: "nightmare",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:bp01-040",
        side: "front",
        type: "follower",
        cost: 2,
        attack: 1,
        defense: 2,
        traits: [],
        name: { ja: "試作の亡霊", zhHant: "試作亡靈" },
        effect: {
          ja: "【ラストワード】試作の亡霊トークン1体を出す。",
          zhHant: "【謝幕曲】使 1 張試作亡靈衍生物登場。",
        },
      },
    ],
    printings: [bp("bp01-040", "jp", 40, { image: "withdrawn" })],
    related: [{ to: "c:token-001", relation: "produces_token" }],
    keywords: [{ code: "lastword", relation: "has" }],
    coverage: "none",
  },
  {
    id: "c:bp01-041",
    set: "set:bp01",
    class: "nightmare",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:bp01-041",
        side: "front",
        type: "follower",
        cost: 7,
        attack: 6,
        defense: 6,
        traits: [],
        name: {
          ja: "試作の魔界の門番にして終わりなき夜の支配者の右腕",
          zhHant: "試作魔界門衛暨無盡之夜支配者的右手",
        },
        effect: {
          ja: "【ファンファーレ】自分の墓場のカードが10枚以上なら、このフォロワーは【守護】を持つ。",
          zhHant: "【入場曲】若我方墓場有 10 張以上的卡，此從者獲得【守護】。",
        },
      },
    ],
    printings: [bp("bp01-041", "jp", 41, { rarity: "gold", image: "missing" })],
    keywords: [
      { code: "fanfare", relation: "has" },
      { code: "ward", relation: "grants" },
    ],
    coverage: "partial",
  },
  {
    id: "c:bp01-050",
    set: "set:bp01",
    class: "bishop",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-050",
        side: "front",
        type: "amulet",
        cost: 2,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の祈り", zhHant: "試作祈禱", en: "Prototype Prayer" },
        effect: {
          ja: "【カウントダウン2】\n【ラストワード】自分のリーダーを3回復する。",
          zhHant: "【倒數 2】\n【謝幕曲】回復我方主戰者 3 點。",
          en: "Countdown 2\nLast Words: Restore 3 defense to your leader.",
        },
      },
    ],
    printings: [bp("bp01-050", "jp", 50), bp("bp01-050-en", "en", 50)],
    keywords: [{ code: "lastword", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-051",
    set: "set:bp01",
    class: "bishop",
    layout: "single",
    mapping: "confirmed",
    engine: "draft",
    faces: [
      {
        id: "f:bp01-051",
        side: "front",
        type: "follower",
        cost: 3,
        attack: 1,
        defense: 4,
        traits: [],
        name: { ja: "試作の聖堂騎士", zhHant: "試作聖堂騎士", en: "Prototype Templar" },
        effect: { ja: "【守護】", zhHant: "【守護】", en: "Ward" },
      },
    ],
    printings: [bp("bp01-051", "jp", 51), bp("bp01-051-en", "en", 51)],
    keywords: [{ code: "ward", relation: "has" }],
    coverage: "complete",
    aliases: [
      { lang: "ja", text: "テンプラー" },
      { lang: "zh-Hant", text: "聖騎" },
    ],
  },
  {
    id: "c:bp01-060",
    set: "set:bp01",
    class: "witch",
    layout: "single",
    mapping: "confirmed",
    engine: "reviewed",
    faces: [
      {
        id: "f:bp01-060",
        side: "front",
        type: "follower",
        cost: 3,
        attack: 3,
        defense: 2,
        traits: ["machine", "artifact"],
        name: {
          ja: "合成カード・歯車パネル",
          zhHant: "合成卡片・齒輪面板",
          en: "Synthetic Gear Panel",
        },
        effect: {
          ja: "【ファンファーレ】【アーティファクト】1枚を手札に加える。",
          zhHant: "【入場曲】將 1 張【神器】加入手牌。",
          en: "Fanfare: Add an Artifact card to your hand.",
        },
      },
    ],
    printings: [bp("bp01-060", "jp", 60), bp("bp01-060-en", "en", 60)],
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-061",
    set: "set:bp01",
    class: "witch",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-061",
        side: "front",
        type: "spell",
        cost: 0,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の再起動", zhHant: "試作重啟", en: "Prototype Reboot" },
        effect: {
          ja: "自分の【機械】フォロワー1体を手札に戻す。",
          zhHant: "將我方 1 張【機械】從者返回手牌。",
          en: "Return an allied Machina follower to your hand.",
        },
      },
    ],
    printings: [bp("bp01-061", "jp", 61), bp("bp01-061-en", "en", 61)],
    related: [{ to: "c:bp01-060", relation: "mentions" }],
  },
  {
    id: "c:bp01-070",
    set: "set:bp01",
    class: "witch",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:bp01-070",
        side: "front",
        type: "follower",
        cost: 1,
        attack: 1,
        defense: 2,
        traits: [],
        name: { ja: "試作の魔女見習い" },
        effect: {
          ja: "【ファンファーレ】自分のリーダーが【スペル】を今ターン使っていたなら、カードを1枚引く。",
          en: "Fanfare: If your leader used a spell this turn, draw a card.",
        },
      },
    ],
    printings: [bp("bp01-070", "jp", 70)],
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01en-090",
    set: "set:bp01",
    class: "royal",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:bp01en-090",
        side: "front",
        type: "follower",
        cost: 2,
        attack: 2,
        defense: 2,
        traits: ["soldier"],
        name: { ja: "", en: "Prototype Squire" },
        effect: { ja: "", en: "Fanfare: Give an allied Officer follower +1/+0." },
      },
    ],
    printings: [bp("bp01en-090", "en", 90)],
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "none",
  },
  {
    id: "c:sd01-001",
    set: "set:sd01",
    class: "royal",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:sd01-001",
        side: "front",
        type: "leader",
        cost: null,
        attack: null,
        defense: 20,
        traits: [],
        name: { ja: "試作の王", zhHant: "試作之王", en: "Prototype King" },
        effect: { ja: "", zhHant: "", en: "" },
      },
    ],
    printings: [
      {
        id: "p:sd01-001",
        region: "jp",
        cardNo: "SD01-L01",
        intId: 2001,
        variant: "standard",
        rarity: "bronze",
        product: "prod:sd01-jp",
      },
      {
        id: "p:sd01-001-en",
        region: "en",
        cardNo: "SD01EN-L01",
        intId: 6001,
        variant: "standard",
        rarity: "bronze",
        product: "prod:sd01-en",
      },
    ],
  },
  {
    id: "c:sd01-002",
    set: "set:sd01",
    class: "royal",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:sd01-002",
        side: "front",
        type: "follower",
        cost: 2,
        attack: 2,
        defense: 2,
        traits: ["soldier"],
        name: { ja: "試作の槍兵", zhHant: "試作槍兵", en: "Prototype Lancer" },
        effect: { ja: "", zhHant: "", en: "" },
      },
    ],
    printings: [
      {
        id: "p:sd01-002",
        region: "jp",
        cardNo: "SD01-002",
        intId: 2002,
        variant: "standard",
        rarity: "bronze",
        product: "prod:sd01-jp",
      },
      {
        id: "p:sd01-002-en",
        region: "en",
        cardNo: "SD01EN-002",
        intId: 6002,
        variant: "standard",
        rarity: "bronze",
        product: "prod:sd01-en",
      },
      {
        id: "p:sd01-002-bp",
        region: "jp",
        cardNo: "BP01-004",
        intId: 1004,
        variant: "standard",
        rarity: "bronze",
        product: "prod:bp01-jp",
      },
    ],
    coverage: "complete",
  },
  {
    id: "c:pr-001",
    set: "set:pr",
    class: "elf",
    layout: "single",
    mapping: "unmapped",
    engine: "missing_dsl",
    faces: [
      {
        id: "f:pr-001",
        side: "front",
        type: "follower",
        cost: 3,
        attack: 3,
        defense: 3,
        traits: ["fairy"],
        name: { ja: "試作の妖精王", zhHant: "試作妖精王" },
        effect: {
          ja: "【ファンファーレ】自分の場の【フェアリー】すべてを+1/+1する。",
          zhHant: "【入場曲】我方場上所有【妖精】+1/+1。",
        },
      },
    ],
    printings: [
      {
        id: "p:pr-001",
        region: "jp",
        cardNo: "PR-001",
        intId: 3001,
        variant: "standard",
        rarity: "sp",
        product: "prod:pr-jp",
      },
    ],
    keywords: [{ code: "fanfare", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:token-001",
    set: "set:bp01",
    class: "nightmare",
    layout: "single",
    mapping: "unmapped",
    engine: "engine_passed",
    faces: [
      {
        id: "f:token-001",
        side: "front",
        type: "token",
        cost: 1,
        attack: 1,
        defense: 1,
        traits: [],
        name: { ja: "試作の亡霊トークン", zhHant: "試作亡靈衍生物" },
        effect: { ja: "", zhHant: "" },
      },
    ],
    printings: [
      {
        id: "p:token-001",
        region: "jp",
        cardNo: "BP01-T01",
        intId: 1901,
        variant: "standard",
        rarity: "bronze",
        product: "prod:bp01-jp",
      },
    ],
  },
  {
    id: "c:bp01-080",
    set: "set:bp01",
    class: null,
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-080",
        side: "front",
        type: "spell",
        cost: 1,
        attack: null,
        defense: null,
        traits: [],
        name: { ja: "試作の共通魔法", zhHant: "試作共通魔法", en: "Prototype Neutral Spell" },
        effect: { ja: "カードを1枚引く。", zhHant: "抽 1 張卡。", en: "Draw a card." },
      },
    ],
    printings: [bp("bp01-080", "jp", 80), bp("bp01-080-en", "en", 80)],
    coverage: "complete",
  },
  {
    id: "c:bp01-081",
    set: "set:bp01",
    class: "dragon",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-081",
        side: "front",
        type: "follower",
        cost: 2,
        attack: 2,
        defense: 1,
        traits: [],
        name: { ja: "試作の子竜", zhHant: "試作幼龍", en: "Prototype Whelp" },
        effect: {
          ja: "【進化】：EP1\n進化後、試作の成竜になる。",
          zhHant: "【進化】：EP1\n進化後成為試作成龍。",
          en: "Evolve: EP1\nEvolves into Prototype Drake.",
        },
      },
    ],
    printings: [bp("bp01-081", "jp", 81), bp("bp01-081-en", "en", 81)],
    related: [{ to: "c:bp01-082", relation: "evolves_to" }],
    keywords: [{ code: "evolve", relation: "has" }],
    coverage: "complete",
  },
  {
    id: "c:bp01-082",
    set: "set:bp01",
    class: "dragon",
    layout: "single",
    mapping: "confirmed",
    engine: "engine_passed",
    faces: [
      {
        id: "f:bp01-082",
        side: "front",
        type: "evolved",
        cost: null,
        attack: 4,
        defense: 3,
        traits: [],
        name: { ja: "試作の成竜", zhHant: "試作成龍", en: "Prototype Drake" },
        effect: {
          ja: "【進化時】カードを1枚引く。",
          zhHant: "【進化時】抽 1 張卡。",
          en: "On evolve: Draw a card.",
        },
      },
    ],
    printings: [bp("bp01-082", "jp", 82), bp("bp01-082-en", "en", 82)],
    coverage: "complete",
  },
]

export const SYNTHETIC_VOCABULARY: Vocabulary = {
  classes: CLASSES,
  types: TYPES,
  rarities: RARITIES,
  traits: TRAITS,
}
