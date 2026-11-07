import {
  type Box,
  deckTiles,
  lyingSideways,
  mergeDeckBoxes,
  placeTileFinds,
  turnedQuarter,
} from "../../domain/scan/deck-detect"
import {
  chooseReading,
  type DeckCard,
  guessFromDeck,
  withPrinting,
} from "../../domain/scan/deck-read"
import { finishOf, finishScores, pickFinish } from "../../domain/scan/finish"
import { warpQuad } from "../../domain/scan/geometry"
import type { ScanEngine } from "./engine"
import { finishReaderFor } from "./finish-reader"

// Reads every card of one deck photo with the shared engine. The rules (tiling, merging, which
// reading wins, same-deck guesses) live in domain/scan; this file moves pixels.

/** Long side the photo is scaled down to: enough for 50 cards, small enough for a phone. */
const MAX_SIDE = 2400
const FIND_THRESHOLD = 0.35
const CANDIDATES = 6
/** Cards recognised per inference batch (each as four samples). */
const BATCH = 8

export class PhotoError extends Error {
  readonly kind: "not-image" | "cannot-decode"
  readonly type: string

  constructor(kind: "not-image" | "cannot-decode", type: string) {
    super(kind)
    this.name = "PhotoError"
    this.kind = kind
    this.type = type
  }
}

export type DeckProgress =
  | { readonly phase: "finding"; readonly pct: number }
  | { readonly phase: "reading"; readonly done: number; readonly total: number }

function context(element: HTMLCanvasElement): CanvasRenderingContext2D {
  const ctx = element.getContext("2d", { willReadFrequently: true })
  if (!ctx) throw new Error("2D canvas is unavailable")
  return ctx
}

async function decode(blob: Blob): Promise<CanvasImageSource & { width: number; height: number }> {
  try {
    return await createImageBitmap(blob)
  } catch {
    // An <img> reads a few formats createImageBitmap does not; neither reads HEIC on desktop Chrome.
    return await new Promise((resolve, reject) => {
      const url = URL.createObjectURL(blob)
      const image = new Image()
      image.onload = () => {
        URL.revokeObjectURL(url)
        resolve(image)
      }
      image.onerror = () => {
        URL.revokeObjectURL(url)
        reject(new PhotoError("cannot-decode", blob.type || "?"))
      }
      image.src = url
    })
  }
}

/** A picked, pasted or dropped file as a canvas no larger than the reader needs. */
export async function photoFromBlob(blob: Blob | null | undefined): Promise<HTMLCanvasElement> {
  if (!blob || !blob.type.startsWith("image/")) throw new PhotoError("not-image", blob?.type ?? "")
  const image = await decode(blob)
  const k = Math.min(1, MAX_SIDE / Math.max(image.width, image.height))
  const canvas = document.createElement("canvas")
  canvas.width = Math.round(image.width * k)
  canvas.height = Math.round(image.height * k)
  const ctx = context(canvas)
  ctx.imageSmoothingQuality = "high"
  ctx.drawImage(image, 0, 0, canvas.width, canvas.height)
  return canvas
}

