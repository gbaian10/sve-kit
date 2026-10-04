import {
  canonical,
  compareCodePoints,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
  utf8,
} from "../../src/data/format-v1/json"
import {
  columns,
  definition,
  primaryKey,
  requiredTypes,
  rowType,
  tables,
} from "../../src/data/format-v1/schema"
import { bucket, digest, hex, sha256 } from "../../src/data/format-v1/sha256"
import {
  type Card,
  CARDS,
  type Face,
  type Family,
  KEYWORDS,
  type Printing,
  PROFILE,
  type Region,
  SETS,
  STAMPS,
  SYNTHETIC_VOCABULARY,
  type Text,
  type Vocabulary,
} from "./cards"

export interface ImageRequest {
  readonly width: number
  readonly height: number
  readonly seed: number
  /** A source file to resize, or null for a synthetic placeholder. */
  readonly source: string | null
}

type ImageEncoder = (request: ImageRequest) => Promise<Uint8Array>

export interface BuildOptions {
  readonly encodeImage: ImageEncoder
  readonly cards?: readonly Card[]
  readonly families?: Record<string, Family>
  readonly vocabulary?: Vocabulary
  /** Synthetic-only extras (keywords, symbol, stamp, route rows); off for real data. */
  readonly synthetic?: boolean
  readonly dataVersion?: string
  readonly publishedAt?: string
}

export interface BuiltSnapshot {
  /** Every file of the CDN root, keyed by its path. */
  readonly files: Map<string, Uint8Array>
  readonly manifest: JsonObject
  readonly manifestPath: string
  readonly counts: Readonly<Record<string, number>>
}

const FORMAT = "2.0.0"
const AS_OF = "2026-09-29"
const IMAGE_SIZES = [
  { key: "art_m", purpose: "art", max_width: 384, max_height: 288 },
  { key: "art_s", purpose: "art", max_width: 160, max_height: 120 },
  { key: "card_l", purpose: "card", max_width: 459, max_height: 641 },
  { key: "card_m", purpose: "card", max_width: 320, max_height: 447 },
  { key: "card_s", purpose: "card", max_width: 128, max_height: 179 },
] as const
const LANGS = { ja: "ja", zhHant: "zh-Hant", en: "en" } as const
const ARTISTS = ["artist:aoi", "artist:kuro", "artist:shiro"]

type Owner =
  | { readonly kind: "home_set"; readonly id: string }
  | { readonly kind: "global"; readonly id: null }
type Partition = "bootstrap" | "detail" | "history"
const GLOBAL: Owner = { kind: "global", id: null }
const home = (id: string): Owner => ({ kind: "home_set", id })

function textId(lang: string, text: string): string {
  return `t:${lang}:${hex(sha256(utf8(text))).slice(0, 16)}`
}

// null < string < integer, then by value: the order the reader requires between rows.
function rank(value: JsonValue | undefined): number {
  return value === null || value === undefined ? 0 : typeof value === "string" ? 1 : 2
}
function compareCell(a: JsonValue | undefined, b: JsonValue | undefined): number {
  const ra = rank(a)
  const rb = rank(b)
  if (ra !== rb) return ra - rb
  if (typeof a === "string" && typeof b === "string") return compareCodePoints(a, b)
  if (typeof a === "number" && typeof b === "number") return a - b
  return 0
}
function byFields(fields: readonly string[]) {
  return (a: JsonObject, b: JsonObject): number => {
    for (const field of fields) {
      const result = compareCell(a[field], b[field])
      if (result !== 0) return result
    }
    return 0
  }
}
const sortRows = (rows: JsonObject[], fields: readonly string[]): JsonObject[] =>
  [...rows].sort(byFields(fields))

interface Fragment {
  readonly table: string
  readonly owner: Owner
  readonly partition: Partition
  readonly rows: JsonObject[]
  readonly bucket?: number
}

/** Accumulates logical rows per (table, owner, partition) and assembles the transport files. */
class Builder {
  readonly units = new Map<string, { lang: string; text: string; bootstrap: boolean }>()
  readonly fragments = new Map<string, Fragment>()
  readonly translationRows = new Map<string, JsonObject>()

  unit(lang: string, text: string, bootstrap = false): string {
    const id = textId(lang, text)
    const existing = this.units.get(id)
    if (existing) {
      if (bootstrap) existing.bootstrap = true
    } else {
      this.units.set(id, { lang, text, bootstrap })
    }
    return id
  }

  push(table: string, owner: Owner, partition: Partition, row: JsonObject): void {
    const key = `${table}|${owner.kind}|${owner.id ?? ""}|${partition}`
    const fragment = this.fragments.get(key) ?? { table, owner, partition, rows: [] }
    fragment.rows.push(row)
    this.fragments.set(key, fragment)
  }

  /** A chosen translation row plus the FieldTranslation entry the referencing row embeds. */
  translation(
    idBase: string,
    field: string,
    sourceUnit: string,
    targetLang: string,
    text: string,
    origin: string,
    authority: string,
    lowConfidence: boolean,
    basis: string,
    bootstrap: boolean,
  ): JsonObject {
    const id = `tr:${idBase}:${field}:${targetLang}`
    const unit = this.unit(targetLang, text, bootstrap)
    this.translationRows.set(id, {
      id,
      source_unit_id: sourceUnit,
      target_lang: targetLang,
      text_unit_id: unit,
      origin,
      authority,
      low_confidence: lowConfidence,
      bootstrap,
    })
    return { field, ordinal: null, target_lang: targetLang, translation_id: id, basis }
  }

  /** zh-Hant and English translations of a label-like text (names of vocabulary, sets, keywords). */
  labelTranslations(idBase: string, field: string, text: Text, bootstrap: boolean): JsonObject[] {
    const source = this.unit(LANGS.ja, text.ja, bootstrap)
    const out: JsonObject[] = []
    if (text.zhHant !== undefined) {
      out.push(
        this.translation(
          idBase,
          field,
          source,
          LANGS.zhHant,
          text.zhHant,
          "project",
          "unofficial",
          false,
          "own_source",
          bootstrap,
        ),
      )
    }
    if (text.en !== undefined) {
      out.push(
        this.translation(
          idBase,
          field,
          source,
          LANGS.en,
          text.en,
          "project",
          "unofficial",
          false,
          "own_source",
          bootstrap,
        ),
      )
    }
    return sortRows(out, ["field", "ordinal", "target_lang"])
  }
}

function regionOf(card: Card, region: Region): boolean {
  return card.printings.some((printing) => printing.region === region)
}

function effectSections(builder: Builder, lang: string, effect: string): JsonObject[] {
  if (effect === "") return []
  return effect
    .split("\n")
    .map((line, ordinal) => ({ ordinal, text_unit_id: builder.unit(lang, line), kind: "rule" }))
}

interface RevisionPlan {
  readonly id: string
  readonly region: Region
  readonly revision: number
  readonly lang: string
  readonly name: string
  readonly effect: string
  readonly current: boolean
  readonly effectiveFrom: string | null
  readonly effectiveUntil: string | null
  readonly changeKind: string
  readonly correctedFrom: string | null
}

