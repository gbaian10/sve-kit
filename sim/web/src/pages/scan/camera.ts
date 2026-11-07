import {
  finishKey,
  finishOf,
  finishScores,
  FinishSmoother,
  pickFinish,
} from "../../domain/scan/finish"
import { type QuadCandidate, QuadFollower } from "../../domain/scan/follow"
import { orderQuad, warpQuad } from "../../domain/scan/geometry"
import type { Point, Quad } from "../../domain/scan/tensor"
import { ScanTracker, type TrackFrame } from "../../domain/scan/tracker"
import type { ScanEngine } from "./engine"
import { finishReaderFor } from "./finish-reader"

// The live single-card camera: each frame, find the card around the aiming frame, straighten it,
// read it upright and turned, and fold the result into the tracker. Everything that can be decided
// without a camera lives in domain/scan; this file only moves pixels between video and canvas.

const FIND_THRESHOLD = 0.5
/** The finder looks at a square this much larger than the aiming frame, so a loosely aimed card still fits. */
const FIND_REGION = 1.6
/** How strongly a detection off the frame centre loses to one in the middle. */
const CENTRE_PULL = 0.6
const FRAME_GAP_MS = 30
const IDLE_MS = 200
const CANDIDATES = 10
/** Zoom offered without an optical one: the picture is scaled up. */
const DIGITAL_ZOOM = 8

export interface CameraFrame extends TrackFrame {
  /** The card finder saw a card this frame. */
  readonly found: boolean
}

interface Geometry {
  /** Screen pixels per video pixel, and the video's top left on screen. */
  readonly scale: number
  readonly left: number
  readonly top: number
  /** The aiming frame in video pixels. */
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
}

function canvas(width: number, height: number) {
  const element = document.createElement("canvas")
  element.width = width
  element.height = height
  const context = element.getContext("2d", { willReadFrequently: true })
  if (!context) throw new Error("2D canvas is unavailable")
  return { element, context }
}

interface Canvases {
  readonly finder: ReturnType<typeof canvas>
  readonly work: ReturnType<typeof canvas>
  readonly card: ReturnType<typeof canvas>
}

/** Thrown by `start` when `stop` came first: the late camera has already been closed again. */
class CameraStopped extends Error {
  constructor() {
    super("camera was stopped while starting")
    this.name = "CameraStopped"
  }
}

export class CameraScanner {
  /** Null while another edition's engine loads: the loop waits instead of using a released one. */
  private engine: ScanEngine | null
  private tracker: ScanTracker
  private readonly finish = new FinishSmoother()
  private readonly follower = new QuadFollower()
  private video: HTMLVideoElement | null = null
  private frame: HTMLElement | null = null
  private overlay: HTMLCanvasElement | null = null
  private stream: MediaStream | null = null
  private track: MediaStreamTrack | null = null
  private hardwareZoom: { readonly min: number; readonly max: number } | null = null
  private running = false
  private paused = false
  private sideways = false
  private classes: readonly string[] = []
  private canvases: Canvases | null = null
  /** Bumped by every start and stop, so a start that finishes after a stop knows it is stale. */
  private generation = 0

  constructor(engine: ScanEngine) {
    this.engine = engine
    this.tracker = new ScanTracker(engine.index.dim)
  }

  attach(video: HTMLVideoElement, frame: HTMLElement, overlay: HTMLCanvasElement): void {
    this.video = video
    this.frame = frame
    this.overlay = overlay
  }

  /** Stops using the current engine before it is released (another edition is loading). */
  detach(): void {
    this.engine = null
  }

  /** Swaps in another edition's engine without stopping the camera. */
  setEngine(engine: ScanEngine): void {
    if (engine === this.engine) return
    this.engine = engine
    this.tracker = new ScanTracker(engine.index.dim)
    this.finish.reset()
    this.follower.reset()
    this.canvases = null
  }

  setClasses(classes: readonly string[]): void {
    this.classes = classes
    this.tracker.reset()
    this.finish.reset()
  }

  setSideways(sideways: boolean): void {
    this.sideways = sideways
    this.tracker.reset()
    this.finish.reset()
    this.follower.reset()
  }

