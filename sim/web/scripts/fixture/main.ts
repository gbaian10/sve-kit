import { mkdir, rm, writeFile } from "node:fs/promises"
import path from "node:path"

import sharp from "sharp"

import { buildSnapshot, canonicalSize } from "./build"

// Solid-colour lossless WebP placeholders; the colour follows the seed so cards look different.
async function encodeImage(width: number, height: number, seed: number): Promise<Uint8Array> {
  const background = {
    r: (seed * 53 + 40) % 256,
    g: (seed * 97 + 80) % 256,
    b: (seed * 193 + 120) % 256,
  }
  const buffer = await sharp({ create: { width, height, channels: 3, background } })
    .webp({ lossless: true })
    .toBuffer()
  return new Uint8Array(buffer)
}

async function main(): Promise<void> {
  const out = path.resolve(
    process.argv[2] ?? path.join(import.meta.dirname, "../../fixtures/snapshot"),
  )
  const snapshot = await buildSnapshot({ encodeImage })
  await rm(out, { recursive: true, force: true })
  for (const [relative, bytes] of snapshot.files) {
    const target = path.join(out, relative)
    await mkdir(path.dirname(target), { recursive: true })
    await writeFile(target, bytes)
  }
  const size = canonicalSize(snapshot)
  process.stdout.write(
    `wrote ${String(snapshot.files.size)} files (${String(Math.round(size / 1024))} KiB) to ${out}; manifest ${snapshot.manifestPath}\n`,
  )
}

await main()