function revisionPlans(card: Card, face: Face): RevisionPlan[] {
  const plans: RevisionPlan[] = []
  const errata = card.errata
  if (regionOf(card, "jp") && face.name.ja !== "") {
    if (face.previousEffectJa !== undefined && errata) {
      const effective = errata.effectiveOn <= AS_OF
      plans.push(
        {
          id: `r:${face.id}:jp:1`,
          region: "jp",
          revision: 1,
          lang: LANGS.ja,
          name: face.name.ja,
          effect: face.previousEffectJa,
          current: !effective,
          effectiveFrom: null,
          effectiveUntil: errata.effectiveOn,
          changeKind: "initial",
          correctedFrom: null,
        },
        {
          id: `r:${face.id}:jp:2`,
          region: "jp",
          revision: 2,
          lang: LANGS.ja,
          name: face.name.ja,
          effect: face.effect.ja,
          current: effective,
          effectiveFrom: errata.effectiveOn,
          effectiveUntil: null,
          changeKind: "errata",
          correctedFrom: face.previousEffectJa,
        },
      )
    } else {
      plans.push({
        id: `r:${face.id}:jp:1`,
        region: "jp",
        revision: 1,
        lang: LANGS.ja,
        name: face.name.ja,
        effect: face.effect.ja,
        current: true,
        effectiveFrom: null,
        effectiveUntil: null,
        changeKind: "initial",
        correctedFrom: null,
      })
    }
  }
  if (regionOf(card, "en") && face.name.en !== undefined) {
    plans.push({
      id: `r:${face.id}:en:1`,
      region: "en",
      revision: 1,
      lang: LANGS.en,
      name: face.name.en,
      effect: face.effect.en ?? "",
      current: true,
      effectiveFrom: null,
      effectiveUntil: null,
      changeKind: "initial",
      correctedFrom: null,
    })
  }
  return plans
}

function supportFor(card: Card): JsonObject {
  const status = card.engine
  const missing = status === "missing_dsl"
  const reasons = {
    missing_dsl: ["no_formal_candidate"],
    draft: ["draft_only"],
    reviewed: ["awaiting_engine"],
    engine_passed: [],
    load_rejected: ["schema_rejected"],
  }[status]
  return {
    status,
    dsl_status: missing
      ? null
      : status === "engine_passed"
        ? "verified"
        : status === "load_rejected"
          ? "disputed"
          : status,
    dsl_version: missing ? null : "1.0",
    dsl_id: missing ? null : `dsl:${card.id.slice(2)}`,
    validation_state: status === "engine_passed" ? "fresh" : "not_applicable",
    reasons,
    reason_detail: null,
    program_ref: null,
    ruling_revision_ids: [],
  }
}

function addVocabulary(builder: Builder, kind: string, code: string, text: Text): void {
  builder.push("vocabulary", GLOBAL, "bootstrap", {
    kind,
    code,
    label_unit_id: builder.unit(LANGS.ja, text.ja, true),
    active: true,
    translations: builder.labelTranslations(`vocab:${kind}:${code}`, "label", text, true),
  })
  for (const [lang, value] of [
    [LANGS.ja, text.ja],
    [LANGS.zhHant, text.zhHant],
    [LANGS.en, text.en],
  ] as const) {
    if (value === undefined) continue
    builder.push("search_alias", GLOBAL, "bootstrap", {
      kind,
      code,
      lang,
      text: value,
      normalized: value.toLowerCase(),
    })
  }
}

function addSets(builder: Builder, families: Record<string, Family>): void {
  for (const [id, set] of Object.entries(families)) {
    builder.push("product_family", GLOBAL, "bootstrap", {
      id,
      code: set.code,
      public_code: set.publicCode,
      kind: set.kind,
      name_unit_id: builder.unit(LANGS.ja, set.name.ja, true),
      translations: builder.labelTranslations(`family:${set.code}`, "name", set.name, true),
    })
    for (const region of set.regions) {
      const released = set.code === "pr" ? null : region === "jp" ? "2026-02-01" : "2026-04-01"
      builder.push("product", GLOBAL, "bootstrap", {
        id: `prod:${set.code}-${region}`,
        family_id: id,
        region,
        product_code:
          set.code === "pr" ? null : region === "jp" ? set.publicCode : `${set.publicCode}EN`,
        name_unit_id: builder.unit(
          region === "en" && set.name.en !== undefined ? LANGS.en : LANGS.ja,
          region === "en" && set.name.en !== undefined ? set.name.en : set.name.ja,
          true,
        ),
        product_type: set.kind,
        released_on: released,
        date_precision: released === null ? "unknown" : "day",
        date_raw: null,
        translations:
          region === "jp"
            ? builder.labelTranslations(`product:${set.code}-${region}`, "name", set.name, true)
            : [],
      })
    }
  }
}

function addStamp(builder: Builder): void {
  for (const [id, stamp] of Object.entries(STAMPS)) {
    builder.push("stamp", GLOBAL, "bootstrap", {
      id,
      code: stamp.code,
      series_code: stamp.series,
      text_raw: stamp.text,
      kind: stamp.kind,
      displayed_year: stamp.year,
      review_level: "sampled",
    })
  }
  addVocabulary(builder, "stamp_series", "championship", {
    ja: "チャンピオンシップ",
    zhHant: "冠軍賽",
    en: "Championship",
  })
}

function addKeywords(builder: Builder): void {
  for (const [code, keyword] of Object.entries(KEYWORDS)) {
    builder.push("keyword", GLOBAL, "bootstrap", {
      id: `kw:${code}`,
      code,
      kind: keyword.kind,
      name_unit_id: builder.unit(LANGS.ja, keyword.name.ja, true),
      definition_unit_id: builder.unit(LANGS.ja, keyword.definition.ja),
      actions: [],
      translations: builder.labelTranslations(`keyword:${code}`, "name", keyword.name, true),
    })
    for (const [lang, value] of [
      [LANGS.ja, keyword.name.ja],
      [LANGS.zhHant, keyword.name.zhHant],
      [LANGS.en, keyword.name.en],
    ] as const) {
      if (value !== undefined)
        builder.push("search_alias", GLOBAL, "bootstrap", {
          kind: "keyword",
          code,
          lang,
          text: value,
          normalized: value.toLowerCase(),
        })
    }
  }
  const schema = {
    parameters: [{ name: "amount", uint: { minimum: 0, maximum: 9 }, variables: ["X"] }],
  }
  builder.push("text_symbol", GLOBAL, "detail", {
    id: "sym:ep",
    code: "ep",
    parameter_schema: schema,
    keyword_id: "kw:evolve",
    spellings: [
      {
        lang: LANGS.en,
        literal_prefix: "EP",
        literal_suffix: "",
        parameter_name: "amount",
        parse_kind: "uint",
      },
      {
        lang: LANGS.ja,
        literal_prefix: "EP",
        literal_suffix: "",
        parameter_name: "amount",
        parse_kind: "uint",
      },
      {
        lang: LANGS.zhHant,
        literal_prefix: "EP",
        literal_suffix: "",
        parameter_name: null,
        parse_kind: "literal",
      },
    ],
    localizations: [
      {
        lang: LANGS.en,
        name: "Evolve points",
        tooltip: "Points spent to evolve",
        copy_pattern: "EP{amount}",
      },
      {
        lang: LANGS.ja,
        name: "進化ポイント",
        tooltip: "進化に使うポイント",
        copy_pattern: "EP{amount}",
      },
      { lang: LANGS.zhHant, name: "進化點數", tooltip: "進化用的點數", copy_pattern: "EP{amount}" },
    ],
  })
}