  setPaused(paused: boolean): void {
    this.paused = paused
    if (paused) this.video?.pause()
    else void this.video?.play().catch(() => undefined)
  }

  /** Opens the back camera; resolves to the largest zoom the presets may offer. */
  async start(): Promise<number> {
    const video = this.video
    if (!video) throw new Error("camera is not attached")
    this.generation += 1
    const generation = this.generation
    const stream = await navigator.mediaDevices.getUserMedia({
      video: {
        facingMode: { ideal: "environment" },
        width: { ideal: 1920 },
        height: { ideal: 1080 },
      },
      audio: false,
    })
    const close = () => {
      for (const track of stream.getTracks()) track.stop()
      if (video.srcObject === stream) video.srcObject = null
    }
    // Permission can take long; the page may have been left meanwhile.
    if (generation !== this.generation) {
      close()
      throw new CameraStopped()
    }
    this.stream = stream
    video.srcObject = stream
    try {
      await video.play()
    } catch (error) {
      close()
      this.stream = null
      throw error
    }
    if (generation !== this.generation) {
      close()
      throw new CameraStopped()
    }
    const track = stream.getVideoTracks()[0] ?? null
    this.track = track
    // Some browsers (older Firefox) have no getCapabilities; they just get no optical zoom.
    const capabilities = (
      track && "getCapabilities" in track ? track.getCapabilities() : {}
    ) as MediaTrackCapabilities & {
      readonly zoom?: { readonly min: number; readonly max: number }
    }
    try {
      await track?.applyConstraints({
        advanced: [{ focusMode: "continuous" } as MediaTrackConstraintSet],
      })
    } catch {
      // Not every camera can focus continuously; the default focus still works.
    }
    if (generation !== this.generation) {
      close()
      throw new CameraStopped()
    }
    this.hardwareZoom = capabilities.zoom && capabilities.zoom.max > 1 ? capabilities.zoom : null
    return this.hardwareZoom ? Math.max(this.hardwareZoom.max, DIGITAL_ZOOM) : DIGITAL_ZOOM
  }

  /** Optical zoom where the camera has it, the rest by scaling the picture. */
  async setZoom(zoom: number): Promise<void> {
    const video = this.video
    if (!video) return
    // The picture moves under the box: start following afresh.
    this.follower.reset()
    const hardware = this.hardwareZoom
    if (hardware && this.track) {
      const optical = Math.max(hardware.min, Math.min(zoom, hardware.max))
      try {
        await this.track.applyConstraints({
          advanced: [{ zoom: optical } as MediaTrackConstraintSet],
        })
      } catch {
        // A camera may refuse a zoom step; the preview keeps the last one.
      }
      video.style.transform = zoom > hardware.max ? `scale(${String(zoom / hardware.max)})` : ""
    } else video.style.transform = zoom > 1 ? `scale(${String(zoom)})` : ""
  }

  stop(): void {
    this.generation += 1
    this.running = false
    for (const track of this.stream?.getTracks() ?? []) track.stop()
    if (this.video && this.video.srcObject === this.stream) this.video.srcObject = null
    this.stream = null
    this.track = null
    this.follower.reset()
  }

  async run(onFrame: (frame: CameraFrame) => void): Promise<void> {
    this.running = true
    while (this.isRunning()) {
      const video = this.video
      const engine = this.engine
      if (this.paused || !video || video.videoWidth === 0 || !engine) {
        await new Promise((resolve) => setTimeout(resolve, IDLE_MS))
        continue
      }
      try {
        const canvases = this.canvasesFor(engine)
        const geometry = this.geometry(video)
        const quad = await this.find(engine, canvases, video, geometry)
        const pixels = quad
          ? this.straighten(engine, canvases, video, quad)
          : this.aimed(canvases, video, geometry)
        const [upright, turned] = await engine.embed([{ pixels, turned: true }])
        if (!this.isRunning() || engine !== this.engine || !upright || !turned) continue
        const smoothed = this.tracker.smooth([upright, turned])
        const matches = engine.match(smoothed, this.classes, CANDIDATES)
        const frame = this.tracker.step(upright, matches, performance.now(), (picks) =>
          engine.confident(picks),
        )
        this.draw(geometry, quad, frame.status === "ok")
        const shown = quad ? this.withFinish(engine, frame, pixels) : this.withoutFinish(frame)
        onFrame({ ...shown, found: quad !== null })
      } catch (error) {
        // Only a frame begun on an engine that was detached since may fail quietly.
        if (engine === this.engine) throw error
      }
      await new Promise((resolve) => setTimeout(resolve, FRAME_GAP_MS))
    }
  }

