"""Invert current annotations and asymmetric official related-card links."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, string

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from sve_carddb.export.project.source import Record, View


class _Uses:
    def __init__(
        self, field_annotations: Sequence[Record], translations: Sequence[Record]
    ) -> None:
        self.fields = {
            canonical({key: row[key] for key in ("owner", "field", "ordinal")}): string(
                row["annotation_set_id"]
            )
            for row in field_annotations
        }
        self.sections: dict[bytes, set[str]] = defaultdict(set)
        for row in field_annotations:
            if row["field"] == "section":
                self.sections[canonical(row["owner"])].add(
                    string(row["annotation_set_id"])
                )
        self.translations = {row["id"]: row for row in translations}

    def selected(self, revision: Record) -> set[str]:
        owner: Record = {"kind": "face_revision", "id": revision["id"]}
        used = self.sections[canonical(owner)].copy()
        self._original(used, {"owner": owner, "field": "effect", "ordinal": None})
        for raw in array(revision["translations"]):
            selection = object_value(raw)
            if selection["field"] not in {"effect", "section"}:
                continue
            translated = self.translations[selection["translation_id"]]
            if translated["annotation_set_id"] is not None:
                used.add(string(translated["annotation_set_id"]))
            source = object_value(selection["source"])
            self._original(used, source)
            # EN may select a JP source with different section coordinates.
            used.update(self.sections[canonical(source["owner"])])
        return used

    def _original(self, used: set[str], pointer: Record) -> None:
        identifier = self.fields.get(canonical(pointer))
        if identifier is not None:
            used.add(identifier)


def _targets(annotation: Record, targets: Mapping[str, tuple[str, ...]]) -> set[str]:
    result: set[str] = set()
    for raw in array(annotation["occurrences"]):
        reference = object_value(object_value(raw)["reference"])
        if reference["kind"] == "card_name":
            result.update(targets.get(string(reference["term_id"]), ()))
    return result


def _official_asymmetric(view: View) -> set[tuple[str, str, str]]:
    pairs = {
        (string(row["from_card_id"]), string(row["to_card_id"]))
        for row in view["card_related"]
    }
    regions: dict[str, set[str]] = defaultdict(set)
    for printing in view["printing"]:
        regions[string(printing["card_id"])].add(string(printing["region"]))
    cards = {string(row["id"]) for row in view["card"]}
    return {
        (string(row["to_card_id"]), region, string(row["from_card_id"]))
        for row in view["card_related"]
        if row["relation"] == "official_unspecified"
        and row["from_card_id"] in cards
        and row["to_card_id"] in cards
        and (row["to_card_id"], row["from_card_id"]) not in pairs
        for region in regions[string(row["from_card_id"])]
    }


def mentioned_by(
    view: View,
    field_annotations: Sequence[Record],
    annotations: Mapping[str, Record],
    targets: Mapping[str, tuple[str, ...]],
) -> list[Record]:
    """Union selected languages; the index describes card faces, not UI language."""
    uses = _Uses(field_annotations, view["translation"])
    revisions = {row["id"]: row for row in view["face_revision"]}
    cards = {string(row["id"]) for row in view["card"]}
    edges = _official_asymmetric(view)
    for face in view["face"]:
        for raw in array(face["current"]):
            current = object_value(raw)
            mentioned = set().union(
                *(
                    _targets(annotations[identifier], targets)
                    for identifier in uses.selected(revisions[current["revision_id"]])
                )
            )
            edges.update(
                (target, string(current["region"]), string(face["card_id"]))
                for target in mentioned & cards - {string(face["card_id"])}
            )
    return [
        {"card_id": target, "region": region, "from_card_id": source}
        for target, region, source in sorted(edges)
    ]
