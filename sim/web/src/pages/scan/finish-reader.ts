import { loadScanCardImage, scanCardImage } from "../../data"
import { type Finish, finishGroups, finishKey, finishOf } from "../../domain/scan/finish"
import type { ScanCard } from "../../domain/scan/search"
import type { ScanEngine } from "./engine"

// Reads each printing's official small image once (downloaded, so a canvas can read its pixels) and
// keeps its frame colours for the printing check in domain/scan/finish.

/** Size the official images are read at; only colours in large frame regions matter. */
const READ_WIDTH = 120
const READ_HEIGHT = 168

/** RGBA pixels of a printing face's official image, `READ_WIDTH` × `READ_HEIGHT`. */
export type ImageLoader = (card: ScanCard) => Promise<Uint8ClampedArray>

async function loadImagePixels(card: ScanCard): Promise<Uint8ClampedArray> {
  const bitmap = await createImageBitmap(await loadScanCardImage(card.imageId, card.face))
  try {
    const canvas = document.createElement("canvas")
    canvas.width = READ_WIDTH
    canvas.height = READ_HEIGHT
    const context = canvas.getContext("2d", { willReadFrequently: true })
    if (!context) throw new Error("2D canvas is unavailable")
    context.drawImage(bitmap, 0, 0, READ_WIDTH, READ_HEIGHT)
    return context.getImageData(0, 0, READ_WIDTH, READ_HEIGHT).data
  } finally {
    bitmap.close()
  }
}

export class FinishReader {
  private readonly groups: Map<string, ScanCard[]>
  /** Finish of each printing image; null when it could not be read. */
  private readonly done = new Map<string, Finish | null>()
  private readonly pending = new Map<string, Promise<Finish | null>>()
  private readonly load: ImageLoader

  constructor(cards: readonly ScanCard[], load: ImageLoader = loadImagePixels) {
    this.groups = finishGroups(cards)
    this.load = load
  }

  /** The printings that share `card`'s art, when there is more than one. */
  group(card: ScanCard): readonly ScanCard[] | undefined {
    return this.groups.get(finishKey(card))
  }

  /** The group's frame colours if every image is read; otherwise starts reading and returns null. */
  ready(group: readonly ScanCard[]): Finish[] | null {
    const finishes: Finish[] = []
    let missing = false
    for (const card of group) {
      const finish = this.done.get(scanCardImage(card.imageId, card.face))
      if (finish) finishes.push(finish)
      else {
        missing = true
        if (finish === undefined) void this.read(card)
      }
    }
    return missing ? null : finishes
  }

  /** The group's frame colours once read; null when any image cannot be read. */
  async loaded(group: readonly ScanCard[]): Promise<Finish[] | null> {
    const finishes = await Promise.all(group.map((card) => this.read(card)))
    return finishes.every((finish): finish is Finish => finish !== null) ? finishes : null
  }

  private read(card: ScanCard): Promise<Finish | null> {
    const url = scanCardImage(card.imageId, card.face)
    const known = this.done.get(url)
    if (known !== undefined) return Promise.resolve(known)
    let reading = this.pending.get(url)
    if (!reading) {
      reading = this.load(card).then(
        (pixels) => finishOf(pixels, READ_WIDTH, READ_HEIGHT),
        () => null,
      )
      reading = reading.then((finish) => {
        this.done.set(url, finish)
        this.pending.delete(url)
        return finish
      })
      this.pending.set(url, reading)
    }
    return reading
  }
}

const readers = new WeakMap<ScanEngine, FinishReader>()

/** One reader per engine (that is, per card edition), so images are read once per page load. */
export function finishReaderFor(engine: ScanEngine): FinishReader {
  let reader = readers.get(engine)
  if (!reader) {
    reader = new FinishReader(engine.index.cards)
    readers.set(engine, reader)
  }
  return reader
}
