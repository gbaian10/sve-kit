// Card search scores every index row against the photo: thousands of 256-wide int8 dot products. The
// WebAssembly SIMD kernel in dots.wat does 16 at a time, about 19 times faster than plain JavaScript.

/**
 * dots.wat compiled with `bunx -p wabt@1.0.39 wat2wasm dots.wat -o dots.wasm`, base64. Rebuild it
 * whenever dots.wat changes; the tests compare its results with the plain JavaScript scores.
 */
const DOTS_WASM =
  "AGFzbQEAAAABCAFgBH9/f38AAwIBAAUDAQBQBw4CA21lbQIABGRvdHMAAAqVAgGSAgMDfwV7A38CQANAIAQgAE8NAf0MAAAAAAAAAAAAAAAAAAAAACEH/QwAAAAAAAAAAAAAAAAAAAAAIQggASAEQQh0aiEGQQAhBQNAIAYgBWr9AAQAIQkgCf2HASEKIAn9iAEhCyACIAVBAXRqIQwgByAKIAz9AAQA/boBIAsgDP0ABBD9ugH9rgH9rgEhByAIIAogDP0ABIAE/boBIAsgDP0ABJAE/boB/a4B/a4BIQggBUEQaiEFIAVBgAJJDQALIAf9GwAgB/0bAWogB/0bAiAH/RsDamohDSAI/RsAIAj9GwFqIAj9GwIgCP0bA2pqIQ4gAyAEQQJ0aiANIA4gDSAOShs2AgAgBEEBaiEEDAALCws="

/** Rows of the kernel are this wide (its loop and offsets are fixed). */
const DIM = 256
const PAGE = 65536

/** Each index row's best dot product with the views, computed together for all rows. */
export interface IndexDots {
  /**
   * Row i's best dot product with any of `views` (unit vectors of the index dimension), before the
   * row scale. The array is reused by the next call.
   */
  best(views: readonly Float32Array[]): Float32Array
}

interface Kernel {
  readonly mem: WebAssembly.Memory
  readonly dots: (n: number, rows: number, query: number, out: number) => void
}

function bytes(): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(DOTS_WASM), (c) => c.charCodeAt(0))
}

function kernel(): Kernel | null {
  const code = bytes()
  // Browsers without WebAssembly SIMD reject the module: they keep the JavaScript scores.
  if (typeof WebAssembly === "undefined" || !WebAssembly.validate(code)) return null
  const exports = new WebAssembly.Instance(new WebAssembly.Module(code)).exports
  const mem = exports["mem"]
  const dots = exports["dots"]
  if (!(mem instanceof WebAssembly.Memory) || typeof dots !== "function") return null
  return { mem, dots: dots as Kernel["dots"] }
}

/**
 * Copies the index rows into a SIMD kernel once. Null when the browser has no WebAssembly SIMD or the
 * index is not 256 wide; card search then scores in plain JavaScript.
 */
export function createIndexDots(vectors: Int8Array, dim: number): IndexDots | null {
  if (dim !== DIM || vectors.length % DIM !== 0) return null
  const found = kernel()
  if (!found) return null
  const { mem, dots } = found
  const n = vectors.length / DIM
  // Memory: rows, then the two int16 query rows, then one int32 result per row.
  const query = vectors.length
  const out = query + 2 * DIM * 2
  const need = out + 4 * n
  if (need > mem.buffer.byteLength) mem.grow(Math.ceil((need - mem.buffer.byteLength) / PAGE))
  new Int8Array(mem.buffer, 0, vectors.length).set(vectors)
  const query16 = new Int16Array(mem.buffer, query, 2 * DIM)
  const result = new Int32Array(mem.buffer, out, n)
  const best = new Float32Array(n)
  // One kernel pass scores two views; more views take more passes.
  const pass = (a: Float32Array, b: Float32Array, first: boolean) => {
    let largest = 0
    for (let j = 0; j < DIM; j += 1)
      largest = Math.max(largest, Math.abs(a[j] ?? 0), Math.abs(b[j] ?? 0))
    const k = largest > 0 ? 32767 / largest : 1
    for (let j = 0; j < DIM; j += 1) {
      query16[j] = Math.round((a[j] ?? 0) * k)
      query16[DIM + j] = Math.round((b[j] ?? 0) * k)
    }
    dots(n, 0, query, out)
    for (let i = 0; i < n; i += 1) {
      const value = (result[i] ?? 0) / k
      if (first || value > (best[i] ?? -Infinity)) best[i] = value
    }
  }
  return {
    best(views) {
      const [head] = views
      if (!head) return best.fill(-Infinity)
      for (let v = 0; v < views.length; v += 2)
        pass(views[v] ?? head, views[v + 1] ?? views[v] ?? head, v === 0)
      return best
    },
  }
}
