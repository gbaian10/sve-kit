import { existsSync, realpathSync } from "node:fs"
import { mkdir, readFile, rm, writeFile } from "node:fs/promises"
import path from "node:path"

import sharp from "sharp"

import { buildSnapshot, canonicalSize, type ImageRequest } from "./build"
import { convertLocal, type LocalRecord, outputTargetError } from "./local"

// Written into every output directory; the tool only ever replaces a directory carrying it.
const MARKER = ".sve-local-snapshot"
const REPO_ROOT = path.resolve(import.meta.dirname, "../../../..")

// Absolute path with symlinks resolved as far as the path exists, so boundary checks see real places.
function canonical(target: string): string {
  const absolute = path.resolve(target)
  let existing = absolute
  while (!existsSync(existing)) existing = path.dirname(existing)
  return path.join(realpathSync(existing), path.relative(existing, absolute))
}

function checkedOutput(out: string, list: string, dataDir: string): string {
  const target = canonical(out)
  const problem = outputTargetError(target, {
    repoRoot: canonical(REPO_ROOT),
    dataDir: canonical(dataDir),
    listDir: canonical(path.dirname(list)),
  })
  if (problem !== null) throw new Error(problem)
  if (existsSync(target) && !existsSync(path.join(target, MARKER))) {
    throw new Error(
      `refusing to replace ${target}: it was not written by fixture:local (no ${MARKER})`,
    )
  }
  return target
}

// Real-looking local snapshot for demos: the private JP card list plus the crawled card images,
// written to SVE_CDN_DIR (never into the repo). Delete it once M2 delivers real snapshots.
function argument(name: string): string | undefined {
  const index = process.argv.indexOf(`--${name}`)
  return index === -1 ? undefined : process.argv[index + 1]
}

async function encodeImage({ width, height, seed, source }: ImageRequest): Promise<Uint8Array> {
  const image =
    source === null
      ? sharp({
          create: {
            width,
            height,
            channels: 3,
            background: {
              r: (seed * 53 + 40) % 256,
              g: (seed * 97 + 80) % 256,
              b: (seed * 193 + 120) % 256,
            },
          },
        })
      : sharp(source).resize(width, height, { fit: "cover" })
  return new Uint8Array(await image.webp({ quality: 80 }).toBuffer())
}

async function main(): Promise<void> {
  const list = process.env["SVE_TEST_SNAPSHOT"]
  const dataDir = process.env["SVE_DATA_DIR"]
  const out = argument("out") ?? process.env["SVE_CDN_DIR"]
  if (list === undefined || dataDir === undefined || out === undefined) {
    throw new Error("SVE_TEST_SNAPSHOT, SVE_DATA_DIR and SVE_CDN_DIR (or --out) must be set")
  }
  const target = checkedOutput(out, list, dataDir)
  const records = (await readFile(list, "utf8"))
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as LocalRecord)
  const sets =
    argument("sets")
      ?.split(",")
      .filter((set) => set !== "") ?? []
  const limit = Number(argument("limit") ?? "300")
  const imageRoot = path.join(dataDir, "media/images/jp")
  const converted = convertLocal(records, {
    sets,
    limit,
    imagePath: (setCode, imageUrl) => {
      const file = path.join(imageRoot, setCode, path.basename(imageUrl))
      return existsSync(file) ? file : null
    },
  })
  const snapshot = await buildSnapshot({
    encodeImage,
    cards: converted.cards,
    families: converted.families,
    vocabulary: converted.vocabulary,
    synthetic: false,
    dataVersion: "20260929T000000Z-0002",
  })
  await rm(target, { recursive: true, force: true })
  await mkdir(target, { recursive: true })
  await writeFile(
    path.join(target, MARKER),
    "written by `bun run fixture:local`; safe to replace\n",
  )
  for (const [relative, bytes] of snapshot.files) {
    const file = path.join(target, relative)
    await mkdir(path.dirname(file), { recursive: true })
    await writeFile(file, bytes)
  }
  process.stdout.write(
    `wrote ${String(converted.cards.length)} cards, ${String(snapshot.files.size)} files (${String(Math.round(canonicalSize(snapshot) / 1024))} KiB) to ${target}\n`,
  )
}

await main()
