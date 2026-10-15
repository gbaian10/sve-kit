import { normalizeText } from "../../domain/normalize"
import type { Region } from "../../domain/search"
import { decodeMessage } from "../format-v3/decode-message"
import { arrayValue, type JsonObject, objectValue, stringValue } from "../format-v3/json"
import { type Files, findBase, type Fragment, joinDetail } from "../format-v3/reader"
import type { SearchBlobs } from "./blobs"
import type { SearchIndex } from "./index"

/** The explicit effect: prefix keeps an advanced URL intact while its regional closure loads. */
export const effectTerm = (text: string): string | undefined =>
  text.startsWith("effect:") ? text.slice(7).replace(/^"(.*)"$/su, "$1") : undefined

export class EffectIndex {
  private readonly texts = new Map<string, string[]>()
  private constructor() {}
  static async build(
    files: Files,
    edition: Region,
    version: string,
    blobs: SearchBlobs,
    index: SearchIndex,
    signal: AbortSignal,
    progress: (done: number, total: number) => void,
  ): Promise<EffectIndex> {
    const result = new EffectIndex()
    const running = new Map<string, Promise<readonly Fragment[]>>()
    const read = (key: string): Promise<readonly Fragment[]> => {
      const previous = running.get(key)
      if (previous) return previous
      const job = (async () => {
        const file = files.get(key)
        if (!file) throw new Error("effect dependency missing")
        const decoded = decodeMessage({
          kind: "file",
          bytes: await blobs.read(file, signal),
          file,
          version,
        })
        if (decoded.kind !== "fragments") throw new Error("effect container missing")
        return decoded.fragments
      })()
      running.set(key, job)
      return job
    }
    const closure = async (key: string): Promise<readonly Fragment[]> => {
      const seen = new Set<string>()
      const visit = async (key: string): Promise<readonly Fragment[]> => {
        if (seen.has(key)) return []
        seen.add(key)
        const file = files.get(key)
        if (!file) throw new Error("effect dependency missing")
        const dependencies: Fragment[] = []
        for (const raw of arrayValue(file["dependencies"]))
          dependencies.push(...(await visit(stringValue(objectValue(raw)["key"]))))
        return [...dependencies, ...(await read(key))]
      }
      return visit(key)
    }
    const targets = [...files].filter(
      ([, file]) =>
        ["detail", "history"].includes(stringValue(file["purpose"])) && file["region"] === edition,
    )
    let done = 0
    progress(done, targets.length)
    for (const [key] of targets) {
      signal.throwIfAborted()
      const fragments = await closure(key)
      const dictionary = new Map<string, JsonObject>()
      const translations = new Map<string, JsonObject>()
      for (const fragment of fragments)
        for (const row of fragment.rows)
          if (fragment.table === "text_unit") dictionary.set(stringValue(row["id"]), row)
          else if (fragment.table === "translation") translations.set(stringValue(row["id"]), row)
      for (const fragment of fragments) {
        if (fragment.file !== key || fragment.table !== "face_revision") continue
        const rows =
          fragment.value["base"] === null
            ? fragment.rows
            : joinDetail(fragment, findBase(fragment, fragments), new Map())
        for (const row of rows) {
          const cardId = index.cardForFace(stringValue(row["face_id"]))
          if (!cardId) throw new Error("effect face has no catalog card")
          const units = [
            row["effect_unit_id"],
            ...arrayValue(row["sections"]).map((section) => objectValue(section)["text_unit_id"]),
          ]
          for (const raw of arrayValue(row["translations"])) {
            const selection = objectValue(raw)
            if (!["effect", "section"].includes(stringValue(selection["field"]))) continue
            const translation = translations.get(stringValue(selection["translation_id"]))
            if (!translation) throw new Error("effect translation missing")
            units.push(translation["text_unit_id"])
          }
          const text = result.texts.get(cardId) ?? []
          for (const id of units) {
            if (id === null || id === undefined) continue
            const unit = dictionary.get(stringValue(id))
            if (!unit) throw new Error("effect text missing")
            text.push(normalizeText(stringValue(unit["text"])))
          }
          result.texts.set(cardId, text)
        }
      }
      // Retain text only; tuple containers and their dictionaries are disposable after each file.
      done += 1
      progress(done, targets.length)
      running.clear()
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    return result
  }
  matching(term: string): ReadonlySet<string> {
    const normalized = normalizeText(term)
    return new Set(
      [...this.texts]
        .filter(([, texts]) => normalized !== "" && texts.some((text) => text.includes(normalized)))
        .map(([id]) => id),
    )
  }
}