function addRules(builder: Builder): void {
  builder.push("rules_profile", GLOBAL, "bootstrap", {
    id: PROFILE.id,
    region: PROFILE.region,
    format_code: PROFILE.formatCode,
    name_unit_id: builder.unit(LANGS.ja, PROFILE.name.ja, true),
    revisions: [
      {
        id: PROFILE.revisionId,
        effective_from: PROFILE.effectiveFrom,
        effective_until: null,
        cr_version_id: null,
        default_copy_limit: 3,
        construction_rules_ref: null,
      },
    ],
  })
}

interface CardContext {
  readonly builder: Builder
  readonly card: Card
  readonly owner: Owner
  readonly encodeImage: ImageEncoder
  readonly images: Map<string, Uint8Array>
  readonly seed: number
  readonly vocabulary: Vocabulary
  readonly artistIds: Map<string, string>
  readonly synthetic: boolean
}

interface FaceRevisions {
  readonly revisions: Map<string, JsonObject>
  readonly currentIds: Set<string>
}

function addFaces(ctx: CardContext): FaceRevisions {
  const { builder, card, owner } = ctx
  const currentByFace = new Map<string, JsonObject[]>()
  const revisions = new Map<string, JsonObject>()
  const currentIds = new Set<string>()
  card.faces.forEach((face, ordinal) => {
    const plans = revisionPlans(card, face)
    const current: JsonObject[] = []
    for (const plan of plans) {
      const nameUnit = builder.unit(plan.lang, plan.name, true)
      const effectUnit = builder.unit(plan.lang, plan.effect)
      const translations: JsonObject[] = []
      if (plan.region === "jp") {
        if (face.name.zhHant !== undefined) {
          translations.push(
            builder.translation(
              `${face.id}:jp`,
              "name",
              nameUnit,
              LANGS.zhHant,
              face.name.zhHant,
              "project",
              "unofficial",
              false,
              "own_source",
              true,
            ),
          )
        }
        if (face.effect.zhHant !== undefined && plan.current) {
          translations.push(
            builder.translation(
              `${face.id}:jp`,
              "effect",
              effectUnit,
              LANGS.zhHant,
              face.effect.zhHant,
              "project",
              "unofficial",
              false,
              "own_source",
              false,
            ),
          )
        }
        if (face.name.en !== undefined && plan.current) {
          const official = regionOf(card, "en")
          translations.push(
            builder.translation(
              `${face.id}:jp`,
              "name",
              nameUnit,
              LANGS.en,
              face.name.en,
              official ? "official" : "machine",
              official ? "sve_official" : "unofficial",
              !official,
              official ? "official_counterpart" : "own_source",
              true,
            ),
          )
          if (face.effect.en !== undefined) {
            translations.push(
              builder.translation(
                `${face.id}:jp`,
                "effect",
                effectUnit,
                LANGS.en,
                face.effect.en,
                official ? "official" : "machine",
                official ? "sve_official" : "unofficial",
                !official,
                official ? "official_counterpart" : "own_source",
                false,
              ),
            )
          }
        }
      } else if (face.name.zhHant !== undefined && face.name.ja !== "") {
        // The EN face shows the same Traditional Chinese rows the JP face chose.
        const jaName = builder.unit(LANGS.ja, face.name.ja, true)
        translations.push(
          builder.translation(
            `${face.id}:jp`,
            "name",
            jaName,
            LANGS.zhHant,
            face.name.zhHant,
            "project",
            "unofficial",
            false,
            "shared_jp",
            true,
          ),
        )
        if (face.effect.zhHant !== undefined) {
          translations.push(
            builder.translation(
              `${face.id}:jp`,
              "effect",
              builder.unit(LANGS.ja, face.effect.ja),
              LANGS.zhHant,
              face.effect.zhHant,
              "project",
              "unofficial",
              false,
              "shared_jp",
              false,
            ),
          )
        }
      }
      const row: JsonObject = {
        id: plan.id,
        face_id: face.id,
        region: plan.region,
        revision: plan.revision,
        effective_from: plan.effectiveFrom,
        effective_until: plan.effectiveUntil,
        temporal_status:
          plan.effectiveFrom === null && plan.effectiveUntil === null ? "unknown" : "known",
        change_kind: plan.changeKind,
        name_unit_id: nameUnit,
        effect_unit_id: effectUnit,
        class_code: card.class,
        type_code: face.type,
        cost: face.cost,
        attack: face.attack,
        defense: face.defense,
        traits: [...face.traits].sort(compareCodePoints),
        titles: [],
        special_kinds: [],
        sections: effectSections(builder, plan.lang, plan.effect),
        translations: sortRows(translations, ["field", "ordinal", "target_lang"]),
        corrections:
          plan.correctedFrom === null
            ? []
            : [
                {
                  field: "effect",
                  corrected_from: plan.correctedFrom,
                  is_corrected: true,
                  reason: card.errata?.reasonJa ?? "",
                  source_url: null,
                },
              ],
      }
      builder.push("face_revision", owner, plan.current ? "bootstrap" : "history", row)
      revisions.set(plan.id, row)
      if (plan.current) currentIds.add(plan.id)
      if (plan.current)
        current.push({
          region: plan.region,
          revision_id: plan.id,
          basis: plan.effectiveFrom === null ? "latest_adopted_wording" : "dated_effective",
        })
    }
    currentByFace.set(face.id, sortRows(current, ["region"]))
    builder.push("face", owner, "bootstrap", {
      id: face.id,
      card_id: card.id,
      ordinal,
      side: face.side,
      current: currentByFace.get(face.id) ?? [],
      wording: [],
    })
    for (const plan of plans) {
      if (!plan.current) continue
      const rulesName = `rn:${plan.region}:${face.id}`
      builder.push("rules_name", GLOBAL, "detail", {
        id: rulesName,
        region: plan.region,
        official_name: plan.name,
      })
      builder.push("face_rules_name", GLOBAL, "detail", {
        face_id: face.id,
        region: plan.region,
        rules_name_id: rulesName,
        role: "primary",
      })
    }
  })
  return { revisions, currentIds }
}

