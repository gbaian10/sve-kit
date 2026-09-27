// case: fetch in src/settings -> no-restricted-globals
export const remote = () => fetch("/settings.json")

// case: globalThis.fetch in src/settings -> no-restricted-properties
export const viaGlobal = () => globalThis.fetch("/settings.json")
