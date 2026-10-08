"""Mechanical draft triage, never a human decision or an authored-data writer."""

from collections import Counter, defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import input_record
from sve_carddb.digital_links.catalogue import complete_inventory as complete_inventory  # ruff: ignore[useless-import-alias] -- preserve the existing typed triage import while sharing the catalogue boundary
from sve_carddb.digital_links.evidence import Evidence
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_links.models import SveName
from sve_carddb.registry.records import PrintingData
from sve_carddb.translations.sources import Sources, pointer

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import ReviewContext

# This is the maintainer-approved enum table, not a similarity heuristic.
CLASSES: dict[str, dict[int, str | None]] = {
    "sv1": {
        0: "neutral",
        1: "elf",
        2: "royal",
        3: "witch",
        4: "dragon",
        5: "nightmare",
        6: "nightmare",
        7: "bishop",
        8: None,
    },
    "svwb": {
        0: "neutral",
        1: "elf",
        2: "royal",
        3: "witch",
        4: "dragon",
        5: "nightmare",
        6: "bishop",
        7: None,
    },
}
SVE_CLASSES = {
    "ニュートラル": "neutral",
    "エルフ": "elf",
    "ロイヤル": "royal",
    "ウィッチ": "witch",
    "ドラゴン": "dragon",
    "ナイトメア": "nightmare",
    "ビショップ": "bishop",
}
SVE_TYPES = {"フォロワー": "follower", "スペル": "spell", "アミュレット": "amulet"}
TYPES = {1: "follower", 2: "amulet", 3: "amulet", 4: "spell"}
CONDITIONS = (
    "previous_pending",
    "not_same_card",
    "class_mismatch_or_unknown",
    "type_mismatch_or_unknown",
    "sve_exact_name_mismatch",
    "digital_exact_name_mismatch",
    "nonunique_or_missing_zh",
    "draft_target_mismatch",
    "invalid_source",
    "missing_sve_source",
    "missing_matching_face",
    "missing_digital_source",
)


@dataclass(frozen=True)
class Target:
    game: str
    official_id: str
    target: str


@dataclass(frozen=True)
class Draft:
    name: str
    pending: bool
    relation: str
    numbers: tuple[str, ...]
    targets: tuple[Target, ...]
    relations: dict[str, str]


def _draft(raw: JsonValue) -> Draft:
    row = object_value(raw)
    name, pending, relation = row["ja"], row["needs_decision"], row["relation"]
    numbers = array(row["sve_numbers"])
    relations = object_value(row["relation_by_source"])
    if (
        not isinstance(name, str)
        or not name
        or type(pending) is not bool
        or not isinstance(relation, str)
    ):
        raise ValueError("Invalid draft")
    if (
        not numbers
        or any(not isinstance(n, str) or not n for n in numbers)
        or any(not isinstance(v, str) for v in relations.values())
    ):
        raise ValueError("Invalid draft")
    targets = []
    for raw_target in array(row["sources"]):
        target = object_value(raw_target)
        game, identifier, translated = target["source"], target["card_id"], target["zh"]
        if (
            not isinstance(game, str)
            or type(identifier) is not int
            or not isinstance(translated, str)
        ):
            raise ValueError("Invalid draft")
        targets.append(Target(game, str(identifier), translated))
    return Draft(
        name,
        pending,
        relation,
        tuple(str(n) for n in numbers),
        tuple(targets),
        {k: str(v) for k, v in relations.items()},
    )


def read_draft(content: bytes) -> tuple[Draft, ...]:
    """Project research fields only; confidence and tool comments have no authority."""
    try:
        return tuple(_draft(raw) for raw in array(parse(content)))
    except ValueError, KeyError, TypeError:
        raise ValueError("Invalid digital-link research draft") from None


