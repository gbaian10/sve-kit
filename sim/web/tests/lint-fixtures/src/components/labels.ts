declare function t(key: string, options?: Record<string, unknown>): string

// case: literal label -> no-restricted-syntax
export const literal = [{ value: "cost", label: "Cost" }]

// case: quoted label key -> no-restricted-syntax
// prettier-ignore
export const quotedKey = [{ "label": "Cost" }]

// case: template label without interpolation -> no-restricted-syntax
export const plainTemplate = [{ label: `Cost` }]

// case: English text with interpolation -> no-restricted-syntax
export const english = (n: string) => ({ label: `Cost ${n}` })

// case: Japanese text with interpolation -> no-restricted-syntax
export const japanese = (n: string) => ({ label: `コスト${n}` })

// case: Chinese text with interpolation -> no-restricted-syntax
export const chinese = (n: string) => ({ label: `費用 ${n}` })

// case: text on the left of + -> no-restricted-syntax
export const concatLeft = (n: string) => ({ label: "Cost " + n })

// case: text on the right of + -> no-restricted-syntax
export const concatRight = (n: string) => ({ label: n + " cards" })

// case: text on the right end of a + chain -> no-restricted-syntax
export const concatChainEnd = (a: string, b: string, c: string) => ({ label: a + b + c + " cards" })

// case: text at the deepest covered + level -> no-restricted-syntax
export const concatDeepest = (a: string, b: string, c: string, d: string) => ({
  label: "Cost" + a + b + c + d,
})

// bypass: text one + level deeper than covered -> none
export const concatTooDeep = (a: string, b: string, c: string, d: string, e: string) => ({
  label: "Cost" + a + b + c + d + e,
})

// case: letter-bearing template inside + -> no-restricted-syntax
export const concatTemplate = (a: string, b: string) => ({ label: a + `枚${b}` })

// case: interpolation with a slash -> none
export const ratio = (a: string, b: string) => ({ label: `${a}/${b}` })

// case: interpolation with a space -> none
export const spaced = (n: string, m: string) => ({ label: `${n} ${m}` })

// case: interpolation around an escaped newline -> none
export const newline = (a: string, b: string) => ({ label: `${a}\n${b}` })

// case: escaped punctuation only -> none
export const escapedDash = [{ label: `\u2014` }]

// case: key of a nested template does not count as fixed text -> none
export const nestedKey = (a: string) => ({ label: `${t(`cost`)}${a}` })

// case: escaped letter is still text -> no-restricted-syntax
export const escapedLetter = (a: string) => ({ label: `\u0041${a}` })

// case: interpolation only -> none
export const joined = (n: string, m: string) => ({ label: `${n}${m}` })

// case: single interpolation is caught by typescript-eslint instead -> @typescript-eslint/no-unnecessary-template-expression
export const single = (cardNo: string) => ({ label: `${cardNo}` })

// case: + without letters -> none
export const plusNoLetters = (n: string) => ({ label: n + " / " + n })

// case: translated label with interpolation -> none
export const translated = (n: string) => ({ label: t("cost", { n }) })

// case: translated label plus a value (sentence assembly is a review matter) -> none
export const assembled = (n: string) => ({ label: t("cost") + n })

// case: empty label -> none
export const empty = [{ label: "" }]

// case: punctuation-only label -> none
export const dash = [{ label: "—" }]
