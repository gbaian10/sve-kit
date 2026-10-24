/** UTF-16 offsets in the unchanged source string, also used by inline symbol rendering. */
export interface TextRange {
  readonly start: number
  readonly end: number
}

export interface TextDiff {
  readonly removed: readonly TextRange[]
  readonly added: readonly TextRange[]
}

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" })

function tokens(text: string): string[] {
  const result: string[] = []
  let cursor = 0
  // Icons and Latin words are indivisible; other text uses visible Unicode characters.
  for (const match of text.matchAll(
    /\{[^{}]+\}|《[^《》]+》|\p{Script=Latin}[\p{Script=Latin}\p{M}]*(?:['’]\p{Script=Latin}[\p{Script=Latin}\p{M}]*)*/gu,
  )) {
    result.push(...Array.from(graphemes.segment(text.slice(cursor, match.index)), (s) => s.segment))
    result.push(match[0])
    cursor = match.index + match[0].length
  }
  result.push(...Array.from(graphemes.segment(text.slice(cursor)), (s) => s.segment))
  return result
}

function append(ranges: TextRange[], start: number, length: number) {
  const previous = ranges.at(-1)
  if (previous?.end === start)
    ranges[ranges.length - 1] = { start: previous.start, end: start + length }
  else ranges.push({ start, end: start + length })
}

/** A shortest token edit script; unchanged words between separate edits are never marked. */
export function textDiff(before: string, after: string): TextDiff {
  const old = tokens(before)
  const current = tokens(after)
  let first = 0
  let offset = 0
  while (first < old.length && old[first] === current[first]) offset += (old[first++] ?? "").length
  let oldEnd = old.length
  let newEnd = current.length
  while (oldEnd > first && newEnd > first && old[oldEnd - 1] === current[newEnd - 1]) {
    oldEnd--
    newEnd--
  }
  const rows = oldEnd - first
  const columns = newEnd - first
  const lengths = new Uint32Array((rows + 1) * (columns + 1))
  const cell = (i: number, j: number) => lengths[i * (columns + 1) + j] ?? 0
  for (let i = rows - 1; i >= 0; i--)
    for (let j = columns - 1; j >= 0; j--)
      lengths[i * (columns + 1) + j] =
        old[first + i] === current[first + j]
          ? 1 + cell(i + 1, j + 1)
          : Math.max(cell(i + 1, j), cell(i, j + 1))
  const removed: TextRange[] = []
  const added: TextRange[] = []
  let i = 0
  let j = 0
  let oldOffset = offset
  let newOffset = offset
  while (i < rows || j < columns) {
    const left = old[first + i] ?? ""
    const right = current[first + j] ?? ""
    if (i < rows && j < columns && left === right) {
      oldOffset += left.length
      newOffset += right.length
      i++
      j++
    } else if (i < rows && (j === columns || cell(i + 1, j) > cell(i, j + 1))) {
      append(removed, oldOffset, left.length)
      oldOffset += left.length
      i++
    } else {
      append(added, newOffset, right.length)
      newOffset += right.length
      j++
    }
  }
  return { removed, added }
}
