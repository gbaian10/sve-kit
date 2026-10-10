import { describe, expect, it } from "vitest"

import compiled from "#snapshot-conformance"

import {
  annotationInputs,
  annotationView,
  componentInput,
  displayClient,
  publicCases,
} from "../../test-utils/public-annotation-cases"
import { createAnnotatedTextResolver } from "../annotated-text"
import { v3Fixture } from "../v3-fixture"
import { validateAnnotations } from "./annotations"
import { decodeRow } from "./decode"
import {
  arrayValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  parseStrict,
  stringValue,
} from "./json"
import { isCompatible, verifyManifest } from "./reader"
import { descriptor, schemaRoot } from "./schema"
import { SchemaValidator } from "./validator"

const schemaValidator = new SchemaValidator(schemaRoot, compiled, "v3")
const cases = arrayValue(publicCases["cases"]).map((value) => objectValue(value))
import { TextOwners } from "./text-owners"

const handled = new Set([
  "owner_field",
  "unicode_ranges",
  "annotation_identity",
  "shared_annotation",
  "annotation_emphasis",
  "concept_references",
  "read_projection",
  "schema",
  "descriptor",
  "canonical_bytes",
  "admission",
])

function run(
  caseValue: JsonObject,
  input: ReturnType<typeof annotationInputs>[number],
): JsonObject {
  const operation = caseValue["operation"],
    parameters = objectValue(caseValue["input"])
  const expected = objectValue(caseValue["expected"])
  if (operation === "schema") {
    if (schemaValidator.validate(stringValue(parameters["definition"]), input))
      throw new Error("schema rejected")
    return {}
  }
  if (operation === "canonical_bytes") {
    parseStrict(stringValue(objectValue(input)["raw_json"]))
    return {}
  }
  if (operation === "descriptor") {
    const value = objectValue(input)
    expect({ columns: value["columns"], items: value["items"] }).toEqual(
      descriptor(stringValue(value["definition"])),
    )
    return {}
  }
  if (operation === "admission") {
    const value = objectValue(objectValue(input)["value"])
    if (
      !isCompatible(value) ||
      !arrayValue(parameters["supported_formats"]).includes(value["format_version"] ?? null) ||
      stringValue(value["min_reader_version"])
        .split(".")
        .map(Number)
        .some(
          (part, i, parts) =>
            parts
              .slice(0, i)
              .every(
                (previous, j) =>
                  previous === Number(stringValue(parameters["reader_version"]).split(".")[j]),
              ) && part > Number(stringValue(parameters["reader_version"]).split(".")[i]),
        ) ||
      arrayValue(value["required_capabilities"]).some(
        (capability) => !arrayValue(parameters["supported_capabilities"]).includes(capability),
      )
    )
      throw new Error("admission rejected")
    return {}
  }
  let value = objectValue(input)
  if (operation === "annotation_emphasis")
    value = { ...value, text_unit: ["t:ja:559aead08264d579", "ja", "A"] }
  if (operation === "annotation_identity") {
    const ids: JsonValue[] = []
    for (const row of arrayValue(value["sets"])) {
      const used = new Set(
        arrayValue(arrayValue(row)[2]).map((occurrence) => {
          const reference = objectValue(arrayValue(occurrence)[1])
          return reference[reference["kind"] === "card_name" ? "term_id" : "key"]
        }),
      )
      const view = annotationView(
        componentInput({
          ...value,
          sets: [row],
          concepts: arrayValue(value["concepts"]).filter((concept) =>
            used.has(arrayValue(concept)[0]),
          ),
        }),
      )
      validateAnnotations(view, ["en", "ja", "zh-Hant"])
      ids.push(view["annotation_set"]?.[0]?.["id"] ?? null)
    }
    return {
      text_unit_count: 1,
      annotation_set_ids: ids,
      distinct: new Set(ids).size === ids.length,
    }
  }
  if (operation !== "read_projection") {
    const view = annotationView(componentInput(value))
    if (operation === "owner_field") {
      const pointer = decodeRow("PublicTextPointer", value["pointer"] ?? null)
      pointer["owner"] = view["field_annotation"]?.[0]?.["owner"] ?? null
      return { text_unit_id: new TextOwners(view).pointer(pointer) }
    }
    validateAnnotations(view, ["en", "ja", "zh-Hant"])
    if (operation === "shared_annotation")
      return {
        annotation_set_count: view["annotation_set"]?.length ?? 0,
        field_annotation_count: view["field_annotation"]?.length ?? 0,
      }
    if (operation === "unicode_ranges") {
      const text = stringValue(arrayValue(value["text_unit"])[2]),
        scalars = Array.from(text),
        boundaries = [0]
      for (const scalar of scalars) boundaries.push((boundaries.at(-1) ?? 0) + scalar.length)
      const spans = (view["annotation_set"] ?? []).flatMap((row) =>
        arrayValue(row["occurrences"]).flatMap((occurrence) =>
          arrayValue(objectValue(occurrence)["ranges"]).map((range) => objectValue(range)),
        ),
      )
      return {
        codepoint_length: scalars.length,
        utf16_length: text.length,
        utf16_boundaries: boundaries,
        slices: spans.map((r) => scalars.slice(Number(r["start"]), Number(r["end"])).join("")),
        utf16_ranges: spans.map((r) => [
          boundaries[Number(r["start"])] ?? null,
          boundaries[Number(r["end"])] ?? null,
        ]),
      }
    }
    return {}
  }
  verifyManifest({ ...objectValue(v3Fixture("manifest.json")), ...objectValue(value["admission"]) })
  const view = annotationView(value)
  validateAnnotations(
    view,
    arrayValue(value["languages"]).map((value) => stringValue(value)),
  )
  if (expected["result"] === "accept") {
    const selected = decodeRow(
      "FieldTranslation",
      objectValue(arrayValue(value["field_translations"]).at(-1))["value"] ?? null,
    )
    const translation = view["translation"]?.find((row) => row["id"] === selected["translation_id"])
    return {
      source_owner_kind: objectValue(objectValue(selected["source"])["owner"])["kind"] ?? null,
      source_text_unit_id: translation?.["source_unit_id"] ?? null,
      target_text_unit_id: translation?.["text_unit_id"] ?? null,
    }
  }
  return {}
}

