// case: literal console.log -> no-console
export function debug() {
  console.log("debug")
}

// case: console.warn is allowed -> none
export function warn() {
  console.warn("warn")
}

// case: unused import is reported once -> unused-imports/no-unused-imports
import { useState } from "react"

// case: unused variable is reported once -> unused-imports/no-unused-vars
export function unusedVariable() {
  const leftover = 1
}
