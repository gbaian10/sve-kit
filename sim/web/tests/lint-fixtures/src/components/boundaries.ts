// case: public index is fine, data internals are not -> no-restricted-imports
import { loadSnapshot } from "../data"
import { readCache } from "../data/cache"

// case: using the imports -> none
export const usesPublicApi = loadSnapshot
export const usesInternal = readCache

// case: fetch outside src/data -> no-restricted-globals
export const direct = () => fetch("/snapshot.json")

// case: window.fetch outside src/data -> no-restricted-properties
export const viaWindow = () => window.fetch("/snapshot.json")

// case: localStorage outside src/settings -> no-restricted-globals
export const storage = () => localStorage.getItem("k")

// case: globalThis.localStorage outside src/settings -> no-restricted-properties
export const viaGlobal = () => globalThis.localStorage.getItem("k")