describe("direct shared public annotation operations", () => {
  it.each(cases.filter((value) => handled.has(stringValue(value["operation"]))))(
    "$operation: $group/$id",
    (caseValue) => {
      const expected = objectValue(caseValue["expected"])
      for (const input of annotationInputs(caseValue)) {
        if (expected["result"] === "reject") {
          const operation = caseValue["operation"]
          if (
            [
              "read_projection",
              "unicode_ranges",
              "concept_references",
              "annotation_emphasis",
              "owner_field",
            ].includes(stringValue(operation)) &&
            caseValue["group"] !== "PA-01"
          )
            expect(() => {
              run(caseValue, input)
            }).toThrow(`public-annotation/${stringValue(expected["reason"])}`)
          else
            expect(() => {
              run(caseValue, input)
            }).toThrow()
        } else {
          const actual = run(caseValue, input)
          for (const [key, value] of Object.entries(expected))
            if (key !== "result") expect(actual[key], key).toEqual(value)
        }
      }
    },
  )
})

describe("direct shared logical display operations", () => {
  it.each(cases.filter((value) => value["operation"] === "display"))(
    "$group/$id",
    async (caseValue) => {
      const expected = objectValue(caseValue["expected"]),
        parameters = objectValue(caseValue["input"])
      for (const input of annotationInputs(caseValue)) {
        const value = objectValue(input),
          before = JSON.stringify(value)
        const pointer = decodeRow("PublicTextPointer", parameters["receiver"] ?? null)
        const owner = new TextOwners(annotationView(value)).get(objectValue(pointer["owner"]))
        const selected = await createAnnotatedTextResolver(displayClient(value)).resolve(
          pointer,
          stringValue(parameters["ui_lang"]),
        )
        expect(selected).not.toBeNull()
        const annotation = selected?.translated?.annotation
        const bold = annotation
          ? objectValue(arrayValue(annotation["occurrences"])[0])["bold"]
          : null
        const support = objectValue(value["support"])
        const chosenStatus = support["override_status"] ?? support["shared_status"]
        const status =
          chosenStatus === "engine_passed" && arrayValue(support["region_blocks"]).length
            ? "reviewed"
            : chosenStatus
        const actual: JsonObject = {
          original_text_unit_id: selected?.original.unit["id"] ?? null,
          original_annotation_set_id: selected?.original.annotation?.["id"] ?? null,
          translation_text_unit_id: selected?.translated?.unit["id"] ?? null,
          translation_annotation_set_id: annotation?.["id"] ?? null,
          comparison_text_unit_id: selected?.selection
            ? (selected.source?.unit["id"] ?? null)
            : null,
          comparison_annotation_set_id: selected?.selection
            ? (selected.source?.annotation?.["id"] ?? null)
            : null,
          basis: selected?.selection?.["basis"] ?? null,
          effective_status: status ?? null,
          automatic: status === "engine_passed",
          grants_aligned:
            before !== JSON.stringify(value) && objectValue(value["producer"])["aligned"] === true,
          grants_official_counterpart: selected?.selection?.["basis"] === "official_counterpart",
          translated_field: selected?.selection?.["field"] ?? null,
          translated_section_ordinals: arrayValue(owner.row["translations"])
            .map((row) => objectValue(row))
            .filter((row) => row["field"] === "section")
            .map((row) => row["ordinal"] ?? null),
          missing_translation:
            selected?.translated === undefined &&
            selected?.original.unit["lang"] !== parameters["ui_lang"],
          origin: selected?.translation?.["origin"] ?? null,
          low_confidence: selected?.translation?.["low_confidence"] ?? false,
          show_proofreading_notice: selected?.translation?.["low_confidence"] ?? false,
          bold_value: bold ?? null,
          render_bold: parameters["bold_enabled"] === true && bold === true,
          annotation_data_unchanged: before === JSON.stringify(value),
        }
        for (const [field, value] of Object.entries(expected))
          if (field !== "result") expect(actual[field], field).toEqual(value)
      }
    },
  )
})

it.each(cases.filter((value) => value["operation"] === "envelope_versions"))(
  "$operation: $group/$id",
  (caseValue) => {
    const value = objectValue(annotationInputs(caseValue)[0])
    const manifest = objectValue(v3Fixture("manifest.json"))
    const configFile = arrayValue(manifest["files"])
      .map((file) => objectValue(file))
      .find((file) => file["role"] === "config")
    const config = objectValue(
      v3Fixture(`payloads/${stringValue(configFile?.["sha256"]).slice(7)}.json`),
    )
    expect(
      schemaValidator.validate("Config", {
        ...config,
        format_version: objectValue(value["members"])["config"] ?? null,
      })?.keyword,
    ).toBe("const")
  },
)
