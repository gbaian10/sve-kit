"""Project only selected owner/context translations and their exact text closure."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.project.source import Record, Source, pick
from sve_carddb.snapshot.values import array, digest, object_value, string

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.snapshot.project.evidence import Decisions, DisplayBinding


class Texts:
    def __init__(self, rows: list[Record]) -> None:
        self.rows = rows
        self.by_id = {string(row["id"]): row for row in rows}

    def intern(self, lang: str, text: str) -> str:
        """Derive the fixed public ID, failing rather than reallocating collisions."""
        identifier = "t:" + lang + ":" + digest(text.encode())[7:23]
        row: Record = {"id": identifier, "lang": lang, "text": text}
        if identifier in self.by_id and self.by_id[identifier] != row:
            raise ValueError("Public text ID collision")
        if identifier not in self.by_id:
            self.rows.append(row)
            self.by_id[identifier] = row
        return identifier


def keywords(source: Source, view: dict[str, list[Record]], texts: Texts) -> None:
    """Use explicit glossary concepts; symbols never infer keyword semantics."""
    terms = source.index("glossary_term", "id,source_ja")
    internal = source.index("keyword", "id,term_id")
    for keyword in view["keyword"]:
        term = terms[string(internal[string(keyword["id"])]["term_id"])]
        keyword["name_unit_id"] = texts.intern("ja", string(term["source_ja"]))
        keyword["actions"] = [
            pick(row, "action,label_unit_id")
            for row in source.matching(
                "mechanic_action",
                "keyword_id,action,label_unit_id",
                keyword_id=keyword["id"],
            )
        ]


def _owners(view: dict[str, list[Record]]) -> dict[tuple[str, ...], Record]:
    result: dict[tuple[str, ...], Record] = {}
    for table in (
        "face_revision",
        "qa_version",
        "keyword",
        "product_family",
        "product",
    ):
        result.update({(table, string(row["id"])): row for row in view[table]})
    result.update(
        {
            ("vocabulary", string(row["kind"]), string(row["code"])): row
            for row in view["vocabulary"]
        }
    )
    for printing in view["printing"]:
        result.update(
            {
                (
                    "printing_face",
                    string(printing["id"]),
                    string(face["face_id"]),
                ): face
                for raw in array(printing["faces"])
                if (face := object_value(raw))
            }
        )
    return result


def _owner(use: Record) -> tuple[str, ...]:
    owners: list[tuple[str, ...]] = [
        (table, string(use[table + "_id"]))
        for table in (
            "face_revision",
            "qa_version",
            "keyword",
            "product_family",
            "product",
            "cr_clause",
        )
        if use[table + "_id"] is not None
    ]
    if use["printing_id"] is not None:
        owners.append(
            ("printing_face", string(use["printing_id"]), string(use["face_id"]))
        )
    if use["vocabulary_kind"] is not None:
        owners.append(
            (
                "vocabulary",
                string(use["vocabulary_kind"]),
                string(use["vocabulary_code"]),
            )
        )
    if len(owners) != 1:
        raise ValueError("Translation use must have exactly one owner")
    return owners[0]


def source_unit(owner: Record, field: str, ordinal: JsonValue) -> JsonValue:
    """Resolve original text from the actual owner and field, not a global string."""
    if field in {"section", "action_label"}:
        key = "sections" if field == "section" else "actions"
        values = [object_value(raw) for raw in array(owner[key])]
        if field == "section":
            values = [row for row in values if row["ordinal"] == ordinal]
        else:
            if type(ordinal) is not int or not 0 <= ordinal < len(values):
                raise ValueError("Translation action ordinal missing")
            values = [values[ordinal]]
        if len(values) != 1:
            raise ValueError("Translation section ordinal missing")
        return values[0]["text_unit_id" if field == "section" else "label_unit_id"]
    key = {
        "name": "name_unit_id",
        "effect": "effect_unit_id",
        "flavor": "flavor_unit_id",
        "question": "question_unit_id",
        "answer": "answer_unit_id",
        "label": "label_unit_id",
    }[field]
    if "printed_text_state" in owner and field in {"name", "effect"}:
        key = "printed_" + key
    if "actions" in owner and field == "label":
        key = "name_unit_id"
    return owner[key]


def _current_region(
    view: dict[str, list[Record]],
    key: tuple[str, ...],
) -> tuple[str, str, str] | None:
    if key[0] == "face_revision":
        revision = next(row for row in view["face_revision"] if row["id"] == key[1])
        face_id, region = string(revision["face_id"]), string(revision["region"])
        face = next(row for row in view["face"] if row["id"] == face_id)
        if not any(
            object_value(raw)["region"] == region
            and object_value(raw)["revision_id"] == key[1]
            for raw in array(face["current"])
        ):
            return None
        return string(face["card_id"]), face_id, region
    if key[0] == "printing_face":
        printing = next(row for row in view["printing"] if row["id"] == key[1])
        return string(printing["card_id"]), key[2], string(printing["region"])
    return None


def _cross_region_allowed(
    source: Source,
    view: dict[str, list[Record]],
    use: Record,
    binding: DisplayBinding,
    decisions: Decisions,
) -> bool:
    original = _current_region(view, _owner(use))
    if original is None:
        return False
    card_id, face_id, region = original
    if binding.basis == "official_counterpart":
        if binding.destination != _owner(use):
            raise ValueError("Official counterpart must keep the display source owner")
        if binding.target_lang != {"jp": "en", "en": "ja"}[region]:
            raise ValueError("Official counterpart language mismatch")
    else:
        destination = _current_region(view, binding.destination)
        if destination is None:
            return False
        if destination[:2] != (card_id, face_id) or destination[2] == region:
            raise ValueError("Cross-region translation owner mismatch")
        if region != "jp" or destination[2] != "en" or binding.target_lang != "zh-Hant":
            raise ValueError("Shared JP translation region/language mismatch")
    if (card_id, "en") not in decisions.aligned_regions:
        return False
    scope = "name" if use["field"] == "name" else "rules"
    return not any(
        row["resolved"] is False and row["field_scope"] in {scope, "all"}
        for row in source.matching(
            "region_divergence",
            "card_id,region,field_scope,resolved",
            card_id=card_id,
        )
    )


class _SelectedTranslations:
    def __init__(
        self, source: Source, view: dict[str, list[Record]], texts: Texts
    ) -> None:
        self.public = {string(row["id"]): row for row in view["translation"]}
        self.internal = source.index("translation", "id,context_id,text")
        self.contexts = source.index("translation_context", "id,source_unit_id")
        self.selections = source.rows(
            "translation_selection", "context_id,target_lang,translation_id"
        )
        self.texts = texts

    def append(
        self,
        owner: Record,
        use: Record,
        selected: Record,
        basis: str,
    ) -> None:
        """Keep only reviewed selections and their exact source/text closure."""
        identifier = string(selected["translation_id"])
        translation, details = self.public[identifier], self.internal[identifier]
        if (
            details["context_id"] != use["context_id"]
            or translation["target_lang"] != selected["target_lang"]
        ):
            raise ValueError("Translation selection context mismatch")
        if translation["status"] != "reviewed":
            return
        if basis == "official_counterpart" and (
            translation["origin"] != "official_sve"
            or translation["authority"] != "sve_official"
        ):
            raise ValueError("Official counterpart origin/authority mismatch")
        translation["source_unit_id"] = self.contexts[string(use["context_id"])][
            "source_unit_id"
        ]
        translation["text_unit_id"] = self.texts.intern(
            string(translation["target_lang"]), string(details["text"])
        )
        bindings = array(owner["translations"])
        if basis != "own_source":
            bindings[:] = [
                raw
                for raw in bindings
                if (
                    object_value(raw)["basis"] != "own_source"
                    or (
                        object_value(raw)["field"],
                        object_value(raw)["ordinal"],
                        object_value(raw)["target_lang"],
                    )
                    != (use["field"], use["ordinal"], translation["target_lang"])
                )
            ]
        bindings.append(
            {
                "field": use["field"],
                "ordinal": use["ordinal"],
                "target_lang": translation["target_lang"],
                "translation_id": identifier,
                "basis": basis,
            }
        )

    def bind(self, owner: Record, use: Record, binding: DisplayBinding) -> None:
        """Official counterpart IDs do not occupy the common selection slot."""
        selected = [
            row
            for row in self.selections
            if (
                row["context_id"] == use["context_id"]
                and row["target_lang"] == binding.target_lang
            )
        ]
        if binding.basis == "official_counterpart":
            if binding.translation_id is None:
                raise ValueError(
                    "Official counterpart requires a direct translation ID"
                )
            selected = [
                {
                    "translation_id": binding.translation_id,
                    "target_lang": binding.target_lang,
                }
            ]
        elif binding.translation_id is not None:
            raise ValueError("Shared JP binding must use the common selection")
        for row in selected:
            self.append(owner, use, row, binding.basis)


def translations(
    source: Source, view: dict[str, list[Record]], texts: Texts, decisions: Decisions
) -> None:
    """Validate source ownership before deriving any cross-region display binding."""
    chosen = _SelectedTranslations(source, view, texts)
    owners = _owners(view)
    fields = "id,context_id,field,ordinal,face_revision_id,printing_id,face_id,qa_version_id,cr_clause_id,vocabulary_kind,vocabulary_code,keyword_id,product_family_id,product_id"
    uses = source.index("translation_use", fields)
    for use in uses.values():
        owner = owners.get(_owner(use))
        if owner is None:
            continue
        unit = chosen.contexts[string(use["context_id"])]["source_unit_id"]
        if source_unit(owner, string(use["field"]), use["ordinal"]) != unit:
            raise ValueError("Translation owner/context source mismatch")
        for selected in chosen.selections:
            if selected["context_id"] == use["context_id"]:
                chosen.append(owner, use, selected, "own_source")
    for binding in decisions.display_bindings:
        if binding.basis not in {"shared_jp", "official_counterpart"}:
            raise ValueError("Unknown translation display basis")
        use = uses[binding.source_use_id]
        owner = owners.get(binding.destination)
        if owner is None or _owner(use) not in owners:
            continue
        if _cross_region_allowed(source, view, use, binding, decisions):
            chosen.bind(owner, use, binding)
    used = {
        string(object_value(raw)["translation_id"])
        for owner in owners.values()
        for raw in array(owner["translations"])
    }
    view["translation"] = [row for row in view["translation"] if row["id"] in used]