async function addImages(
  ctx: CardContext,
  printing: Printing,
  face: Face,
  ordinal: number,
): Promise<void> {
  const { builder, owner, encodeImage, images } = ctx
  const source = printing.imagePaths?.[ordinal] ?? null
  const state =
    printing.imagePaths !== undefined && source === null
      ? "missing"
      : (printing.image ?? "approved")
  const imageId = `img:${printing.id.slice(2)}:${String(ordinal)}`
  builder.push("image_asset", GLOBAL, "detail", {
    id: imageId,
    origin: "official",
    publication_state:
      state === "withdrawn" ? "withdrawn" : state === "pending" ? "pending" : "approved",
    withdrawal_reason: state === "withdrawn" ? "Synthetic withdrawal for the fixture" : null,
    source_src_raw: `/synthetic/${printing.cardNo}-${String(ordinal)}.png`,
    source_url: `https://example.invalid/cards/${printing.cardNo}-${String(ordinal)}.png`,
    availability: state === "missing" ? "missing" : state === "pending" ? "unfetched" : "available",
    width: state === "missing" ? null : 459,
    height: state === "missing" ? null : 641,
    format: state === "missing" ? null : "png",
  })
  const media: JsonObject = {
    printing_id: printing.id,
    face_id: face.id,
    image_id: imageId,
    publication_state:
      state === "withdrawn" ? "withdrawn" : state === "pending" ? "pending" : "approved",
    availability: state === "missing" ? "missing" : state === "pending" ? "unfetched" : "available",
    withdrawal_reason: state === "withdrawn" ? "Synthetic withdrawal for the fixture" : null,
    card_version: state === "approved" ? 1 : null,
    art_version: state === "approved" ? 1 : null,
    variants: [],
  }
  builder.push("printing_image", owner, "detail", media)
  if (state !== "approved") return
  for (const size of IMAGE_SIZES) {
    const bytes = await encodeImage({
      width: size.max_width,
      height: size.max_height,
      seed:
        ctx.seed * 8 +
        ordinal * 3 +
        (printing.variant === "alt" ? 1 : printing.variant === "signed" ? 2 : 0),
      source,
    })
    images.set(
      `images/${size.key}/${String(printing.intId)}${ordinal === 0 ? "" : `-f${String(ordinal)}`}.webp`,
      bytes,
    )
    ;(media["variants"] as JsonObject[]).push({
      size_key: size.key,
      width: size.max_width,
      height: size.max_height,
    })
    builder.push("image_variant", GLOBAL, "detail", {
      image_id: imageId,
      size_key: size.key,
      format: "webp",
      width: size.max_width,
      height: size.max_height,
      bytes: bytes.length,
    })
  }
}

function artistFor(ctx: CardContext, face: Face, index: number): string | null {
  if (face.illustrator === undefined) {
    return ctx.synthetic ? (ARTISTS[(ctx.seed + index) % ARTISTS.length] ?? "") : null
  }
  let id = ctx.artistIds.get(face.illustrator)
  if (id === undefined) {
    id = `artist:${String(ctx.artistIds.size + 1)}`
    ctx.artistIds.set(face.illustrator, id)
  }
  return id
}

function artistRows(artistId: string | null): JsonObject[] {
  return artistId === null ? [] : [{ artist_id: artistId, role: "illustrator" }]
}

async function addPrintings(ctx: CardContext, faces: FaceRevisions): Promise<void> {
  const { builder, card, owner } = ctx
  for (const [index, printing] of card.printings.entries()) {
    const frame = printing.variant === "standard" ? "normal" : "alt"
    const facesBootstrap: JsonObject[] = []
    const facesDetail: JsonObject[] = []
    for (const [ordinal, face] of card.faces.entries()) {
      const artId = `art:${printing.id.slice(2)}:${String(ordinal)}`
      builder.push("art", owner, "detail", {
        id: artId,
        card_id: card.id,
        face_id: face.id,
        classification: printing.variant === "standard" ? "base" : "alternate",
        review_level: "unreviewed",
        regions: [printing.region],
        artists: artistRows(artistFor(ctx, face, index)),
      })
      facesBootstrap.push({
        face_id: face.id,
        art_id: artId,
        frame_code: frame,
        signed: printing.variant === "signed" ? true : null,
        embellishment_state: "unreviewed",
        stamps:
          printing.stamp === undefined
            ? []
            : [{ stamp_id: printing.stamp, position: null, color: null }],
      })
      // A printing carries the text it was printed with: printings an erratum lists were made
      // before it, so they show revision 1, not the current wording.
      const prefix = `r:${face.id}:${printing.region}:`
      const printedBeforeErrata =
        card.errata !== undefined && card.errata.printings.includes(printing.id)
      const revisionId = printedBeforeErrata
        ? `${prefix}1`
        : [...faces.revisions.keys()].find(
            (id) => id.startsWith(prefix) && faces.currentIds.has(id),
          )
      const revision = revisionId === undefined ? undefined : faces.revisions.get(revisionId)
      const lang = printing.region === "jp" ? LANGS.ja : LANGS.en
      const nameText = printing.region === "jp" ? face.name.ja : (face.name.en ?? "")
      facesDetail.push({
        face_ordinal: ordinal,
        printed_name_unit_id: revision ? builder.unit(lang, nameText, true) : null,
        printed_effect_unit_id: revision ? (revision["effect_unit_id"] ?? null) : null,
        flavor_unit_id: face.flavor === undefined ? null : builder.unit(lang, face.flavor),
        printed_text_state: revision ? (index === 0 ? "verified" : "derived_no_errata") : "unknown",
        observations: revision
          ? [
              {
                revision_id: revisionId ?? null,
                state: "available",
                source_url: "https://example.invalid/synthetic-observation",
              },
            ]
          : [],
        sections: revision ? (revision["sections"] ?? []) : [],
        stamps: [],
        translations: [],
        corrections: [],
      })
      await addImages(ctx, printing, face, ordinal)
    }
    builder.push("printing", owner, "bootstrap", {
      id: printing.id,
      card_id: card.id,
      region: printing.region,
      card_no: printing.cardNo,
      card_no_state: "official",
      catalog_state: "official",
      listing_confidence: null,
      review_level: "unreviewed",
      reference_urls: [],
      variant_key: printing.variant,
      rarity_code: printing.rarity,
      rarity_raw:
        printing.rarity === null
          ? ""
          : (ctx.vocabulary.rarities[printing.rarity]?.ja ?? printing.rarity),
      premium: printing.premium ?? (printing.variant === "signed" ? true : null),
      serial_total: null,
      int_id: printing.intId,
      decklog_available: true,
      decklog_verification: "unverified",
      decklog_source_url: null,
      decklog_checked_on: null,
      faces: facesBootstrap,
      // Only the detail column partition carries these; the encoder picks columns by name.
      faces_detail: facesDetail,
    })
    builder.push("printing_product", owner, "bootstrap", {
      printing_id: printing.id,
      product_id: printing.product,
      available_on: null,
      date_precision: null,
      date_raw: null,
      inclusion_kind: "pack",
      note_unit_id: null,
      first_inclusion_state: index === 0 ? "first" : "reprint",
    })
  }
}