  /** Without a straightened card the frame regions mean nothing: keep the plain printing. */
  private withoutFinish(frame: TrackFrame): TrackFrame {
    this.finish.follow("")
    return frame
  }

  /** Shows the leader as the printing whose frame colours match this card (SL against plain). */
  private withFinish(engine: ScanEngine, frame: TrackFrame, pixels: Uint8ClampedArray): TrackFrame {
    const [leader, ...rest] = frame.picks
    this.finish.follow(leader ? finishKey(leader.card) : "")
    const reader = finishReaderFor(engine)
    const group = leader && reader.group(leader.card)
    const references = group && reader.ready(group)
    if (!leader || !group || !references) return frame
    const { width, height } = engine.cardSize
    const scores = finishScores(references, [
      finishOf(pixels, width, height),
      finishOf(pixels, width, height, true),
    ])
    if (!scores) return frame
    const card = pickFinish(group, this.finish.update(finishKey(leader.card), scores), leader.card)
    return card === leader.card ? frame : { ...frame, picks: [{ ...leader, card }, ...rest] }
  }

  // A method, so the loop re-reads the flag that stop() clears while a frame is awaited.
  private isRunning(): boolean {
    return this.running
  }

  private canvasesFor(engine: ScanEngine): Canvases {
    this.canvases ??= {
      finder: canvas(engine.det.input, engine.det.input),
      work: canvas(1, 1),
      card: canvas(engine.cardSize.width, engine.cardSize.height),
    }
    return this.canvases
  }

  private geometry(video: HTMLVideoElement): Geometry {
    const shown = video.getBoundingClientRect()
    const aim = (this.frame ?? video).getBoundingClientRect()
    // The video is drawn with object-cover: scaled to fill, centred, the overflow cut off.
    const scale = Math.max(shown.width / video.videoWidth, shown.height / video.videoHeight)
    const left = shown.left + (shown.width - video.videoWidth * scale) / 2
    const top = shown.top + (shown.height - video.videoHeight * scale) / 2
    return {
      scale,
      left,
      top,
      x: (aim.left - left) / scale,
      y: (aim.top - top) / scale,
      width: aim.width / scale,
      height: aim.height / scale,
    }
  }

  /** Copies a video rectangle (which may stick out of the picture) into a canvas, black outside. */
  private copy(
    target: ReturnType<typeof canvas>,
    video: HTMLVideoElement,
    x: number,
    y: number,
    width: number,
    height: number,
  ): void {
    const { element, context } = target
    const kx = element.width / width
    const ky = element.height / height
    const x0 = Math.max(0, x)
    const y0 = Math.max(0, y)
    const x1 = Math.min(video.videoWidth, x + width)
    const y1 = Math.min(video.videoHeight, y + height)
    // Model input, not UI: the finder and recogniser see black outside the picture.
    context.fillStyle = "#000"
    context.fillRect(0, 0, element.width, element.height)
    if (x1 > x0 && y1 > y0)
      context.drawImage(
        video,
        x0,
        y0,
        x1 - x0,
        y1 - y0,
        (x0 - x) * kx,
        (y0 - y) * ky,
        (x1 - x0) * kx,
        (y1 - y0) * ky,
      )
  }

