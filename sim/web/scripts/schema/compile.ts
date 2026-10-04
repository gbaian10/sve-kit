import Ajv2020 from "ajv/dist/2020"
import standaloneCode from "ajv/dist/standalone"

const ANNOTATIONS = ["x-columns", "x-types", "x-primary-key", "x-fragments", "x-tables"]

/** Compile the published schemas on the build side, never in the browser or decode Worker. */
export function compileSchemas(
  roots: Record<string, object>,
  definitions?: ReadonlySet<string>,
): string {
  const ajv = new Ajv2020({
    strictSchema: true,
    strictTypes: false,
    strictTuples: false,
    inlineRefs: false,
    ownProperties: true,
    code: { source: true },
  })
  for (const keyword of ANNOTATIONS) ajv.addKeyword({ keyword, valid: true })
  const exports: Record<string, string> = {}
  for (const [version, root] of Object.entries(roots)) {
    const { $id, $defs } = root as { $id: string; $defs: Record<string, object> }
    const prepare = (schema: object, rewrite: boolean): object =>
      JSON.parse(
        JSON.stringify(schema, (key: string, value: unknown) => {
          if (key !== "$ref") return value
          if (typeof value !== "string" || !/^#\/\$defs\/[^/~]+$/.test(value))
            throw new Error(`unsupported schema $ref: ${String(value)}`)
          return rewrite ? `${$id}:definition:${value.slice("#/$defs/".length)}` : value
        }),
      ) as object
    // Check root keywords and reference shapes before separating reusable definitions.
    ajv.compile(prepare(root, false))
    for (const [name, definition] of Object.entries($defs)) {
      const id = `${$id}:definition:${name}`
      // Separate schema identities let Ajv reuse referenced functions across standalone exports.
      const prepared = prepare(definition, true)
      ajv.addSchema(prepared, id)
      if (definitions === undefined || definitions.has(name)) exports[`${version}_${name}`] = id
    }
  }
  return standaloneCode(ajv, exports)
}