async function addCard(ctx: CardContext): Promise<void> {
  const { builder, card, owner } = ctx
  const faces = addFaces(ctx)
  await addPrintings(ctx, faces)
  const regions: JsonObject[] = []
  for (const region of ["jp", "en"] as const) {
    if (!regionOf(card, region)) continue
    const first = card.printings.find((printing) => printing.region === region)
    const mapping =
      region === "en" ? (card.mapping === "confirmed" ? "confirmed" : "unmapped") : card.mapping
    regions.push({
      region,
      release_state: "released",
      mapping_state: mapping,
      as_of: AS_OF,
      mapping_as_of: mapping === "confirmed_none" || mapping === "pending" ? "2026-09-01" : null,
      mapping_scope: mapping === "confirmed_none" ? "BP01EN" : null,
      default_printing_id: first?.id ?? null,
      default_method: "earliest_general",
      deck_role:
        card.faces[0]?.type === "leader"
          ? "leader"
          : card.faces[0]?.type === "evolved"
            ? "evolve"
            : card.faces[0]?.type === "token"
              ? "extra"
              : "main",
      debut_product_ids: first ? [first.product] : [],
      debut_state: first ? "known" : "unknown",
    })
  }
  // Alias codes are the card id without its `c:` prefix, because Code allows no colon.
  for (const alias of card.aliases ?? []) {
    builder.push("search_alias", GLOBAL, "bootstrap", {
      kind: "card",
      code: card.id.slice(2),
      lang: alias.lang,
      text: alias.text,
      normalized: alias.text.toLowerCase(),
    })
  }
  builder.push("card", owner, "bootstrap", {
    id: card.id,
    layout: card.layout,
    identity_state: "confirmed",
    home_set_id: card.set,
    faces: card.faces.map((face) => face.id),
    regions: sortRows(regions, ["region"]),
  })
  builder.push("card_engine_support", owner, "bootstrap", {
    card_id: card.id,
    shared: supportFor(card),
    overrides: [],
    region_blocks: card.enBlock
      ? [{ region: "en", reasons: [...card.enBlock].sort(compareCodePoints) }]
      : [],
  })
  for (const keyword of card.keywords ?? []) {
    builder.push("mechanic_projection", owner, "bootstrap", {
      card_id: card.id,
      keyword_id: `kw:${keyword.code}`,
      scope: "shared",
      relations: [keyword.relation],
      actions: [],
    })
  }
  if (card.coverage !== undefined && card.coverage !== "none") {
    const complete = card.coverage === "complete"
    builder.push("card_mechanic_coverage", owner, "bootstrap", {
      card_id: card.id,
      scope: "shared",
      complete_all: complete,
      complete_mode: "include",
      complete_keyword_ids: complete
        ? []
        : [...new Set((card.keywords ?? []).map((keyword) => `kw:${keyword.code}`))].sort(
            compareCodePoints,
          ),
      partial_mode: "include",
      partial_keyword_ids: [],
    })
  }
  for (const related of card.related ?? []) {
    builder.push("card_related", owner, "detail", {
      id: `rel:${card.id.slice(2)}:${related.to.slice(2)}`,
      from_card_id: card.id,
      to_card_id: related.to,
      relation: related.relation,
      suggested_count: related.relation === "produces_token" ? 3 : null,
      applicable_regions: null,
    })
  }
  for (const qa of card.qa ?? []) {
    const count = qa.revisions ?? 1
    const versionIds = Array.from({ length: count }, (_, i) => `${qa.id}:v${String(i + 1)}`)
    builder.push("qa", GLOBAL, "detail", {
      id: qa.id,
      region: "jp",
      official_number: qa.number,
      source_url: `https://example.invalid/qa/${qa.number}`,
      current_version_id: versionIds.at(-1) ?? null,
    })
    versionIds.forEach((versionId, i) => {
      const questionUnit = builder.unit(
        LANGS.ja,
        i === 0 ? qa.question.ja : `${qa.question.ja}（改訂${String(i + 1)}）`,
      )
      const answerUnit = builder.unit(
        LANGS.ja,
        i === 0 ? qa.answer.ja : `${qa.answer.ja}（改訂${String(i + 1)}）`,
      )
      const translations: JsonObject[] = []
      if (i === count - 1) {
        if (qa.question.zhHant !== undefined)
          translations.push(
            builder.translation(
              versionId,
              "question",
              questionUnit,
              LANGS.zhHant,
              qa.question.zhHant,
              "project",
              "unofficial",
              false,
              "own_source",
              false,
            ),
          )
        if (qa.answer.zhHant !== undefined)
          translations.push(
            builder.translation(
              versionId,
              "answer",
              answerUnit,
              LANGS.zhHant,
              qa.answer.zhHant,
              "project",
              "unofficial",
              false,
              "own_source",
              false,
            ),
          )
      }
      builder.push("qa_version", GLOBAL, "detail", {
        id: versionId,
        qa_id: qa.id,
        revision: i + 1,
        published_on: qa.publishedOn ?? "2026-05-01",
        updated_on: i === 0 ? null : "2026-06-01",
        date_raw: null,
        question_unit_id: questionUnit,
        answer_unit_id: answerUnit,
        state: "active",
        cards: [...(qa.cards ?? [card.id])].sort(compareCodePoints),
        translations: sortRows(translations, ["field", "ordinal", "target_lang"]),
      })
    })
  }
  if (card.errata) {
    const face = card.faces[0]
    if (face?.previousEffectJa !== undefined) {
      builder.push("errata", GLOBAL, "detail", {
        id: card.errata.id,
        region: "jp",
        official_url: `https://example.invalid/errata/${card.errata.id.slice(7)}`,
        versions: [
          {
            id: `${card.errata.id}:v1`,
            revision: 1,
            announced_on: card.errata.announcedOn,
            effective_on: card.errata.effectiveOn,
            date_raw: null,
            reason_unit_id: builder.unit(LANGS.ja, card.errata.reasonJa),
            exchange_offered: null,
            changes: [
              {
                face_id: face.id,
                field: "effect",
                before: face.previousEffectJa,
                after: face.effect.ja,
              },
            ],
            printings: [...card.errata.printings]
              .sort(compareCodePoints)
              .map((printingId) => ({ printing_id: printingId, scope: "listed" })),
          },
        ],
      })
    }
  }
  if (card.ban) {
    builder.push("restriction", GLOBAL, "bootstrap", {
      id: `restr:${card.id.slice(2)}`,
      profile_id: PROFILE.id,
      announced_on: card.ban.announcedOn,
      effective_from: card.ban.effectiveFrom,
      effective_until: null,
      kind: "copy_limit",
      state: card.ban.state,
      max_copies: card.ban.maxCopies,
      max_selected_groups: null,
      members: [
        { rules_name_id: `rn:jp:${card.faces[0]?.id ?? ""}`, choice_option: 0, deck_scope: "all" },
      ],
    })
  }
}

function fragmentsOf(builder: Builder, table: string): JsonObject[] {
  return [...builder.fragments.values()]
    .filter((fragment) => fragment.table === table)
    .flatMap((fragment) => fragment.rows)
}

