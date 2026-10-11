interface NumberedPrinting {
  readonly lookup: string
  readonly flat: string
  readonly cardId: string
  readonly printingId: string
}
const lexical = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)
const chunks = (value: string) =>
  (value.match(/\d+|\D+/gu) ?? []).map((text) => ({
    numeric: /^\d+$/u.test(text),
    text: /^\d+$/u.test(text) ? text.replace(/^0+(?=\d)/u, "") : text,
  }))
function natural(a: ReturnType<typeof chunks>, b: ReturnType<typeof chunks>): number {
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    const first = a[i]
    const second = b[i]
    if (!first || !second) return first ? 1 : second ? -1 : 0
    const compared =
      first.numeric && second.numeric
        ? first.text.length - second.text.length || lexical(first.text, second.text)
        : lexical(first.text, second.text)
    if (compared) return compared
  }
  return 0
}

/** Rank every printing once; queries sort scalar ordinals without decoding summaries. */
export function cardNumberOrder(printings: readonly NumberedPrinting[]): Uint32Array {
  const keys = printings.map((printing, ordinal) => {
    const [set = "", number = "", suffix = ""] = printing.lookup.split("|")
    const parts = /^([a-z]*)(\d+)$/u.exec(number)
    return {
      ...printing,
      ordinal,
      known: parts !== null,
      set: chunks(set),
      prefix: parts?.[1] ?? "",
      number: chunks(parts?.[2] ?? ""),
      suffix: suffix.replace(/en$/u, ""),
    }
  })
  keys.sort(
    (a, b) =>
      Number(b.known) - Number(a.known) ||
      (a.known && b.known
        ? natural(a.set, b.set) ||
          lexical(a.prefix, b.prefix) ||
          natural(a.number, b.number) ||
          lexical(a.suffix, b.suffix)
        : lexical(a.flat, b.flat)) ||
      lexical(a.cardId, b.cardId) ||
      lexical(a.printingId, b.printingId),
  )
  const ranks = new Uint32Array(printings.length)
  keys.forEach((key, rank) => {
    ranks[key.ordinal] = rank
  })
  return ranks
}
