// A deck as one picture drawn on a canvas: the leader beside the name, then the main and evolve
// decks ten cards a row with a count on each. "small" is 1200 px wide for chat previews; "hd" is
// the same layout scaled so each card is drawn at the largest card image (`card_l`, 459 px wide).

export type DeckImageSize = "small" | "hd"

export interface DeckImageCard {
  readonly src: string | undefined
  readonly label: string
  readonly count: number
}

export interface DeckImageInput {
  readonly size: DeckImageSize
  readonly title: string
  readonly subtitle: string
  readonly leader: { readonly src: string | undefined; readonly label: string } | null
  readonly leaderLine: string
  readonly sections: readonly { readonly label: string; readonly cards: readonly DeckImageCard[] }[]
  readonly footer: string
  readonly colors: {
    readonly page: string
    readonly text: string
    readonly muted: string
    readonly badge: string
    readonly badgeText: string
  }
}

const WIDTH = 1200
const COLUMNS = 10
const GAP = 10
const PAD = 28
const CARD_W = (WIDTH - PAD * 2 - GAP * (COLUMNS - 1)) / COLUMNS
const CARD_H = (CARD_W * 88) / 63
const CARD_L_WIDTH = 459
const SECTION_TITLE = 34
const HEADER_WITHOUT_LEADER = 120
const FOOTER = 40

interface DeckImageLayout {
  /** Pixel size of the picture. */
  readonly width: number
  readonly height: number
  /** Multiplier from the 1200-wide layout to the picture's pixels. */
  readonly scale: number
  /** Top of the first section's title, in layout units. */
  readonly header: number
}

const rows = (count: number) => Math.ceil(count / COLUMNS)
const sectionHeight = (count: number) =>
  count === 0 ? 0 : SECTION_TITLE + rows(count) * (CARD_H + GAP) + 8

function deckImageLayout(
  size: DeckImageSize,
  hasLeader: boolean,
  sectionCounts: readonly number[],
): DeckImageLayout {
  const scale = size === "hd" ? CARD_L_WIDTH / CARD_W : 1
  const header = hasLeader ? 20 + CARD_H + 20 : HEADER_WITHOUT_LEADER
  const sections = sectionCounts.reduce((sum, count) => sum + sectionHeight(count), 0)
  return {
    width: Math.round(WIDTH * scale),
    height: Math.round((header + sections + FOOTER) * scale),
    scale,
    header,
  }
}

/** Card pictures that could not be loaded (no image data, a failed download or CORS refusal). */
export class DeckImageMissing extends Error {
  readonly cards: readonly string[]
  constructor(cards: readonly string[]) {
    super(`missing card images: ${String(cards.length)}`)
    this.cards = cards
  }
}

export type LoadImage = (src: string) => Promise<HTMLImageElement | null>

function loadImage(src: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image()
    // The CDN host serves images with CORS headers; without them the canvas could not be exported.
    image.crossOrigin = "anonymous"
    image.onload = () => {
      resolve(image)
    }
    image.onerror = () => {
      resolve(null)
    }
    image.src = src
  })
}

/**
 * Draws the picture. Every card image loads first: a deck picture with blank cards would look
 * complete, so any card without its image fails the whole picture with `DeckImageMissing`.
 */