  /** The card nearest the aiming frame's centre, in video pixels; the one shown last is preferred. */
  private async find(
    engine: ScanEngine,
    canvases: Canvases,
    video: HTMLVideoElement,
    g: Geometry,
  ): Promise<Quad | null> {
    const side = Math.max(g.width, g.height) * FIND_REGION
    const x0 = g.x + g.width / 2 - side / 2
    const y0 = g.y + g.height / 2 - side / 2
    const size = engine.det.input
    this.copy(canvases.finder, video, x0, y0, side, side)
    const found = await engine.detect(
      canvases.finder.context.getImageData(0, 0, size, size).data,
      FIND_THRESHOLD,
    )
    const k = side / size
    const candidates = found.map((detection): QuadCandidate => {
      const cx = detection.quad.reduce((sum, p) => sum + p[0], 0) / 4 / size - 0.5
      const cy = detection.quad.reduce((sum, p) => sum + p[1], 0) / 4 / size - 0.5
      return {
        // Video pixels: the finder's square moves with the aiming frame, the video does not.
        quad: detection.quad.map(([x, y]): Point => [x0 + x * k, y0 + y * k]) as unknown as Quad,
        key: detection.score - CENTRE_PULL * Math.hypot(cx, cy),
      }
    })
    return this.follower.follow(candidates)
  }

  /** Straightens the found card; the source is cut and shrunk to about the card's output size first. */
  private straighten(
    engine: ScanEngine,
    canvases: Canvases,
    video: HTMLVideoElement,
    quad: Quad,
  ): Uint8ClampedArray {
    const ordered = orderQuad(quad, this.sideways)
    const { width, height } = engine.cardSize
    const xs = ordered.map((p) => p[0])
    const ys = ordered.map((p) => p[1])
    const bx = Math.floor(Math.min(...xs)) - 2
    const by = Math.floor(Math.min(...ys)) - 2
    const bw = Math.ceil(Math.max(...xs)) + 2 - bx
    const bh = Math.ceil(Math.max(...ys)) + 2 - by
    const longest = Math.max(
      ...ordered.map((p, i) => {
        const q = ordered[(i + 1) % 4] as Point
        return Math.hypot(q[0] - p[0], q[1] - p[1])
      }),
    )
    const k = Math.min(1, (Math.max(width, height) * 1.15) / longest)
    canvases.work.element.width = Math.max(1, Math.ceil(bw * k))
    canvases.work.element.height = Math.max(1, Math.ceil(bh * k))
    this.copy(canvases.work, video, bx, by, bw, bh)
    const source = canvases.work.context.getImageData(
      0,
      0,
      canvases.work.element.width,
      canvases.work.element.height,
    )
    const local = ordered.map(([x, y]): Point => [(x - bx) * k, (y - by) * k]) as unknown as Quad
    return warpQuad(source, local, width, height)
  }

  /** No card found: read whatever is inside the aiming frame. */
  private aimed(canvases: Canvases, video: HTMLVideoElement, g: Geometry): Uint8ClampedArray {
    const { element, context } = canvases.card
    context.save()
    if (this.sideways) {
      context.translate(element.width, 0)
      context.rotate(Math.PI / 2)
      context.drawImage(video, g.x, g.y, g.width, g.height, 0, 0, element.height, element.width)
    } else
      context.drawImage(video, g.x, g.y, g.width, g.height, 0, 0, element.width, element.height)
    context.restore()
    return context.getImageData(0, 0, element.width, element.height).data
  }

  private draw(g: Geometry, quad: Quad | null, recognised: boolean): void {
    const overlay = this.overlay
    const context = overlay?.getContext("2d")
    if (!overlay || !context) return
    const box = overlay.getBoundingClientRect()
    const ratio = window.devicePixelRatio || 1
    const width = Math.round(box.width * ratio)
    const height = Math.round(box.height * ratio)
    if (overlay.width !== width || overlay.height !== height) {
      overlay.width = width
      overlay.height = height
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0)
    context.clearRect(0, 0, box.width, box.height)
    if (!quad) return
    context.strokeStyle = getComputedStyle(overlay)
      .getPropertyValue(recognised ? "--camera-ok" : "--camera-frame")
      .trim()
    context.lineWidth = 3
    context.lineJoin = "round"
    context.beginPath()
    quad.forEach(([x, y], i) => {
      const sx = g.left + x * g.scale - box.left
      const sy = g.top + y * g.scale - box.top
      if (i) context.lineTo(sx, sy)
      else context.moveTo(sx, sy)
    })
    context.closePath()
    context.stroke()
  }
}
