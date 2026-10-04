import { parseArgs } from "node:util"

export function parseLocalArgs(args: string[]) {
  return parseArgs({
    args,
    options: {
      out: { type: "string" },
      sets: { type: "string" },
      limit: { type: "string" },
    },
    strict: true,
    allowPositionals: false,
  }).values
}
