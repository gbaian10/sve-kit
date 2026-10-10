import type { AnnotatedText as TextValue } from "./annotated-text"
import { arrayValue, type JsonObject, objectValue, stringValue } from "./format-v3/json"

interface TextRun {
  readonly text: string
  readonly reference?: JsonObject
  readonly bold?: boolean | null
}

/** R has checked the exact scalar ranges; presentation never normalizes the text. */
export function textRuns(value: TextValue): readonly TextRun[] {
  const text = stringValue(value.unit["text"])
  const boundaries = [0]
  for (const scalar of text) boundaries.push((boundaries.at(-1) ?? 0) + scalar.length)
  const spans = value.annotation
    ? arrayValue(value.annotation["occurrences"]).flatMap((raw) => {
        const occurrence = objectValue(raw)
        return arrayValue(occurrence["ranges"]).map((range) => ({
          start: Number(objectValue(range)["start"]),
          end: Number(objectValue(range)["end"]),
          reference: objectValue(occurrence["reference"]),
          bold: occurrence["bold"] === null ? null : occurrence["bold"] === true,
        }))
      })
    : []
  spans.sort((a, b) => a.start - b.start)
  const result: TextRun[] = []
  let cursor = 0
  for (const span of spans) {
    const start = boundaries[span.start]
    const end = boundaries[span.end]
    if (start === undefined || end === undefined || start < cursor || start >= end)
      throw new Error("unvalidated annotation range")
    if (cursor < start) result.push({ text: text.slice(cursor, start) })
    result.push({ text: text.slice(start, end), reference: span.reference, bold: span.bold })
    cursor = end
  }
  if (cursor < text.length) result.push({ text: text.slice(cursor) })
  return result
}