export async function renderDeckImage(
  input: DeckImageInput,
  load: LoadImage = loadImage,
): Promise<HTMLCanvasElement> {
  const withImage = async <T extends { readonly src: string | undefined }>(item: T) => ({
    ...item,
    image: item.src === undefined ? null : await load(item.src),
  })
  const [leader, sections] = await Promise.all([
    input.leader ? withImage(input.leader) : null,
    Promise.all(
      input.sections.map(async (section) => ({
        label: section.label,
        cards: await Promise.all(section.cards.map(withImage)),
      })),
    ),
  ])
  const missing = [...(leader ? [leader] : []), ...sections.flatMap((section) => section.cards)]
    .filter((item) => item.image === null)
    .map((item) => item.label)
  if (missing.length > 0) throw new DeckImageMissing(missing)
  // Every image loaded; the filters below only narrow the types.
  const loaded = <T extends { readonly image: HTMLImageElement | null }>(
    item: T,
  ): item is T & { readonly image: HTMLImageElement } => item.image !== null
  const [drawnLeader] = (leader ? [leader] : []).filter(loaded)
  const layout = deckImageLayout(
    input.size,
    input.leader !== null,
    input.sections.map((section) => section.cards.length),
  )
  const canvas = document.createElement("canvas")
  canvas.width = layout.width
  canvas.height = layout.height
  const ctx = canvas.getContext("2d")
  if (!ctx) throw new Error("canvas unavailable")
  ctx.scale(layout.scale, layout.scale)
  const height = layout.height / layout.scale
  const { colors } = input
  ctx.fillStyle = colors.page
  ctx.fillRect(0, 0, WIDTH, height)
  ctx.textBaseline = "top"
  const font = (px: number, weight = 400) =>
    `${String(weight)} ${String(px)}px "IBM Plex Sans", "Noto Sans TC", "Noto Sans JP", sans-serif`

  const drawCard = (image: HTMLImageElement, x: number, y: number, count: number | null) => {
    ctx.save()
    ctx.beginPath()
    ctx.roundRect(x, y, CARD_W, CARD_H, 6)
    ctx.clip()
    ctx.drawImage(image, x, y, CARD_W, CARD_H)
    ctx.restore()
    if (count === null) return
    // The count sits top right as on the pool's cards: a disc ringed in the page colour.
    const diameter = CARD_W * 0.21
    const centreX = x + CARD_W - diameter / 2 - 4
    const centreY = y + diameter / 2 + 4
    ctx.beginPath()
    ctx.arc(centreX, centreY, diameter / 2 + diameter * 0.07, 0, Math.PI * 2)
    ctx.fillStyle = colors.page
    ctx.fill()
    ctx.beginPath()
    ctx.arc(centreX, centreY, diameter / 2, 0, Math.PI * 2)
    ctx.fillStyle = colors.badge
    ctx.fill()
    ctx.fillStyle = colors.badgeText
    ctx.font = font(diameter * 0.64, 800)
    ctx.textAlign = "center"
    ctx.textBaseline = "middle"
    ctx.fillText(String(count), centreX, centreY + diameter * 0.04)
    ctx.textAlign = "start"
    ctx.textBaseline = "top"
  }

  let textX = PAD
  if (drawnLeader) {
    drawCard(drawnLeader.image, PAD, 20, null)
    textX = PAD + CARD_W + 18
  }
  const textWidth = WIDTH - textX - PAD
  ctx.fillStyle = colors.text
  ctx.font = font(34, 700)
  ctx.fillText(input.title, textX, 26, textWidth)
  ctx.fillStyle = colors.muted
  ctx.font = font(18)
  ctx.fillText(input.subtitle, textX, 72, textWidth)
  if (drawnLeader) {
    ctx.fillStyle = colors.text
    ctx.font = font(16, 600)
    ctx.fillText(input.leaderLine, textX, 104, textWidth)
  }
  let top = layout.header
  for (const section of sections) {
    const cards = section.cards.filter(loaded)
    if (cards.length === 0) continue
    ctx.fillStyle = colors.text
    ctx.font = font(20, 600)
    ctx.fillText(section.label, PAD, top)
    cards.forEach((card, i) => {
      drawCard(
        card.image,
        PAD + (i % COLUMNS) * (CARD_W + GAP),
        top + SECTION_TITLE + Math.floor(i / COLUMNS) * (CARD_H + GAP),
        card.count,
      )
    })
    top += sectionHeight(cards.length)
  }
  ctx.fillStyle = colors.muted
  ctx.font = font(13)
  ctx.fillText(input.footer, PAD, height - 28, WIDTH - PAD * 2)
  return canvas
}

/** Encodes one picture; resolves to the blob the browser made (its type may differ from asked). */
export type Encode = (type: string, quality?: number) => Promise<Blob>

/** Discord's free upload limit is above this; the HD picture stays under it. */
const HD_BUDGET_BYTES = 3 * 1024 * 1024
const QUALITIES = [0.9, 0.82, 0.74, 0.66, 0.58, 0.5]

/**
 * The small picture stays lossless PNG. The HD one is WebP where the browser encodes it, else
 * JPEG, at the highest quality that fits the budget (the lowest tried if none does).
 */
export async function encodeDeckImage(size: DeckImageSize, encode: Encode): Promise<Blob> {
  if (size === "small") return encode("image/png")
  let type = "image/webp"
  let blob = await encode(type, QUALITIES[0])
  // A browser without a WebP encoder hands back PNG instead.
  if (blob.type !== type) {
    type = "image/jpeg"
    blob = await encode(type, QUALITIES[0])
  }
  for (const quality of QUALITIES.slice(1)) {
    if (blob.size <= HD_BUDGET_BYTES) break
    blob = await encode(type, quality)
  }
  return blob
}

export function canvasEncoder(canvas: HTMLCanvasElement): Encode {
  return (type, quality) =>
    new Promise((resolve, reject) => {
      canvas.toBlob(
        (blob) => {
          if (blob) resolve(blob)
          else reject(new Error("The picture could not be encoded"))
        },
        type,
        quality,
      )
    })
}
