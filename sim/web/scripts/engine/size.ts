// Prints the engine download size as a Markdown table (raw, gzip -9, Brotli 11) for the CI summary.
import { readFileSync } from "node:fs"
import path from "node:path"
import { brotliCompressSync, constants, gzipSync } from "node:zlib"

import { GENERATED } from "./build"

export function sizeRow(name: string, bytes: Uint8Array): string {
  const gzip = gzipSync(bytes, { level: 9 }).length
  const brotli = brotliCompressSync(bytes, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: 11,
      [constants.BROTLI_PARAM_SIZE_HINT]: bytes.length,
    },
  }).length
  const mib = (size: number) =>
    `${size.toLocaleString("en")} (${(size / 1024 / 1024).toFixed(2)} MiB)`
  return `| ${name} | ${mib(bytes.length)} | ${mib(gzip)} | ${mib(brotli)} |`
}

function main(): void {
  const rows = ["engine_bg.wasm", "engine.js"].map((file) =>
    sizeRow(file, readFileSync(path.join(GENERATED, file))),
  )
  process.stdout.write(
    [
      "### Engine WebAssembly size",
      "",
      "| file | raw | gzip -9 | Brotli 11 |",
      "| --- | ---: | ---: | ---: |",
      ...rows,
      "",
    ].join("\n"),
  )
}

if (import.meta.main) main()