async function findCards(
  engine: ScanEngine,
  photo: HTMLCanvasElement,
  onProgress: (progress: DeckProgress) => void,
): Promise<Box[]> {
  const size = engine.det.input
  const tileCanvas = document.createElement("canvas")
  tileCanvas.width = size
  tileCanvas.height = size
  const ctx = context(tileCanvas)
  ctx.imageSmoothingQuality = "high"
  const tiles = deckTiles(photo.width, photo.height)
  const boxes: Box[] = []
  for (const [n, tile] of tiles.entries()) {
    ctx.drawImage(photo, tile.x, tile.y, tile.side, tile.side, 0, 0, size, size)
    const finds = await engine.detect(ctx.getImageData(0, 0, size, size).data, FIND_THRESHOLD)
    boxes.push(...placeTileFinds(finds, tile, size))
    if (n % 8 === 0) {
      onProgress({ phase: "finding", pct: Math.round((n / tiles.length) * 100) })
      // Let the page paint the progress between tiles.
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
  }
  return mergeDeckBoxes(boxes)
}

/** Every card in the photo, in reading order, with its candidates and status. */
export async function readDeckPhoto(
  engine: ScanEngine,
  photo: HTMLCanvasElement,
  classes: readonly string[],
  onProgress: (progress: DeckProgress) => void,
): Promise<DeckCard[]> {
  onProgress({ phase: "finding", pct: 0 })
  const boxes = await findCards(engine, photo, onProgress)
  const pixels = context(photo).getImageData(0, 0, photo.width, photo.height)
  const { width, height } = engine.cardSize
  const cards: DeckCard[] = []
  for (let start = 0; start < boxes.length; start += BATCH) {
    const part = boxes.slice(start, start + BATCH)
    const crops = part.map((box) => ({
      upright: warpQuad(pixels, box.quad, width, height),
      sideways: warpQuad(pixels, turnedQuarter(box.quad), width, height),
    }))
    const views = await engine.embed(
      crops.flatMap((crop) => [
        { pixels: crop.upright, turned: true },
        { pixels: crop.sideways, turned: true },
      ]),
    )
    part.forEach((box, k) => {
      const view = (j: number) => views[4 * k + j] ?? new Float32Array(0)
      const upright = engine.match([view(0), view(1)], classes, CANDIDATES)
      const sideways = lyingSideways(box.quad)
        ? engine.match([view(2), view(3)], classes, CANDIDATES)
        : null
      const turned = chooseReading(upright, sideways) === "sideways"
      const alts = turned && sideways ? sideways : upright
      const crop = crops[k]
      cards.push({
        quad: box.quad,
        crop: (turned ? crop?.sideways : crop?.upright) ?? new Uint8ClampedArray(0),
        alts,
        pick: alts[0]?.card ?? null,
        status: engine.confident(alts) ? "ok" : "unsure",
        gap: 0,
      })
    })
    onProgress({
      phase: "reading",
      done: Math.min(boxes.length, start + BATCH),
      total: boxes.length,
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  return withFinishes(engine, guessFromDeck(cards))
}

/**
 * Each pick as the printing whose frame colours match its crop (SL against plain). The candidate of
 * that card shows the same printing, so confirming it in the picker keeps it.
 */
async function withFinishes(engine: ScanEngine, cards: DeckCard[]): Promise<DeckCard[]> {
  const reader = finishReaderFor(engine)
  const { width, height } = engine.cardSize
  return Promise.all(
    cards.map(async (card) => {
      const group = card.pick && reader.group(card.pick)
      if (!card.pick || !group) return card
      const references = await reader.loaded(group)
      if (!references) return card
      const scores = finishScores(references, [
        finishOf(card.crop, width, height),
        finishOf(card.crop, width, height, true),
      ])
      const pick = scores ? pickFinish(group, scores, card.pick) : card.pick
      return pick === card.pick ? card : withPrinting(card, pick)
    }),
  )
}

/** The photo with each card's box and number; `colourOf` picks the box colour. */
export function drawDeckBoxes(
  canvas: HTMLCanvasElement,
  photo: HTMLCanvasElement,
  cards: readonly (DeckCard | null)[],
  colourOf: (card: DeckCard) => string,
): void {
  const k = Math.min(1, 1400 / photo.width)
  canvas.width = Math.round(photo.width * k)
  canvas.height = Math.round(photo.height * k)
  const ctx = context(canvas)
  ctx.drawImage(photo, 0, 0, canvas.width, canvas.height)
  const plate = getComputedStyle(canvas).getPropertyValue("--camera-chip").trim()
  const font = Math.max(14, canvas.width / 60)
  ctx.lineWidth = Math.max(2, canvas.width / 500)
  ctx.font = `bold ${String(font)}px system-ui`
  cards.forEach((card, at) => {
    if (!card) return
    ctx.strokeStyle = colourOf(card)
    ctx.beginPath()
    card.quad.forEach(([x, y], i) => {
      if (i) ctx.lineTo(x * k, y * k)
      else ctx.moveTo(x * k, y * k)
    })
    ctx.closePath()
    ctx.stroke()
    const label = String(at + 1)
    const tx = card.quad[0][0] * k + 4
    const ty = card.quad[0][1] * k + font
    // A dark plate under the coloured number, so it reads on any photo.
    ctx.fillStyle = plate
    ctx.fillRect(tx - 2, ty - font + 1, ctx.measureText(label).width + 4, font + 2)
    ctx.fillStyle = ctx.strokeStyle
    ctx.fillText(label, tx, ty)
  })
}

/** One straightened card as an image URL for the picker. */
export function cropUrl(card: DeckCard, width: number, height: number): string {
  const canvas = document.createElement("canvas")
  canvas.width = width
  canvas.height = height
  context(canvas).putImageData(new ImageData(new Uint8ClampedArray(card.crop), width, height), 0, 0)
  return canvas.toDataURL()
}
