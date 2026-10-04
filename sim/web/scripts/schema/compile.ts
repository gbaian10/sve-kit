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
    // Check the untouched authority, including root keywords, before preparing reusable definitions.
    ajv.compile(root)
    const { $id, $defs } = root as { $id: string; $defs: Record<string, object> }
    for (const [name, definition] of Object.entries($defs)) {
      const id = `${$id}:definition:${name}`
      // Separate schema identities let Ajv reuse referenced functions across standalone exports.
      const prepared = JSON.parse(
        JSON.stringify(definition, (key: string, value: unknown) =>
          key === "$ref" && typeof value === "string" && value.startsWith("#/$defs/")
            ? `${$id}:definition:${value.slice("#/$defs/".length)}`
            : value,
        ),
      ) as object
      ajv.addSchema(prepared, id)
      if (definitions === undefined || definitions.has(name)) exports[`${version}_${name}`] = id
    }
  }
  return standaloneCode(ajv, exports)
}
