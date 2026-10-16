"""Project only selected owner/context translations and their exact text closure."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.export.project.source import Record, Source, pick

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.translations.names.bindings import DisplayBinding
    from sve_carddb.export.project.evidence import Decisions, DisplayCheck


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
        "cr_clause",
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
    if field == "effect" and "cr_version_id" in owner:
        return owner["text_unit_id"]
    if field == "label" and "name_unit_id" in owner:
        return owner["name_unit_id"]
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


def _source_owner(source: Source, key: tuple[str, ...]) -> Record | None:
    if key[0] == "face_revision":
        row = source.index(
            "face_revision", "id,face_id,region,name_unit_id,effect_unit_id"
        ).get(key[1])
        if row is None:
            return None
        return row | {
            "sections": [
                pick(item, "ordinal,text_unit_id,kind")
                for item in source.matching(
                    "face_text_section",
                    "revision_id,ordinal,text_unit_id,kind",
                    revision_id=key[1],
                )
            ]
        }
    if key[0] == "printing_face":
        matches = source.matching(
            "printing_face",
            "printing_id,face_id,printed_name_unit_id,printed_effect_unit_id,flavor_unit_id,printed_text_state",
            printing_id=key[1],
            face_id=key[2],
        )
        if not matches:
            return None
        return matches[0] | {
            "sections": [
                pick(item, "ordinal,text_unit_id,kind")
                for item in source.matching(
                    "printing_text_section",
                    "printing_id,face_id,ordinal,text_unit_id,kind",
                    printing_id=key[1],
                    face_id=key[2],
                )
            ]
        }
    return None


def _current_region(
    source: Source, key: tuple[str, ...]
) -> tuple[str, str, str] | None:
    if key[0] == "face_revision":
        revision = source.index("face_revision", "id,face_id,region").get(key[1])
        if revision is None:
            return None
        face_id, region = string(revision["face_id"]), string(revision["region"])
        face = source.index("face", "id,card_id")[face_id]
        if not any(
            row["region"] == region and row["revision_id"] == key[1]
            for row in source.matching(
                "face_current", "face_id,region,revision_id", face_id=face_id
            )
        ):
            return None
        return string(face["card_id"]), face_id, region
    if key[0] == "printing_face":
        printing = source.index("printing", "id,card_id,region").get(key[1])
        if printing is None:
            return None
        return string(printing["card_id"]), key[2], string(printing["region"])
    return None


def _binding_identity(
    source: Source, use: Record, binding: DisplayBinding
) -> tuple[str, str, str] | None:
    original = _current_region(source, _owner(use))
    if original is None:
        return None
    card_id, face_id, region = original
    if binding.basis == "official_counterpart":
        if binding.destination != _owner(use):
            raise ValueError("Official counterpart must keep the display source owner")
        if binding.target_lang != {"jp": "en", "en": "ja"}[region]:
            raise ValueError("Official counterpart language mismatch")
    else:
        destination = _current_region(source, binding.destination)
        if destination is None:
            return None
        if destination[:2] != (card_id, face_id) or destination[2] == region:
            raise ValueError("Cross-region translation owner mismatch")
        if region != "jp" or destination[2] != "en" or binding.target_lang != "zh-Hant":
            raise ValueError("JP source translation region/language mismatch")
    return original


def _display_checks(
    source: Source,
    use: Record,
    checks: tuple[DisplayCheck, ...],
    identity: tuple[str, str, str],
) -> tuple[DisplayCheck, ...]:
    original_owner = _source_owner(source, _owner(use))
    if original_owner is None:
        return ()
    unit = source_unit(original_owner, string(use["field"]), use["ordinal"])
    for check in checks:
        peer = _source_owner(source, check.counterpart_owner)
        relation = _current_region(source, check.counterpart_owner)
        if peer is None or relation is None:
            raise ValueError("Translation display check owner is unavailable")
        if relation[:2] != identity[:2] or relation[2] == identity[2]:
            raise ValueError("Translation display check identity mismatch")
        if (
            unit != check.source_unit_id
            or source_unit(peer, string(use["field"]), use["ordinal"])
            != check.counterpart_unit_id
        ):
            raise ValueError("Translation display check source mismatch")
    return checks


def _counterpart_text(
    source: Source, checks: tuple[DisplayCheck, ...], binding: DisplayBinding
) -> bool:
    peers = [check for check in checks if check.counterpart]
    if not peers:
        return False
    if binding.translation_id is None:
        raise ValueError("Official counterpart requires a direct translation ID")
    translation = source.index("translation", "id,text,target_lang")[
        binding.translation_id
    ]
    units = source.index("text_unit", "id,text,lang")
    if not any(
        translation["text"] == units[check.counterpart_unit_id]["text"]
        and translation["target_lang"] == units[check.counterpart_unit_id]["lang"]
        for check in peers
    ):
        raise ValueError("Official counterpart text differs from its checked source")
    return True


def _cross_region_allowed(
    source: Source,
    owners: dict[tuple[str, ...], Record],
    cards: dict[str, Record],
    use: Record,
    binding: DisplayBinding,
    checks: tuple[DisplayCheck, ...],
) -> bool:
    identity = _binding_identity(source, use, binding)
    if identity is None or not _confirmed_mapping(source, cards[identity[0]]):
        return False
    checks = _display_checks(source, use, checks, identity)
    if binding.basis == "official_counterpart":
        if _divergent(source, identity[0], string(use["field"])):
            return False
        return _counterpart_text(source, checks, binding)
    if use["field"] not in {"name", "effect", "flavor"} or use["ordinal"] is not None:
        return False
    receiver = owners[binding.destination]
    return source_unit(receiver, string(use["field"]), None) is not None


def _confirmed_mapping(source: Source, card: Record) -> bool:
    regions = {
        row["region"]
        for row in source.matching("printing", "id,card_id,region", card_id=card["id"])
    }
    return card["identity_state"] == "confirmed" and regions == {"jp", "en"}


def _divergent(source: Source, card_id: str, field: str) -> bool:
    scope = "name" if field == "name" else "rules"
    return any(
        row["resolved"] is False and row["field_scope"] in {scope, "all"}
        for row in source.matching(
            "region_divergence", "card_id,region,field_scope,resolved", card_id=card_id
        )
    )


class _SelectedTranslations:
    def __init__(
        self, source: Source, view: dict[str, list[Record]], texts: Texts
    ) -> None:
        self.public = {string(row["id"]): row for row in view["translation"]}
        self.internal = source.index("translation", "id,context_id,text,source_hash")
        self.contexts = source.index("translation_context", "id,source_unit_id")
        self.source = source
        # Validate selections even when no public use reaches the indexed lookups.
        source.rows("translation_selection", "context_id,target_lang,translation_id")
        self.texts = texts

    def append(
        self,
        owner: Record,
        use: Record,
        selected: Record,
        basis: str,
        counterpart: Record | None = None,
    ) -> None:
        """Keep selected current values and their exact source/text closure."""
        identifier = string(selected["translation_id"])
        translation, details = self.public[identifier], self.internal[identifier]
        if (
            details["context_id"] != use["context_id"]
            or translation["target_lang"] != selected["target_lang"]
        ):
            raise ValueError("Translation selection context mismatch")
        if basis == "official_counterpart" and (
            translation["origin"] != "official"
            or translation["authority"] != "sve_official"
        ):
            raise ValueError("Official counterpart origin/authority mismatch")
        unit_id = self.contexts[string(use["context_id"])]["source_unit_id"]
        original = self.texts.by_id[string(unit_id)]
        if details["source_hash"] != digest(string(original["text"]).encode()):
            raise ValueError("Translation source hash differs from its current context")
        translation["source_unit_id"] = unit_id
        translation["annotation_set_id"] = None
        translation["text_unit_id"] = self.texts.intern(
            string(translation["target_lang"]), string(details["text"])
        )
        bindings = array(owner["translations"])
        bindings[:] = [
            raw
            for raw in bindings
            if (
                (
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
                "source": pointer(use),
                "counterpart": counterpart,
            }
        )

    def bind(
        self,
        owner: Record,
        use: Record,
        binding: DisplayBinding,
        counterpart: Record | None = None,
    ) -> None:
        """Official counterpart IDs do not occupy the common selection slot."""
        selected = self.source.matching(
            "translation_selection",
            "context_id,target_lang,translation_id",
            context_id=use["context_id"],
            target_lang=binding.target_lang,
        )
        if binding.basis in {"official_counterpart", "own_source"}:
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
            raise ValueError("JP source binding must use the common selection")
        for row in selected:
            self.append(owner, use, row, binding.basis, counterpart)


def _direct_binding(
    chosen: _SelectedTranslations, owner: Record, use: Record, binding: DisplayBinding
) -> None:
    if binding.destination != _owner(use):
        raise ValueError("Direct own-source binding must keep its exact owner")
    if (
        source_unit(owner, string(use["field"]), use["ordinal"])
        != chosen.contexts[string(use["context_id"])]["source_unit_id"]
    ):
        raise ValueError("Translation owner/context source mismatch")
    chosen.bind(owner, use, binding)


def translations(  # ruff: ignore[complex-structure] -- own-source and cross-region bindings must enforce independent owner gates
    source: Source, view: dict[str, list[Record]], texts: Texts, decisions: Decisions
) -> None:
    """Validate source ownership before deriving any cross-region display binding."""
    chosen = _SelectedTranslations(source, view, texts)
    owners = _owners(view)
    cards = {string(row["id"]): row for row in view["card"]}
    grouped_checks: dict[str, list[DisplayCheck]] = {}
    for check in decisions.display_checks:
        grouped_checks.setdefault(check.source_use_id, []).append(check)
    checks_by_use = {key: tuple(checks) for key, checks in grouped_checks.items()}
    fields = "id,context_id,field,ordinal,face_revision_id,printing_id,face_id,qa_version_id,cr_clause_id,vocabulary_kind,vocabulary_code,keyword_id,product_family_id,product_id"
    uses = source.index("translation_use", fields)
    for use in uses.values():
        owner = owners.get(_owner(use))
        if owner is None:
            continue
        unit = chosen.contexts[string(use["context_id"])]["source_unit_id"]
        if source_unit(owner, string(use["field"]), use["ordinal"]) != unit:
            raise ValueError("Translation owner/context source mismatch")
        for selected in source.matching(
            "translation_selection",
            "context_id,target_lang,translation_id",
            context_id=use["context_id"],
        ):
            chosen.append(owner, use, selected, "own_source")
    for binding in decisions.display_bindings:
        if binding.basis not in {
            "own_source",
            "jp_source",
            "official_counterpart",
        }:
            raise ValueError("Unknown translation display basis")
        use = uses[binding.source_use_id]
        owner = owners.get(binding.destination)
        original_owner = owners.get(_owner(use)) or _source_owner(source, _owner(use))
        if owner is None or original_owner is None:
            continue
        if (
            source_unit(original_owner, string(use["field"]), use["ordinal"])
            != chosen.contexts[string(use["context_id"])]["source_unit_id"]
        ):
            raise ValueError("Translation owner/context source mismatch")
        if binding.basis == "own_source":
            _direct_binding(chosen, owner, use, binding)
        elif _cross_region_allowed(
            source,
            owners,
            cards,
            use,
            binding,
            checks_by_use.get(binding.source_use_id, ()),
        ):
            chosen.bind(
                owner,
                use,
                binding,
                _counterpart_pointer(
                    use, binding, checks_by_use.get(binding.source_use_id, ())
                ),
            )
    _vocabulary_labels(source, view, texts)
    used = {
        string(object_value(raw)["translation_id"])
        for owner in owners.values()
        for raw in array(owner["translations"])
    }
    view["translation"] = [row for row in view["translation"] if row["id"] in used]


def public_owner(key: tuple[str, ...]) -> Record:
    """Public pointers preserve exact owners while removing private build column names."""
    if key[0] == "vocabulary":
        return {"kind": key[0], "vocabulary_kind": key[1], "code": key[2]}
    result: Record = {"kind": key[0], "id": key[1]}
    if key[0] == "printing_face":
        result["face_id"] = key[2]
    return result


def pointer(use: Record) -> Record:
    """The display owner never supplies coordinates for a donor's text."""
    return {
        "owner": public_owner(_owner(use)),
        "field": use["field"],
        "ordinal": use["ordinal"],
    }


