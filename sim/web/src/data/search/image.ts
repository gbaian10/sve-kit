import type { JsonValue } from "../format-v3/json"

type Packed = Uint8Array | Uint32Array | Float64Array
const TYPES = { u8: Uint8Array, u32: Uint32Array, f64: Float64Array } as const
type TypeName = keyof typeof TYPES

const typeOf = (array: Packed): TypeName =>
  array instanceof Float64Array ? "f64" : array instanceof Uint32Array ? "u32" : "u8"
const align = (offset: number) => Math.ceil(offset / 8) * 8

/**
 * A JSON header and typed arrays in one buffer: [u32 header length][header][arrays], each array
 * 8-byte aligned so unpacking returns views instead of copies.
 */
export function pack(header: JsonValue, arrays: readonly Packed[]): Uint8Array<ArrayBuffer> {
  const text = new TextEncoder().encode(
    JSON.stringify({ header, arrays: arrays.map((array) => [typeOf(array), array.length]) }),
  )
  let size = align(4 + text.length)
  const offsets = arrays.map((array) => {
    const offset = size
    size = align(size + array.byteLength)
    return offset
  })
  const bytes = new Uint8Array(size)
  new DataView(bytes.buffer).setUint32(0, text.length, true)
  bytes.set(text, 4)
  arrays.forEach((array, index) => {
    bytes.set(new Uint8Array(array.buffer, array.byteOffset, array.byteLength), offsets[index])
  })
  return bytes
}

/** Throws on any length that does not add up exactly; callers treat that as a corrupt record. */
export function unpack(source: Uint8Array): { header: JsonValue; arrays: Packed[] } {
  const bytes =
    source.byteOffset % 8 === 0 && source.buffer instanceof ArrayBuffer
      ? new Uint8Array(source.buffer, source.byteOffset, source.byteLength)
      : source.slice()
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const length = view.getUint32(0, true)
  if (4 + length > bytes.length) throw new Error("packed header exceeds buffer")
  const parsed = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + length))) as {
    header: JsonValue
    arrays: [TypeName, number][]
  }
  let offset = align(4 + length)
  const arrays = parsed.arrays.map(([type, count]) => {
    const Type = TYPES[type]
    const end = offset + count * Type.BYTES_PER_ELEMENT
    if (!Number.isSafeInteger(count) || count < 0 || end > bytes.length)
      throw new Error("packed array exceeds buffer")
    const array = new Type(bytes.buffer, bytes.byteOffset + offset, count)
    offset = align(end)
    return array
  })
  if (offset !== bytes.length) throw new Error("packed buffer has trailing bytes")
  return { header: parsed.header, arrays }
}
