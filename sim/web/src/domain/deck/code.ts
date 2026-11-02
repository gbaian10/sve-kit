import type { Region } from "../search"
import type { DeckBasis } from "./types"

// A deck as one short line for chats and links: "SVEK-" and base64url of
// [format version, edition, basis, leader, main, evolve, side, name]. Cards are the snapshot's
// permanent printing integers (`printing.int_id`, never reassigned); each section is sorted
// ascending and stored as varint deltas, each followed by the copy count.

const PREFIX = "SVEK-"
const FORMAT_VERSION = 1
// A whole input that is a code, or a link carrying one in `d`.
const CODE_INPUT = /^(?:\S*[?&]d=)?(SVEK-[A-Za-z0-9_-]+)(?:[&#]\S*)?$/u

export interface DeckCodeCard {
  readonly intId: number
  readonly count: number
}

export interface DeckCodePayload {
  readonly edition: Region
  readonly basis: DeckBasis
  readonly leader: number | null
  readonly main: readonly DeckCodeCard[]
  readonly evolve: readonly DeckCodeCard[]
  readonly side: readonly DeckCodeCard[]
  readonly name: string
}

export class DeckCodeError extends Error {}

function toBase64Url(bytes: Uint8Array): string {
  let binary = ""
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "")
}

function fromBase64Url(text: string): Uint8Array {
  const base64 = text.replaceAll("-", "+").replaceAll("_", "/")
  try {
    const binary = atob(base64 + "=".repeat((4 - (base64.length % 4)) % 4))
    return Uint8Array.from(binary, (char) => char.charCodeAt(0))
  } catch {
    throw new DeckCodeError("not base64url")
  }
}

function writeVarint(out: number[], value: number): void {
  let rest = value
  while (rest >= 0x80) {
    out.push((rest % 0x80) | 0x80)
    rest = Math.floor(rest / 0x80)
  }
  out.push(rest)
}

function writeSection(out: number[], cards: readonly DeckCodeCard[]): void {
  const sorted = cards.toSorted((a, b) => a.intId - b.intId)
  writeVarint(out, sorted.length)
  let previous = 0
  for (const card of sorted) {
    writeVarint(out, card.intId - previous)
    writeVarint(out, card.count)
    previous = card.intId
  }
}

export function encodeDeckCode(payload: DeckCodePayload): string {
  const out: number[] = [FORMAT_VERSION, payload.edition === "en" ? 1 : 0]
  out.push(payload.basis.kind === "title" ? 1 : 0)
  const basis = new TextEncoder().encode(
    payload.basis.kind === "title" ? payload.basis.titleCode : payload.basis.classCode,
  )
  writeVarint(out, basis.length)
  out.push(...basis)
  writeVarint(out, payload.leader ?? 0)
  writeSection(out, payload.main)
  writeSection(out, payload.evolve)
  writeSection(out, payload.side)
  out.push(...new TextEncoder().encode(payload.name))
  return PREFIX + toBase64Url(Uint8Array.from(out))
}

/**
 * The code a pasted input is: a bare code or a share link. Anything longer (a deck list, even one
 * whose name looks like a code) is not a code.
 */
export function deckCodeOf(text: string): string | undefined {
  return CODE_INPUT.exec(text.trim())?.[1]
}

/** Reads a code back; anything malformed is a `DeckCodeError`, never a partial deck. */
export function decodeDeckCode(code: string): DeckCodePayload {
  if (!code.startsWith(PREFIX)) throw new DeckCodeError("not a deck code")
  const bytes = fromBase64Url(code.slice(PREFIX.length))
  let offset = 0
  const byte = () => {
    const value = bytes[offset]
    if (value === undefined) throw new DeckCodeError("truncated")
    offset += 1
    return value
  }
  const varint = () => {
    let value = 0
    let scale = 1
    for (;;) {
      const next = byte()
      value += (next & 0x7f) * scale
      if (!Number.isSafeInteger(value)) throw new DeckCodeError("integer too large")
      if (next < 0x80) return value
      scale *= 0x80
    }
  }
  const text = (end: number) => {
    if (end > bytes.length) throw new DeckCodeError("truncated")
    try {
      return new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(offset, end))
    } catch {
      throw new DeckCodeError("not UTF-8")
    } finally {
      offset = end
    }
  }
  const section = (): DeckCodeCard[] => {
    const cards: DeckCodeCard[] = []
    let previous = 0
    for (let left = varint(); left > 0; left -= 1) {
      const intId = previous + varint()
      const count = varint()
      if (intId === previous || count === 0 || !Number.isSafeInteger(intId))
        throw new DeckCodeError("bad card entry")
      cards.push({ intId, count })
      previous = intId
    }
    return cards
  }
  if (byte() !== FORMAT_VERSION) throw new DeckCodeError("unknown format version")
  const editionByte = byte()
  const kindByte = byte()
  if (editionByte > 1 || kindByte > 1) throw new DeckCodeError("bad header")
  const basisLength = varint()
  const basisCode = text(offset + basisLength)
  const leader = varint()
  const main = section()
  const evolve = section()
  const side = section()
  return {
    edition: editionByte === 1 ? "en" : "jp",
    basis:
      kindByte === 1
        ? { kind: "title", titleCode: basisCode }
        : { kind: "class", classCode: basisCode },
    leader: leader === 0 ? null : leader,
    main,
    evolve,
    side,
    name: text(bytes.length),
  }
}