/** Encode a logical object as the tuple of a fixed row or nested type. */
function encodeRow(name: string, row: JsonObject): JsonValue[] {
  const kinds = definition(name)["x-types"] as JsonObject[]
  return columns(name).map((column, index) => {
    const value = row[column]
    if (value === undefined) throw new Error(`${name}.${column} missing`)
    return encodeValue(kinds[index] ?? {}, value, `${name}.${column}`)
  })
}
function encodeValue(kind: JsonObject, value: JsonValue, where: string): JsonValue {
  if ("nullable" in kind)
    return value === null ? null : encodeValue(objectValue(kind["nullable"]), value, where)
  if ("array" in kind) {
    if (!Array.isArray(value)) throw new Error(`${where} should be an array`)
    return value.map((item) => encodeValue(objectValue(kind["array"]), item, where))
  }
  if ("ref" in kind) return encodeRow(stringValue(kind["ref"]), objectValue(value))
  return value
}

interface FileSpec {
  readonly key: string
  readonly role: "bootstrap" | "text" | "config" | "images" | "programs"
  readonly fragments: Fragment[]
  readonly dependencies: string[]
}

function fileKeyFor(fragment: Fragment): string {
  const role = ["image_asset", "printing_image", "image_variant"].includes(fragment.table)
    ? "images"
    : fragment.partition === "bootstrap"
      ? "bootstrap"
      : "text"
  const global = fragment.owner.kind === "global"
  const width =
    role === "images"
      ? global
        ? 1
        : 32
      : role === "bootstrap"
        ? global
          ? 8
          : ["BP01", "CP04"].includes(fragment.owner.id)
            ? 32
            : 64
        : global && fragment.partition === "detail"
          ? 2
          : 32
  const id = encodeURIComponent(fragment.owner.id ?? "").replace(
    /[!'()*]/g,
    (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
  )
  return `${role}/${fragment.partition}/${fragment.owner.kind}/${id || "global"}/band/${String(Math.floor((fragment.bucket ?? 0) / width))}`
}

const orderOwner = byFields(["kind", "id", "bucket", "partition"])

function buildContainer(
  spec: FileSpec,
  bootstrapRefs: ReadonlyMap<string, JsonObject>,
): JsonObject {
  const types = new Set<string>()
  const tableFragments = new Map<string, JsonObject[]>()
  for (const fragment of spec.fragments) {
    const name = rowType(fragment.table, fragment.partition)
    for (const nested of requiredTypes(name)) types.add(nested)
    const withBase =
      fragment.partition === "detail" &&
      (fragment.table === "printing" || fragment.table === "face_revision")
    const rows = fragment.rows.map((row) => encodeRow(name, row))
    const bootstrapRef = bootstrapRefs.get(fileKeyFor({ ...fragment, partition: "bootstrap" }))
    const base =
      withBase && bootstrapRef
        ? {
            file: bootstrapRef,
            table: fragment.table,
            owner: fragment.owner,
            bucket: fragment.bucket ?? 0,
            partition: "bootstrap",
          }
        : null
    const entry: JsonObject = {
      owner: fragment.owner,
      bucket: fragment.bucket ?? 0,
      partition: fragment.partition,
      base,
      columns: columns(name),
      rows,
    }
    tableFragments.set(fragment.table, [...(tableFragments.get(fragment.table) ?? []), entry])
  }
  const tablesOut: JsonObject = {}
  for (const table of tables()) {
    const list = tableFragments.get(table)
    if (!list) continue
    tablesOut[table] = [...list].sort((a, b) =>
      orderOwner(
        {
          ...(a["owner"] as JsonObject),
          bucket: a["bucket"] ?? 0,
          partition: a["partition"] ?? null,
        },
        {
          ...(b["owner"] as JsonObject),
          bucket: b["bucket"] ?? 0,
          partition: b["partition"] ?? null,
        },
      ),
    )
  }
  const typesOut: JsonObject = {}
  for (const name of [...types].sort(compareCodePoints))
    typesOut[name] = {
      columns: definition(name)["x-columns"] ?? null,
      items: definition(name)["x-types"] ?? null,
    }
  return { format_version: FORMAT, types: typesOut, tables: tablesOut }
}

function blob(value: JsonValue): { bytes: Uint8Array; hash: string; path: string } {
  const bytes = canonical(value)
  const hash = digest(bytes)
  return { bytes, hash, path: `snapshots/blobs/${hash.slice(7)}.json` }
}

export async function buildSnapshot(options: BuildOptions): Promise<BuiltSnapshot> {
  const cards = options.cards ?? CARDS
  const dataVersion = options.dataVersion ?? "20260929T000000Z-0001"
  const publishedAt = options.publishedAt ?? "2026-09-29T00:00:00Z"
  const families = options.families ?? SETS
  const vocabulary = options.vocabulary ?? SYNTHETIC_VOCABULARY
  const synthetic = options.synthetic ?? true
  const builder = new Builder()
  const images = new Map<string, Uint8Array>()
  const artistIds = new Map<string, string>()

  for (const [code, text] of Object.entries(vocabulary.classes))
    addVocabulary(builder, "class", code, text)
  for (const [code, text] of Object.entries(vocabulary.types))
    addVocabulary(builder, "type", code, text)
  for (const [code, text] of Object.entries(vocabulary.rarities))
    addVocabulary(builder, "rarity", code, text)
  for (const [code, text] of Object.entries(vocabulary.traits))
    addVocabulary(builder, "trait", code, text)
  addVocabulary(builder, "frame", "normal", { ja: "通常", zhHant: "一般", en: "Standard" })
  addVocabulary(builder, "frame", "alt", { ja: "特別", zhHant: "特別", en: "Special" })
  addSets(builder, families)
  if (synthetic) {
    addStamp(builder)
    addKeywords(builder)
  }
  addRules(builder)
  for (const [seed, card] of cards.entries()) {
    await addCard({
      builder,
      card,
      owner: home(card.set),
      encodeImage: options.encodeImage,
      images,
      seed,
      vocabulary,
      artistIds,
      synthetic,
    })
  }
  if (synthetic)
    builder.push("card_route_alias", GLOBAL, "detail", {
      namespace: "official",
      old_key: "BP01-002A",
      target_namespace: "official",
      target_key: "BP01-002a",
      reason: "renumbered",
    })
  if (synthetic) {
    builder.push("route_override", GLOBAL, "detail", {
      route_key: "BP01-002",
      printing_id: "p:bp01-002",
    })
  }
  const usedArtists = new Set(
    fragmentsOf(builder, "art").flatMap((row) =>
      (row["artists"] as JsonObject[]).map((item) => stringValue(item["artist_id"])),
    ),
  )
  for (const id of ARTISTS)
    if (usedArtists.has(id))
      builder.push("artist", GLOBAL, "detail", { id, display_name: id.slice(7) })
  for (const [name, id] of artistIds)
    builder.push("artist", GLOBAL, "detail", { id, display_name: name })

  // Text units and chosen translations land in the bootstrap or the detail partition (format §3.1).
  for (const [id, unit] of builder.units) {
    builder.push("text_unit", GLOBAL, unit.bootstrap ? "bootstrap" : "detail", {
      id,
      lang: unit.lang,
      text: unit.text,
    })
  }
  for (const row of builder.translationRows.values()) {
    const { bootstrap, ...rest } = row
    builder.push("translation", GLOBAL, bootstrap === true ? "bootstrap" : "detail", rest)
  }

  const cardsByFace = new Map(
    fragmentsOf(builder, "face").map((row) => [stringValue(row["id"]), row["card_id"] ?? null]),
  )
  const cardTables = new Set([
    "card",
    "face",
    "face_revision",
    "card_engine_support",
    "mechanic_projection",
    "card_mechanic_coverage",
    "card_related",
    "digital_link",
    "digital_link_coverage",
    "card_voice",
  ])
  const printTables = new Set(["printing", "printing_product", "printing_image"])
  const artTables = new Set(["art", "digital_art_link"])
  const grouped: Fragment[] = []
  for (const fragment of builder.fragments.values()) {
    const bins = new Map<number, JsonObject[]>()
    for (const row of fragment.rows) {
      const values = cardTables.has(fragment.table)
        ? [
            fragment.table === "card"
              ? row["id"]
              : fragment.table === "face_revision"
                ? cardsByFace.get(stringValue(row["face_id"]))
                : fragment.table === "card_related"
                  ? row["from_card_id"]
                  : row["card_id"],
          ]
        : printTables.has(fragment.table)
          ? [row[fragment.table === "printing" ? "id" : "printing_id"]]
          : artTables.has(fragment.table)
            ? [row[fragment.table === "art" ? "id" : "art_id"]]
            : ["image_asset", "image_variant"].includes(fragment.table)
              ? [row[fragment.table === "image_asset" ? "id" : "image_id"]]
              : primaryKey(fragment.table).map((key) => row[key])
      const number = bucket(
        values.map((value) => value ?? null),
        64,
      )
      bins.set(number, [...(bins.get(number) ?? []), row])
    }
    for (const [number, rows] of bins) grouped.push({ ...fragment, bucket: number, rows })
  }
  const fragments: Fragment[] = []
  for (const fragment of grouped) {
    const sorted = sortRows(fragment.rows, primaryKey(fragment.table))
    if (fragment.table === "printing") {
      fragments.push({
        ...fragment,
        rows: sorted.map(({ faces_detail: _detail, ...rest }) => rest),
      })
      fragments.push({
        ...fragment,
        partition: "detail",
        rows: sorted.map((row, index) => ({ row_index: index, faces: row["faces_detail"] ?? [] })),
      })
    } else if (fragment.table === "face_revision" && fragment.partition === "bootstrap") {
      fragments.push({
        ...fragment,
        rows: sorted.map((row) => ({
          ...row,
          translations: (row["translations"] as JsonObject[]).filter(
            (item) => item["field"] === "name",
          ),
        })),
      })
      fragments.push({
        ...fragment,
        partition: "detail",
        rows: sorted.map((row, index) => ({
          ...row,
          row_index: index,
          translations: (row["translations"] as JsonObject[]).filter(
            (item) => item["field"] !== "name",
          ),
        })),
      })
    } else {
      fragments.push({ ...fragment, rows: sorted })
    }
  }

  const specs = new Map<string, FileSpec>()
  for (const fragment of fragments) {
    const key = fileKeyFor(fragment)
    const role = key.startsWith("images/")
      ? "images"
      : key.startsWith("bootstrap/")
        ? "bootstrap"
        : "text"
    const spec = specs.get(key) ?? { key, role, fragments: [], dependencies: [] }
    spec.fragments.push(fragment)
    specs.set(key, spec)
  }

  const config: JsonObject = {
    format_version: FORMAT,
    languages: [
      { code: "en", fallback_order: ["ja"], display_name: "English" },
      { code: "ja", fallback_order: [], display_name: "日本語" },
      { code: "zh-Hant", fallback_order: ["ja"], display_name: "繁體中文" },
    ].filter((language) => [...builder.units.values()].some((unit) => unit.lang === language.code)),
    digital_endpoints: [
      {
        game: "sv1",
        card_url_template: "https://example.invalid/sv1/{official_id}/{provider_lang}",
        language_map: { ja: "ja" },
        status: "unknown",
        refresh_policy: "frozen",
      },
      {
        game: "svwb",
        card_url_template: "https://example.invalid/svwb/{official_id}/{provider_lang}",
        language_map: { ja: "ja" },
        status: "unknown",
        refresh_policy: "on_sve_release",
      },
    ],
    shop_links: [],
    image_sizes: IMAGE_SIZES.map((size) => ({ ...size })),
    search: { grammar_version: "synthetic-v1", normalizer_version: "synthetic-v1" },
    catalog_feedback_url: "https://example.invalid/feedback",
    third_party_image_policy: "mirror_reviewed",
    deck_eligibility_policy: "regional_decklog",
  }
  const programs: JsonObject = { format_version: FORMAT, entries: [] }

  const files = new Map<string, Uint8Array>()
  const fileRows: JsonObject[] = []
  const payloads = new Map<string, JsonValue>()
  const register = (
    key: string,
    role: string,
    value: JsonValue,
    rowCounts: JsonObject[],
    dependencies: JsonObject[],
  ): JsonObject => {
    const { bytes, hash, path } = blob(value)
    files.set(path, bytes)
    payloads.set(key, value)
    const row: JsonObject = {
      path,
      sha256: hash,
      bytes: bytes.length,
      compressed_bytes: { br: null, gzip: null },
      key,
      role,
      row_counts: rowCounts,
      dependencies,
    }
    fileRows.push(row)
    return { key, sha256: hash }
  }
  const counts = (container: JsonObject): JsonObject[] => {
    const out: JsonObject[] = []
    for (const [table, list] of Object.entries(objectValue(container["tables"]))) {
      for (const fragment of list as JsonObject[]) {
        out.push({
          table,
          owner: fragment["owner"] ?? null,
          bucket: fragment["bucket"] ?? null,
          partition: fragment["partition"] ?? null,
          count: (fragment["rows"] as JsonValue[]).length,
        })
      }
    }
    return out
  }
  const configRef = register("config", "config", config, [], [])
  register("programs", "programs", programs, [], [])
  const bootstrapRefs = new Map<string, JsonObject>()
  for (const key of [...specs.keys()].sort(compareCodePoints)) {
    const spec = specs.get(key)
    if (!spec || spec.role !== "bootstrap") continue
    const container = buildContainer(spec, bootstrapRefs)
    bootstrapRefs.set(key, register(key, spec.role, container, counts(container), []))
  }
  const anchors = new Map<string, string>()
  for (const [key, spec] of specs) {
    if (spec.role !== "bootstrap") continue
    for (const fragment of spec.fragments)
      if (["printing", "face"].includes(fragment.table))
        for (const row of fragment.rows)
          anchors.set(`${fragment.table}\0${stringValue(row["id"])}`, key)
  }
  for (const key of [...specs.keys()].sort(compareCodePoints)) {
    const spec = specs.get(key)
    if (!spec || spec.role === "bootstrap") continue
    const container = buildContainer(spec, bootstrapRefs)
    const dependencies = new Map<string, JsonObject>()
    if (spec.role === "images") dependencies.set("config", configRef)
    for (const fragment of spec.fragments) {
      if (fragment.table === "printing_image")
        for (const row of fragment.rows) {
          for (const [table, field] of [
            ["printing", "printing_id"],
            ["face", "face_id"],
          ] as const) {
            const anchor = anchors.get(`${table}\0${stringValue(row[field])}`)
            const ref = bootstrapRefs.get(anchor ?? "")
            if (!anchor || !ref) throw new Error("missing media anchor")
            dependencies.set(anchor, ref)
          }
        }
      if (
        fragment.partition === "detail" &&
        ["printing", "face_revision"].includes(fragment.table)
      ) {
        const base = fileKeyFor({ ...fragment, partition: "bootstrap" })
        const ref = bootstrapRefs.get(base)
        if (!ref) throw new Error("missing detail base")
        dependencies.set(base, ref)
      }
    }
    register(
      key,
      spec.role,
      container,
      counts(container),
      [...dependencies].sort(([a], [b]) => compareCodePoints(a, b)).map(([, ref]) => ref),
    )
  }
  fileRows.sort((a, b) => compareCodePoints(stringValue(a["key"]), stringValue(b["key"])))
  for (const [path, bytes] of images) files.set(path, bytes)

  const textKeys = fileRows
    .filter((row) => ["bootstrap", "text", "config"].includes(stringValue(row["role"])))
    .map((row) => stringValue(row["key"]))
  const textAllValue: JsonObject = {
    format_version: FORMAT,
    members: textKeys.map((key) => ({
      key,
      sha256: fileRows.find((row) => row["key"] === key)?.["sha256"] ?? null,
      payload: payloads.get(key) ?? null,
    })),
  }
  const textAll = blob(textAllValue)
  files.set(textAll.path, textAll.bytes)

  const view = (table: string, partition?: Partition): JsonObject[] =>
    fragments
      .filter((f) => f.table === table && (partition === undefined || f.partition === partition))
      .flatMap((f) => f.rows)
  const keywordUniverse = sortRows(view("keyword"), ["id"]).map((row) => ({
    id: row["id"] ?? null,
    kind: row["kind"] ?? null,
    definition_unit_id: row["definition_unit_id"] ?? null,
    actions: row["actions"] ?? null,
  }))
  const qaCards = new Set<string>()
  const versions = new Map(view("qa_version").map((row) => [stringValue(row["id"]), row]))
  for (const row of view("qa"))
    for (const id of (versions.get(stringValue(row["current_version_id"]))?.["cards"] as
      string[] | undefined) ?? [])
      qaCards.add(id)
  const faceCards = new Map(
    view("face").map((row) => [stringValue(row["id"]), stringValue(row["card_id"])]),
  )
  const errataCards = new Set<string>()
  for (const row of view("errata"))
    for (const version of row["versions"] as JsonObject[])
      for (const change of version["changes"] as JsonObject[])
        errataCards.add(faceCards.get(stringValue(change["face_id"])) ?? "")

  const regions = [
    ...new Set(cards.flatMap((card) => card.printings.map((printing) => printing.region))),
  ].sort(compareCodePoints)
  const languages = [...new Set([...builder.units.values()].map((unit) => unit.lang))].sort(
    compareCodePoints,
  )
  const manifest: JsonObject = {
    format_version: FORMAT,
    data_version: dataVersion,
    published_at: publishedAt,
    regions,
    languages,
    min_reader_version: "2.0.0",
    required_capabilities: [
      "column-partition-v1",
      "digital-same-name-links-v1",
      "fragment-container-v1",
      "image-entity-buckets-v1",
      "image-id-url-v1",
      "rules-name-on-demand-v1",
    ],
    engine_support_target: {
      engine_version: null,
      engine_build_hash: null,
      validation_policy_id: null,
    },
    files: fileRows,
    config_ref: configRef,
    text_all: {
      path: textAll.path,
      sha256: textAll.hash,
      bytes: textAll.bytes.length,
      compressed_bytes: { br: null, gzip: null },
      contains: textKeys.map((key) => ({
        key,
        sha256: fileRows.find((row) => row["key"] === key)?.["sha256"] ?? null,
      })),
    },
    partitioning: { algorithm: "sha256-mod-v1", bucket_count: 64 },
    coverage: { reviews: [], translations: [], mechanics: [] },
    mechanic_universe_id: digest(canonical(keywordUniverse)),
    restriction_coverage: [
      {
        profile_id: PROFILE.id,
        from_date: PROFILE.effectiveFrom,
        until_date: null,
        state: "complete",
        source_url: "https://example.invalid/restrictions",
      },
    ],
    source_windows: [
      {
        kind: "errata",
        region: "jp",
        scope_key: "region:*",
        from_date: "2026-01-01",
        until_date: null,
        as_of: AS_OF,
        state: "complete",
        source_url: "https://example.invalid/errata",
      },
      {
        kind: "qa",
        region: "jp",
        scope_key: "region:*",
        from_date: "2026-01-01",
        until_date: null,
        as_of: AS_OF,
        state: "complete",
        source_url: "https://example.invalid/qa",
      },
    ],
    qa_card_ids: [...qaCards].sort(compareCodePoints),
    errata_card_ids: [...errataCards].sort(compareCodePoints),
    changes_ref: null,
  }
  const manifestBytes = canonical(manifest)
  const manifestHash = digest(manifestBytes)
  const manifestPath = `snapshots/manifests/${manifestHash.slice(7)}.json`
  files.set(manifestPath, manifestBytes)

  const entry = {
    data_version: dataVersion,
    published_at: publishedAt,
    format_version: FORMAT,
    min_reader_version: "2.0.0",
    required_capabilities: [
      "column-partition-v1",
      "digital-same-name-links-v1",
      "fragment-container-v1",
      "image-entity-buckets-v1",
      "image-id-url-v1",
      "rules-name-on-demand-v1",
    ],
    manifest_path: manifestPath,
    manifest_sha256: manifestHash,
    engine_support_target: {
      engine_version: null,
      engine_build_hash: null,
      validation_policy_id: null,
    },
  }
  files.set(
    "snapshots/versions/index.json",
    canonical({ index_format: 2, revision: 2, current: entry, previous: null }),
  )

  const countsOut: Record<string, number> = {}
  for (const table of tables()) countsOut[table] = view(table).length
  return { files, manifest, manifestPath, counts: countsOut }
}

export function canonicalSize(snapshot: BuiltSnapshot): number {
  let total = 0
  for (const bytes of snapshot.files.values()) total += bytes.length
  return total
}
