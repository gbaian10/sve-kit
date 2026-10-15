import { decodeMessage } from "../format-v3/decode-message"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  objectValue,
  stringValue,
} from "../format-v3/json"
import { type Files, findBase, type Fragment, joinDetail } from "../format-v3/reader"
import { createLocator, homeSetOwner } from "../locator"
import type { SearchBlobs } from "./blobs"
import type { SearchIndex } from "./index"
import type { AbilityPreview } from "./messages"

/** Only the selected face's display effect is returned; annotations remain a card-page purpose. */
export class PreviewReader {
  private readonly locate: ReturnType<typeof createLocator>
  private readonly running = new Map<string, Promise<readonly Fragment[]>>()
  private readonly settled = new Map<string, AbilityPreview | null>()
  private readonly files: Files
  private region: string | undefined
  private readonly version: string
  private readonly blobs: SearchBlobs
  constructor(files: Files, _count: number, version: string, blobs: SearchBlobs) {
    this.files = files
    this.version = version
    this.blobs = blobs
    this.locate = createLocator(
      files,
      new Set(["face", "face_revision", "translation", "text_unit"]),
    )
  }
  private async row(
    table: string,
    id: string,
    signal: AbortSignal,
    owner: JsonObject,
    entity: string,
    partition: "bootstrap" | "detail" = "detail",
  ): Promise<{ row: JsonObject; file: string } | undefined> {
    const key = this.locate({ table, owner, entityKey: [entity], region: this.region, partition })
    if (!key) return undefined
    const fragments = await this.fragments(key, signal)
    signal.throwIfAborted()
    const row = fragments
      .filter((fragment) => fragment.table === table)
      .flatMap((fragment) => fragment.rows)
      .find((row) => row["id"] === id)
    return row ? { row, file: key } : undefined
  }
  private async dictionary(
    table: string,
    id: string,
    origin: string,
    signal: AbortSignal,
  ): Promise<JsonObject | undefined> {
    let result: JsonObject | undefined
    const pending = [origin]
    const visited = new Set<string>()
    while (pending.length) {
      const key = pending.pop()
      if (key === undefined || visited.has(key)) continue
      visited.add(key)
      for (const fragment of await this.fragments(key, signal))
        if (fragment.table === table)
          for (const row of fragment.rows)
            if (row["id"] === id) {
              if (result && canonicalText(result) !== canonicalText(row))
                throw new Error("conflicting preview dictionary copy")
              result = row
            }
      for (const raw of arrayValue(this.files.get(key)?.["dependencies"]))
        pending.push(stringValue(objectValue(raw)["key"]))
    }
    return result
  }
  private async fragments(key: string, signal: AbortSignal): Promise<readonly Fragment[]> {
    const file = this.files.get(key)
    if (!file || integerValue(file["bytes"]) > 12 * 1024 * 1024)
      throw new Error("preview file exceeds budget")
    let pending = this.running.get(key)
    if (!pending) {
      pending = this.blobs
        .read(file, signal)
        .then(async (bytes) => {
          const decoded = decodeMessage({ kind: "file", bytes, file, version: this.version })
          if (decoded.kind !== "fragments") throw new Error("preview file shape")
          return Promise.all(
            decoded.fragments.map(async (fragment) => {
              if (fragment.table !== "face_revision" || fragment.value["base"] === null)
                return fragment
              const ref = objectValue(fragment.value["base"])
              const base = findBase(
                fragment,
                await this.fragments(stringValue(objectValue(ref["file"])["key"]), signal),
              )
              return { ...fragment, rows: joinDetail(fragment, base, new Map()) }
            }),
          )
        })
        .finally(() => this.running.delete(key))
      this.running.set(key, pending)
    }
    return pending
  }
  async resolve(
    index: SearchIndex,
    printingId: string,
    faceId: string,
    language: string,
    signal: AbortSignal,
  ): Promise<AbilityPreview | null> {
    const key = JSON.stringify([printingId, faceId, language])
    const cached = this.settled.get(key)
    if (cached !== undefined) return cached
    const summary = index.recent([printingId])[0]?.summary
    const target = index.imageTargets([printingId]).find((face) => face.faceId === faceId)
    if (!summary || !target) throw new Error("preview face is not in this edition")
    this.region = summary.region
    const owner = homeSetOwner(target.owner)
    const locatedFace = await this.row("face", faceId, signal, owner, summary.cardId, "bootstrap")
    const face = locatedFace?.row
    if (!face) throw new Error("preview face missing")
    const current = arrayValue(face["current"])
      .map((value) => objectValue(value))
      .find((row) => row["region"] === summary.region)
    const wording = arrayValue(face["wording"])
      .map((value) => objectValue(value))
      .find((row) => row["region"] === summary.region)
    const revisionId =
      current?.["revision_id"] ??
      (wording ? objectValue(wording["display"])["revision_id"] : undefined)
    let value: AbilityPreview | null = null
    if (typeof revisionId === "string") {
      const located = await this.row("face_revision", revisionId, signal, owner, summary.cardId)
      if (!located) throw new Error("preview revision missing")
      const revision = located.row
      let unitId = revision["effect_unit_id"]
      if (typeof unitId === "string") {
        const selection = arrayValue(revision["translations"])
          .map((value) => objectValue(value))
          .find(
            (row) =>
              row["field"] === "effect" &&
              row["ordinal"] === null &&
              row["target_lang"] === language,
          )
        if (selection) {
          const translation = await this.dictionary(
            "translation",
            stringValue(selection["translation_id"]),
            located.file,
            signal,
          )
          if (!translation) throw new Error("preview translation missing")
          unitId = translation["text_unit_id"]
        }
        if (typeof unitId !== "string") throw new Error("preview text pointer missing")
        const unit = await this.dictionary("text_unit", unitId, located.file, signal)
        if (!unit) throw new Error("preview text missing")
        value = { text: stringValue(unit["text"]), language: stringValue(unit["lang"]) }
      }
    }
    this.settled.set(key, value)
    while (this.settled.size > 48) this.settled.delete(this.settled.keys().next().value ?? "")
    return value
  }
}
