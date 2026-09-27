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