def sve_inventory(  # ruff: ignore[complex-structure] -- multiple printing variants retain their explicit face maps
    sources: Sources, review: ReviewContext
) -> dict[str, list[tuple[SveName, dict[str, JsonValue]]]]:
    """Map frozen pages through explicit registry mappings, never infer face ordinal."""
    registry = Evidence(sources).registry()
    printings: dict[str, list[PrintingData]] = defaultdict(list)
    for record in registry.records.values():
        if isinstance(record.data, PrintingData) and record.data.region == "jp":
            printings[record.data.card_no].append(record.data)
    result: dict[str, list[tuple[SveName, dict[str, JsonValue]]]] = defaultdict(list)
    for batch in review.source_batches:
        frozen = sources.batch(batch.batch_id)
        for current in frozen.inventory.current:
            descriptor = frozen.descriptor(current.source_version_id)
            if descriptor.provider != "jp" or descriptor.kind != "card":
                continue
            numbers = parse_qs(urlsplit(descriptor.url).query).get("cardno", [])
            matching = printings.get(numbers[0], []) if len(numbers) == 1 else []
            if not matching:
                continue
            lang, document, _ = sources.projection(
                batch.batch_id,
                current.source_version_id,
                "translation-jp-v1",
            )
            if lang != "ja":
                raise ValueError("Digital candidate SVE language mismatch")
            for printing in matching:
                for mapping in printing.source_face_map:
                    face = object_value(
                        pointer(document, f"/faces/{mapping.source_index}")
                    )
                    text = face.get("name")
                    if not isinstance(text, str) or not text:
                        raise ValueError("Digital candidate SVE name is absent")
                    from sve_carddb.catalog.adoption_models import SourceRef  # ruff: ignore[import-outside-top-level] -- reference is created only after frozen face validation

                    ref = SourceRef(
                        batch_id=batch.batch_id,
                        source_version_id=current.source_version_id,
                        parser="translation-jp-v1",
                        locator=f"/faces/{mapping.source_index}/name",
                        text_hash=digest(text.encode()),
                    )
                    sources.text(ref)
                    result[printing.card_no].append(
                        (
                            SveName(
                                printing_id=printing.id,
                                face_id=mapping.face_id,
                                name_ref=ref,
                            ),
                            face,
                        )
                    )
    return result


