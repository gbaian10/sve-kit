"""Import only requested frozen digital cards and their parent/name-face closure."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse, insert_raw_sources
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import Source
    from sve_carddb.translations.sources import Sources


def configuration(
    refs: tuple[SourceRef, ...], targets: tuple[tuple[str, str], ...]
) -> dict[str, JsonValue]:
    """Pin every explicitly supplied API version and requested closure root in F1."""
    return {
        "digital_evidence": {
            "refs": sorted((r.model_dump(mode="json") for r in refs), key=canonical),
            "targets": [list[JsonValue](t) for t in sorted(set(targets))],
        }
    }


def _phases(
    game: str, common: dict[str, JsonValue], item: dict[str, JsonValue]
) -> tuple[str, ...]:
    if game == "sv1":
        if type(common.get("char_type")) is not int:
            raise ValueError("Invalid sv1 character type")
        evolved = common["char_type"] == 1
    else:
        if any(k in item for k in ("super_evo", "super_evolved", "super")):
            raise ValueError(
                "Unrecognized extra digital face requires parser verification"
            )
        evo = item.get("evo")
        if not isinstance(evo, dict) and evo != []:
            raise ValueError("Invalid svwb evolved face")
        evolved = bool(evo)
    return ("normal", "evolved") if evolved else ("normal",)


def import_digital(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements,too-many-locals] -- parent and language-face closure must be checked as one connected inventory
    db: Database,
    sources: Sources,
    refs: tuple[SourceRef, ...],
    targets: tuple[tuple[str, str], ...],
    *,
    allow_subset: bool = False,
) -> None:
    """Run inside the composing transaction; no candidate link is manufactured."""
    declared = object_value(parse(sources.build.configuration.encode())).get(
        "digital_evidence"
    )
    expected = configuration(refs, targets)["digital_evidence"]
    if allow_subset and isinstance(declared, dict):
        declared_targets = declared.get("targets")
        if isinstance(declared_targets, list) and all(
            list(target) in declared_targets for target in targets
        ):
            expected = {**object_value(expected), "targets": declared_targets}
    if declared != expected:
        raise ValueError("Build configuration does not pin digital inputs")
    cards: dict[tuple[str, str, str], tuple[dict[str, JsonValue], Source]] = {}
    for ref in refs:
        lang, document, source = sources.document(ref)
        game = ref.parser.removeprefix("translation-").removesuffix("-v1")
        if game not in {"sv1", "svwb"}:
            raise ValueError("Digital closure requires frozen API sources")
        data = object_value(object_value(document).get("data"))
        raw_cards = data.get("cards") if game == "sv1" else data.get("card_details")
        if isinstance(raw_cards, list):
            entries = [object_value(c) for c in raw_cards]
        else:
            entries = [object_value(c) for c in object_value(raw_cards).values()]
        for item in entries:
            common = item if game == "sv1" else object_value(item.get("common"))
            identifier = common.get("card_id")
            if type(identifier) is not int:
                raise ValueError("Digital card lacks an official integer ID")
            key = game, str(identifier), lang
            previous = cards.get(key)
            if previous is not None and canonical(previous[0]) != canonical(item):
                raise ValueError("Conflicting frozen digital card versions")
            cards[key] = item, source
    required = set(targets)
    pending = list(required)
    while pending:
        game, official = pending.pop()
        if (
            len(official) != (9 if game == "sv1" else 8)
            or not official.isascii()
            or not official.isdecimal()
        ):
            raise ValueError("Digital official ID width mismatch")
        entry = cards.get((game, official, "ja"))
        if entry is None:
            raise ValueError("Requested digital card has no frozen Japanese source")
        item, _ = entry
        common = item if game == "sv1" else object_value(item.get("common"))
        for field in ("base_card_id", "original_card_id"):
            parent = common.get(field)
            if parent is not None and type(parent) is not int:
                raise ValueError("Invalid digital parent ID")
            if parent:
                parent_key = game, str(parent)
                if parent_key not in required:
                    required.add(parent_key)
                    pending.append(parent_key)
    texts = TextInterner(db)
    for game, official in sorted(required):
        item, source = cards[game, official, "ja"]
        common = item if game == "sv1" else object_value(item.get("common"))
        insert_raw_sources(db, (source,))
        token = common.get("is_token") if game == "svwb" else None
        if token is not None and type(token) is not bool:
            raise ValueError("Invalid digital token flag")
        identifier = f"digital:{game}:{official}"
        db.insert(
            "digital_card",
            {
                "id": identifier,
                "game": game,
                "official_id": official,
                "base_card_id": f"digital:{game}:{common['base_card_id']}"
                if common.get("base_card_id")
                else None,
                "original_card_id": f"digital:{game}:{common['original_card_id']}"
                if common.get("original_card_id")
                else None,
                "resource_id": str(
                    common.get(
                        "resource_card_id" if game == "sv1" else "card_resource_id"
                    )
                )
                if common.get(
                    "resource_card_id" if game == "sv1" else "card_resource_id"
                )
                is not None
                else None,
                "is_token": token,
                "source_id": source.id,
            },
        )
        phases = _phases(game, common, item)
        for phase in phases:
            face = identifier + ":" + phase
            db.insert(
                "digital_face",
                {
                    "id": face,
                    "digital_card_id": identifier,
                    "phase": phase,
                    "source_id": source.id,
                },
            )
            for lang in ("ja", "en", "zh-Hant"):
                entry = cards.get((game, official, lang))
                if entry is None:
                    continue
                localized, localized_source = entry
                value = (
                    localized
                    if game == "sv1"
                    else object_value(localized.get("common"))
                )
                name = value.get("card_name" if game == "sv1" else "name")
                if not isinstance(name, str) or not name:
                    continue
                localized_phases = _phases(game, value, localized)
                if localized_phases != phases:
                    raise ValueError("Digital localized face closure mismatch")
                insert_raw_sources(db, (localized_source,))
                sources.uses.append(
                    SourceUse(
                        source=localized_source,
                        usage="digital_name_closure",
                        locator=f"/digital/{game}/{official}/{phase}",
                    )
                )
                db.insert(
                    "digital_text",
                    {
                        "digital_face_id": face,
                        "lang": lang,
                        "name_unit_id": texts.intern(
                            LocalizedText(lang=lang, text=name)
                        ),
                    },
                )


def name_proof(  # ruff: ignore[too-many-locals] -- exact names need both frozen API identity and language pins
    sources: Sources, game: str, official: str, phase: str, lang: str, text: str
) -> tuple[SourceRef, Source]:
    """Verify the selected digital face and language against exact frozen text."""
    config = object_value(parse(sources.build.configuration.encode()))
    digital = object_value(config.get("digital_evidence"))
    found = []
    for value in array(digital.get("refs")):
        ref = SourceRef.model_validate_json(canonical(value))
        if ref.parser != "translation-" + game + "-v1":
            continue
        actual_lang, document, source = sources.document(ref)
        if actual_lang != lang:
            continue
        data = object_value(object_value(document).get("data"))
        if game == "svwb":
            details = object_value(data.get("card_details"))
            if official not in details:
                continue
            item = object_value(details[official])
            common = object_value(item.get("common"))
            locator = f"/data/card_details/{official}/common/name"
            name = common.get("name")
        else:
            matches = [
                (i, object_value(c))
                for i, c in enumerate(array(data.get("cards")))
                if str(object_value(c).get("card_id")) == official
            ]
            if len(matches) != 1:
                continue
            index, common = matches[0]
            item = common
            locator = f"/data/cards/{index}/card_name"
            name = common.get("card_name")
        if phase not in _phases(game, common, item):
            raise ValueError("Selected digital face is absent from frozen source")
        if name != text:
            raise ValueError("Selected digital name differs from frozen source")
        exact = ref.model_copy(
            update={"locator": locator, "text_hash": digest(text.encode())}
        )
        sources.text(exact)
        found.append((exact, source))
    if not found:
        raise ValueError("Selected digital name lacks frozen language evidence")
    return min(found, key=lambda item: canonical(item[0].model_dump(mode="json")))
