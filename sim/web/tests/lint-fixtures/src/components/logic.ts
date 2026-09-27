// case: literal label on an options object -> no-restricted-syntax
export const sortOptions = [{ value: "cost", label: "Cost" }]

// case: literal console.log -> no-console
export function debug() {
  console.log("debug")
}

// case: console.warn is allowed -> none
export function warn() {
  console.warn("warn")
}

// case: quoted label key -> no-restricted-syntax
// prettier-ignore
export const quotedLabel = [{ "label": "Cost" }]

// case: template label without interpolation -> no-restricted-syntax
export const templateLabel = [{ label: `Cost` }]

// case: template label mixing text and values -> no-restricted-syntax
export const mixedLabel = (n: number) => [{ label: `Cost ${String(n)}` }]

// case: empty label -> none
export const emptyLabel = [{ label: "" }]

// case: unused import is reported once -> unused-imports/no-unused-imports
import { useState } from "react"

// case: unused variable is reported once -> unused-imports/no-unused-vars
export function unusedVariable() {
  const leftover = 1
}