def generate(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- aggregate triage keeps shared and per-game failures distinct
    content: bytes, sources: Sources
) -> dict[str, JsonValue]:
    """Return private, text-free candidates; no authored records or receipts."""
    drafts = read_draft(content)
    review = review_context(sources)
    games = sorted({t.game for r in drafts for t in r.targets if t.game in CLASSES})
    catalogues = {game: complete_inventory(sources, review, game) for game in games}
    physical = sve_inventory(sources, review)
    unique: dict[str, set[str]] = {}
    for game, names in catalogues.items():
        groups: dict[str, set[str]] = defaultdict(set)
        missing = set()
        for key, name in names.items():
            if key[3] != "ja" or not name.text:
                continue
            translated = names.get((*key[:3], "zh-Hant"))
            if translated is None or not translated.text:
                missing.add(name.text)
            else:
                groups[name.text].add(translated.text)
        unique[game] = {
            name
            for name, values in groups.items()
            if len(values) == 1 and name not in missing
        }
    rows: list[JsonValue] = []
    overall: Counter[str] = Counter()
    occurrences: Counter[str] = Counter()
    by_game: dict[str, Counter[str]] = {g: Counter() for g in games}
    local_counts: dict[str, Counter[str]] = {g: Counter() for g in games}
    for index, draft in enumerate(drafts):
        shared = set()
        if draft.pending:
            shared.add("previous_pending")
        if draft.relation != "same_card":
            shared.add("not_same_card")
        if not draft.targets or any(t.game not in CLASSES for t in draft.targets):
            shared.add("invalid_source")
        faces = []
        for number in draft.numbers:
            if number not in physical:
                shared.add("missing_sve_source")
                continue
            matching = [
                (ref, face)
                for ref, face in physical[number]
                if face["name"] == draft.name
            ]
            if not matching:
                shared.add("sve_exact_name_mismatch")
            faces.extend(matching)
        if not faces:
            shared.add("missing_matching_face")
        flags = {g: set(shared) for g in {t.game for t in draft.targets}}
        targets: list[JsonValue] = []
        for target in draft.targets:
            game = target.game
            local = flags[game]
            if game not in catalogues:
                continue
            if draft.relations.get(game) != "same_card":
                local.add("not_same_card")
            matches = [
                n
                for (g, identifier, _, lang), n in catalogues[game].items()
                if identifier == target.official_id and lang == "ja"
            ]
            if not matches:
                local.add("missing_digital_source")
            for name in matches:
                if name.text != draft.name:
                    local.add("digital_exact_name_mismatch")
                if name.text not in unique[game]:
                    local.add("nonunique_or_missing_zh")
                translated = catalogues[game].get(
                    (game, target.official_id, name.phase, "zh-Hant")
                )
                if translated is None or translated.text != target.target:
                    local.add("draft_target_mismatch")
                common = name.common
                code = common.get("clan" if game == "sv1" else "class")
                kind = common.get("char_type" if game == "sv1" else "type")
                digital_class = CLASSES[game].get(code) if type(code) is int else None
                digital_type = TYPES.get(kind) if type(kind) is int else None
                for _, face in faces:
                    sve_class = SVE_CLASSES.get(str(face.get("card_class")))
                    sve_type = SVE_TYPES.get(str(face.get("card_type")).split("・")[0])
                    if (
                        digital_class is None
                        or sve_class is None
                        or digital_class != sve_class
                    ):
                        local.add("class_mismatch_or_unknown")
                    if (
                        digital_type is None
                        or sve_type is None
                        or digital_type != sve_type
                    ):
                        local.add("type_mismatch_or_unknown")
                targets.append(
                    {
                        "game": game,
                        "official_id": target.official_id,
                        "possible_phase": name.phase,
                        "name_ref": name.ref.model_dump(mode="json"),
                    }
                )
        failures = shared | set().union(*flags.values())
        tier = 2 if failures else 1
        overall[f"tier{tier}"] += 1
        overall.update(failures)
        occurrences.update(t.game + "_tier" + str(tier) for t in draft.targets)
        for game, local in flags.items():
            if game in by_game:
                by_game[game][f"tier{tier}"] += 1
                by_game[game].update(failures)
                local_counts[game].update(local)
        rows.append(
            {
                "draft_row": index,
                "tier": tier,
                "failures": list[JsonValue](sorted(failures)),
                "by_game_failures": {
                    g: list[JsonValue](sorted(f)) for g, f in sorted(flags.items())
                },
                "sve_names": [n.model_dump(mode="json") for n, _ in faces],
                "targets": targets,
                "phase_assignment": "requires_human_review",
                "reason": "Mechanical comparison only; no human adoption event",
            }
        )
    report: dict[str, JsonValue] = {
        "recipe": "digital-link-candidates-v1",
        "draft_hash": digest(content),
        "review_context": review.model_dump(mode="json"),
        "input_record": parse(input_record(sources.build, sources.uses).content()),
        "mapping_hash": digest(
            canonical(
                {
                    g: {str(k): v for k, v in table.items()}
                    for g, table in CLASSES.items()
                }
            )
        ),
        "summary": {
            "rows": len(drafts),
            "source_occurrences": dict(occurrences),
            "tier1": overall["tier1"],
            "tier2": overall["tier2"],
            "failures_overlapping": {c: overall[c] for c in CONDITIONS},
            "by_game": {
                g: {
                    "tier1": counts["tier1"],
                    "tier2": counts["tier2"],
                    "whole_row_failures_overlapping": {
                        c: counts[c] for c in CONDITIONS
                    },
                    "local_failures_overlapping": {
                        c: local_counts[g][c] for c in CONDITIONS
                    },
                }
                for g, counts in by_game.items()
            },
        },
        "candidates": rows,
    }
    return report