def _counterpart_pointer(
    use: Record, binding: DisplayBinding, checks: tuple[DisplayCheck, ...]
) -> Record | None:
    if binding.basis != "official_counterpart":
        return None
    peers = [check for check in checks if check.counterpart]
    if len(peers) != 1:
        raise ValueError("Official counterpart requires one exact peer owner")
    return {
        "owner": public_owner(peers[0].counterpart_owner),
        "field": use["field"],
        "ordinal": use["ordinal"],
    }


def _vocabulary_labels(
    source: Source, view: dict[str, list[Record]], texts: Texts
) -> None:
    # Authored provenance embeds the build revision; an unchanged choice keeps its ID.
    choices = source.rows(
        "glossary_translation", "term_id,lang,text,source_id,origin,low_confidence"
    )
    if not choices:
        return
    terms = source.index("glossary_term", "id,category,low_confidence")
    by_term = {(string(r["term_id"]), string(r["lang"])): r for r in choices}
    for owner in view["vocabulary"]:
        kind, code = string(owner["kind"]), string(owner["code"])
        if not owner["active"]:
            continue
        if kind == "trait":
            concept = "term:trait." + {"cutthroat": "killer"}.get(code, code)
            if concept not in terms or terms[concept]["category"] != "trait":
                continue
        elif (kind, code) == ("special_kind", "token"):
            concept = "term:object.token"
        else:
            continue
        choice = by_term.get((concept, "zh-Hant"))
        if choice is None:
            continue
        low = bool(choice["low_confidence"] or terms[concept]["low_confidence"])
        identifier = (
            "tr:"
            + digest(
                canonical(
                    {
                        "recipe": "vocabulary-glossary-v1",
                        "kind": kind,
                        "code": code,
                        "source_unit_id": owner["label_unit_id"],
                        "choice": choice,
                        "low_confidence": low,
                    }
                )
            )[7:]
        )
        view["translation"].append(
            {
                "id": identifier,
                "target_lang": "zh-Hant",
                "origin": choice["origin"],
                "authority": "digital_official"
                if choice["origin"] == "official"
                else "unofficial",
                "low_confidence": low,
                "source_unit_id": owner["label_unit_id"],
                "text_unit_id": texts.intern("zh-Hant", string(choice["text"])),
                "annotation_set_id": None,
            }
        )
        bindings = array(owner["translations"])
        bindings[:] = [
            raw for raw in bindings if object_value(raw)["target_lang"] != "zh-Hant"
        ]
        bindings.append(
            {
                "field": "label",
                "ordinal": None,
                "target_lang": "zh-Hant",
                "translation_id": identifier,
                "basis": "own_source",
                "source": {
                    "owner": {
                        "kind": "vocabulary",
                        "vocabulary_kind": kind,
                        "code": code,
                    },
                    "field": "label",
                    "ordinal": None,
                },
                "counterpart": None,
            }
        )
