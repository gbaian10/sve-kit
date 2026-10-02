"""Replay complete frozen name inventories and the reviewed registry face mapping."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse, Version
from sve_carddb.catalog.adoption_models import Batch, ReviewContext, SourceRef
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.registry.records import CardData, FaceData, PrintingData, Text
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.sources.official_jp import card_url
from sve_carddb.translations.digital import _phases
from sve_carddb.translations.sources import Sources, pointer

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_inputs import Source
    from sve_carddb.digital_links.models import DigitalName, Record, SveName
    from sve_carddb.registry.snapshot import RegistrySnapshot


@dataclass(frozen=True)
class Name:
    game: str
    official_id: str
    phase: str
    lang: str
    text: str
    ref: SourceRef
    source: Source
    common: dict[str, JsonValue]


class PageRef(Batch):
    source_version_id: Version
    parser: Text


def inventory(  # ruff: ignore[complex-structure,too-many-locals] -- both API layouts need complete target and language inventories
    sources: Sources, refs: tuple[SourceRef | PageRef, ...]
) -> dict[tuple[str, str, str, str], Name]:
    """Index exact names by game, target, phase and language without normalization."""
    result: dict[tuple[str, str, str, str], Name] = {}
    for ref in refs:
        lang, document, source = sources.projection(
            ref.store_id, ref.batch_id, ref.source_version_id, ref.parser
        )
        sources.uses.append(
            SourceUse(source=source, usage="digital_name_inventory", locator="/data")
        )
        game = ref.parser.removeprefix("translation-").removesuffix("-v1")
        if game not in {"sv1", "svwb"}:
            raise ValueError("Digital inventory requires API evidence")
        data = object_value(object_value(document).get("data"))
        if game == "sv1":
            entries = [
                (f"/data/cards/{i}", object_value(c))
                for i, c in enumerate(array(data.get("cards")))
            ]
        else:
            entries = [
                (f"/data/card_details/{key}", object_value(c))
                for key, c in object_value(data.get("card_details")).items()
            ]
        for base, item in entries:
            common = item if game == "sv1" else object_value(item.get("common"))
            identifier = common.get("card_id")
            if type(identifier) is not int:
                raise ValueError("Digital inventory requires official integer IDs")
            official = str(identifier)
            if len(official) != (9 if game == "sv1" else 8):
                raise ValueError("Digital inventory official ID width mismatch")
            if game == "svwb" and base.rsplit("/", 1)[1] != official:
                raise ValueError("Digital inventory key differs from official ID")
            phases = _phases(game, common, item)
            field = "card_name" if game == "sv1" else "name"
            text = common.get(field)
            if text is not None and not isinstance(text, str):
                raise ValueError("Digital inventory name must be text")
            locator = base + ("/" if game == "sv1" else "/common/") + field
            for phase in phases:
                exact = SourceRef(
                    store_id=ref.store_id,
                    batch_id=ref.batch_id,
                    source_version_id=ref.source_version_id,
                    parser=ref.parser,
                    locator=locator,
                    text_hash=digest((text or "").encode()),
                )
                name = Name(
                    game, official, phase, lang, text or "", exact, source, common
                )
                key = game, official, phase, lang
                old = result.get(key)
                if old is not None and (
                    old.text != name.text or canonical(old.common) != canonical(common)
                ):
                    raise ValueError("Conflicting frozen digital name inventory")
                if old is None or canonical(exact.model_dump(mode="json")) < canonical(
                    old.ref.model_dump(mode="json")
                ):
                    result[key] = name
    return result


def batch_refs(
    sources: Sources, batches: tuple[Batch, ...], game: str
) -> tuple[SourceRef, ...]:
    """Enumerate the declared frozen pages; no made-up field hash becomes evidence."""
    refs = []
    for batch in batches:
        cache_key = batch.store_id, batch.batch_id
        if cache_key not in sources.batches:
            if batch.store_id not in sources.stores:
                raise ValueError("Digital-link source store is not declared")
            sources.batches[cache_key] = FrozenSources(
                sources.stores[batch.store_id], *cache_key
            )
        frozen = sources.batches[cache_key]
        for current in frozen.inventory.current:
            descriptor = frozen.descriptor(current.source_version_id)
            if descriptor.provider != game or descriptor.kind != "api":
                continue
            parser = "translation-" + game + "-v1"
            _, document, source = sources.projection(
                batch.store_id, batch.batch_id, current.source_version_id, parser
            )
            sources.uses.append(
                SourceUse(
                    source=source, usage="digital_name_inventory", locator="/data"
                )
            )
            data = object_value(object_value(document).get("data"))
            if game == "sv1":
                entries = [
                    (f"/data/cards/{i}/card_name", object_value(c).get("card_name"))
                    for i, c in enumerate(array(data.get("cards")))
                ]
            else:
                entries = [
                    (
                        f"/data/card_details/{key}/common/name",
                        object_value(object_value(c).get("common")).get("name"),
                    )
                    for key, c in object_value(data.get("card_details")).items()
                ]
            names = [
                (loc, text) for loc, text in entries if isinstance(text, str) and text
            ]
            if not names:
                names = [(loc, text) for loc, text in entries if isinstance(text, str)]
            if not names:
                continue
            locator, text = min(names)
            refs.append(
                SourceRef(
                    store_id=batch.store_id,
                    batch_id=batch.batch_id,
                    source_version_id=current.source_version_id,
                    parser=parser,
                    locator=locator,
                    text_hash=digest(text.encode()),
                )
            )
    return tuple(sorted(refs, key=lambda r: canonical(r.model_dump(mode="json"))))


@dataclass(frozen=True)
class RegistryIndex:
    cards: Mapping[str, CardData]
    faces: Mapping[str, FaceData]
    printings: Mapping[str, PrintingData]
    by_card: Mapping[str, tuple[PrintingData, ...]]


class Evidence:
    def __init__(self, sources: Sources) -> None:
        self.sources = sources
        self.registries = sources.identities

    def registry(self, review: ReviewContext) -> RegistrySnapshot:
        """Replay the pinned registry before trusting its identity associations."""
        self.sources.repository.context(review.context)
        return self.registries.registry(review)

    def index(self, review: ReviewContext) -> RegistryIndex:
        """Index one verified registry per resolver, rather than scan it for every owner."""
        registry = self.registry(review)
        key = canonical(review.context.model_dump(mode="json"))
        if key not in self.sources.identity_indexes:
            cards = {
                r.data.id: r.data
                for r in registry.records.values()
                if isinstance(r.data, CardData)
            }
            faces = {
                r.data.id: r.data
                for r in registry.records.values()
                if isinstance(r.data, FaceData)
            }
            printings = {
                r.data.id: r.data
                for r in registry.records.values()
                if isinstance(r.data, PrintingData)
            }
            grouped: dict[str, list[PrintingData]] = {}
            for printing in printings.values():
                grouped.setdefault(printing.card_id, []).append(printing)
            self.sources.identity_indexes[key] = RegistryIndex(
                MappingProxyType(cards),
                MappingProxyType(faces),
                MappingProxyType(printings),
                MappingProxyType({k: tuple(v) for k, v in grouped.items()}),
            )
        return self.sources.identity_indexes[key]

    def sve(self, name: SveName, record: Record, review: ReviewContext) -> str:
        """Recheck the printing, parent identity and exact frozen face field."""
        index = self.index(review)
        printings, faces, cards = index.printings, index.faces, index.cards
        printing = printings.get(name.printing_id)
        face = faces.get(name.face_id)
        subject = record.data.subject
        if subject.card_id not in cards or printing is None or face is None:
            raise ValueError(
                "Digital-link SVE identity is absent from reviewed registry"
            )
        if printing.card_id != subject.card_id or face.card_id != subject.card_id:
            raise ValueError("Digital-link SVE evidence belongs to another card")
        lang, text, source = self.sources.text(name.name_ref)
        locators = {
            f"/faces/{mapping.source_index}/name"
            for mapping in printing.source_face_map
            if mapping.face_id == name.face_id
        }
        if (
            printing.region != "jp"
            or name.name_ref.parser != "translation-jp-v1"
            or lang != "ja"
            or source.url != card_url(printing.card_no)
            or name.name_ref.locator not in locators
        ):
            raise ValueError("Digital-link SVE printing face source mismatch")
        return text

    def digital(
        self,
        name: DigitalName,
        record: Record,
        names: dict[tuple[str, str, str, str], Name],
    ) -> str:
        """Recheck actual API target, phase, language and exact name field."""
        subject = record.data.subject
        if name.name_ref.parser != "translation-" + subject.game + "-v1":
            raise ValueError("Digital-link name provider mismatch")
        lang, text, _ = self.sources.text(name.name_ref)
        expected = names.get((subject.game, subject.official_id, name.phase, name.lang))
        if lang != name.lang:
            raise ValueError("Digital-link name language mismatch")
        if expected is None:
            raise ValueError("Digital-link target phase or language is absent")
        _, document, _ = self.sources.document(name.name_ref)
        if subject.game == "svwb":
            locator = f"/data/card_details/{subject.official_id}/common/name"
            valid = name.name_ref.locator == locator
        else:
            parts = name.name_ref.locator.split("/")
            valid = (
                len(parts) == len(("", "data", "cards", "index", "card_name"))
                and parts[1:3] == ["data", "cards"]
                and parts[-1] == "card_name"
            )
            if valid:
                valid = (
                    str(
                        object_value(pointer(document, "/".join(parts[:-1]))).get(
                            "card_id"
                        )
                    )
                    == subject.official_id
                )
        if not valid:
            raise ValueError(
                "Digital-link name locator belongs to another target or field"
            )
        if expected.text != text:
            raise ValueError("Digital-link name differs from frozen inventory")
        return text

    def validate(
        self, record: Record, review: ReviewContext
    ) -> dict[tuple[str, str], str]:
        """Replay the complete historical name evidence and language closure."""
        if self.sources.build != review.context:
            raise ValueError(
                "Digital-link evidence resolver differs from review context"
            )
        value = record.data.value
        if value is None:
            return {}
        for name in value.sve_names:
            self.sve(name, record, review)
        subject = record.data.subject
        names = inventory(
            self.sources, batch_refs(self.sources, review.source_batches, subject.game)
        )
        actual: dict[tuple[str, str], str] = {
            (name.phase, name.lang): self.digital(name, record, names)
            for name in value.digital_names
        }
        phases = {name.phase for name in value.digital_names}
        expected = {
            (phase, lang): name.text
            for (game, official, phase, lang), name in names.items()
            if game == subject.game
            and official == subject.official_id
            and phase in phases
            and name.text
        }
        if actual != expected:
            raise ValueError("Digital-link frozen name language closure mismatch")
        return actual


def configured_refs(sources: Sources) -> tuple[SourceRef, ...]:
    """Read the explicitly pinned current API sources."""
    config = object_value(parse(sources.build.configuration.encode()))
    evidence = object_value(config.get("digital_evidence"))
    return tuple(
        SourceRef.model_validate_json(canonical(ref))
        for ref in array(evidence.get("refs"))
    )
