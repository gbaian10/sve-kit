"""Project admitted inputs to the engine's temporary legacy test interface."""

from dataclasses import asdict, dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.domains.parsed_cards.batches import selection

if TYPE_CHECKING:
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.parsed_cards.storage import Catalog
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary
    from sve_carddb.parse.pages.extract_en import Face


@dataclass(frozen=True)
class English:
    """Restrict the EN batch to reviewed identities and project native rule labels."""

    batch_id: str
    numbers: frozenset[str]
    vocabulary: Vocabulary


class _EnglishFaces:
    def __init__(self, vocabulary: Vocabulary) -> None:
        self.vocabulary = vocabulary
        self.labels = {
            (term.kind, term.code): term.label.text for term in vocabulary.terms
        }

    def _label(self, kind: str, raw: str) -> str:
        return self.labels[kind, self.vocabulary.lookup("en", kind, raw).code]

    def __call__(self, face: Face) -> dict[str, JsonValue]:
        kind = self.vocabulary.lookup("en", "type", face.info["Card Type"])
        raw_class = face.info["Class"]
        result: dict[str, JsonValue] = {
            "name": face.name,
            "card_type": "・".join(
                [self.labels["type", kind.code]]
                + [self.labels["special_kind", code] for code in kind.special_kinds]
            ),
            "card_class": "-" if raw_class == "-" else self._label("class", raw_class),
            **face.stats,
            "traits": [self._label("trait", trait) for trait in face.traits],
            "text": face.text,
            "sections": list[JsonValue](face.sections),
        }
        if (title := face.info.get("Universe")) is not None:
            result["title"] = self._label("title", title)
        return result


def _records(  # ruff: ignore[complex-structure] -- validate the complete batch while selecting the native subset
    catalog: Catalog, batch_id: str, region: Region, english: English | None = None
) -> dict[str, bytes]:
    batch = selection(catalog, batch_id)
    if batch.region != region:
        raise ValueError(f"Legacy engine test input requires a {region.upper()} batch")
    current = {version for version, active in batch.versions.items() if active}
    records: dict[str, bytes] = {}
    seen: set[str] = set()
    native = _EnglishFaces(english.vocabulary) if english is not None else None
    for pair in catalog.files:
        if pair.region != region:
            continue
        for card in pair.cards:
            for value in card.observations:
                version = value.evidence.descriptor.id
                if version not in current:
                    continue
                if version in seen:
                    raise ValueError("Duplicate current engine version")
                seen.add(version)
                record = value.document.record
                if english is not None and record.number not in english.numbers:
                    continue
                if record.number in records:
                    raise ValueError("Ambiguous current engine printing")
                if value.document.region == "jp":
                    faces: list[JsonValue] = [
                        asdict(f) for f in value.document.record.faces
                    ]
                else:
                    assert native is not None
                    faces = [native(f) for f in value.document.record.faces]
                records[record.number] = canonical(
                    {"number": record.number, "faces": faces}
                )
    if seen != current or (region == "jp" and not records):
        raise ValueError("Engine test selection is incomplete or empty")
    return records


def project(
    catalog: Catalog,
    batch_id: str,
    *,
    english: English | None = None,
) -> bytes:
    """Preserve the JP selection and add only registry-selected EN printings."""
    records = _records(catalog, batch_id, "jp")
    if english is not None:
        en_records = _records(catalog, english.batch_id, "en", english)
        if records.keys() & en_records.keys():
            raise ValueError("Ambiguous current engine printing across regions")
        records.update(en_records)
    return b"\n".join(records[number] for number in sorted(records)) + b"\n"
