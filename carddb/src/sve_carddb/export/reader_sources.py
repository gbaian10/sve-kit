"""File-local dictionaries and one-hop presentation of exact public source owners."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.export.references import DICTIONARIES, references, source_pointers
from sve_carddb.export.text_owners import TextOwners, row_owners

if TYPE_CHECKING:
    from sve_carddb.export.reader import Fragment, Row, View


def validate_local_dictionaries(
    fragments: list[Fragment], files: dict[str, Row]
) -> None:
    """A dictionary elsewhere in the snapshot cannot repair an incomplete file closure."""
    dictionaries: dict[str, set[tuple[str, str]]] = {}
    for fragment in fragments:
        if fragment.table in DICTIONARIES:
            dictionaries.setdefault(fragment.file, set()).update(
                (fragment.table, string(row["id"])) for row in fragment.rows
            )
    by_file: dict[str, list[Fragment]] = {}
    for fragment in fragments:
        by_file.setdefault(fragment.file, []).append(fragment)
    for key in files:
        available: set[tuple[str, str]] = set()
        pending = [key]
        visited: set[str] = set()
        while pending:
            target = pending.pop()
            if target in visited:
                continue
            visited.add(target)
            available.update(dictionaries.get(target, set()))
            pending.extend(
                string(object_value(ref)["key"])
                for ref in array(files[target]["dependencies"])
            )
        for fragment in by_file.get(key, []):
            for row in fragment.rows:
                if any(
                    (table, identifier) not in available
                    for table, identifier in references(row)
                    if table in DICTIONARIES
                ):
                    raise ValueError("Missing file-local dictionary reference")


def validate_source_references(
    view: View, fragments: list[Fragment], files: dict[str, Row]
) -> None:
    """Require exactly the owners' presentation files, independently of ordinary foreign keys."""
    owners = TextOwners(view)
    index = {
        (
            f.file,
            f.table,
            canonical(f.value["owner"]),
            string(f.value["segment"]),
            string(f.value["partition"]),
        ): f
        for f in fragments
    }
    contexts: dict[tuple[str, str], set[str]] = {}
    for fragment in fragments:
        if fragment.table == "face":
            for row in fragment.rows:
                contexts.setdefault(
                    (string(row["id"]), string(fragment.value["region"])), set()
                ).add(fragment.file)
    locations: dict[bytes, set[str]] = {}
    for fragment in fragments:
        source = fragment
        if fragment.value["base"] is not None:
            base = object_value(fragment.value["base"])
            source = index[
                string(object_value(base["file"])["key"]),
                fragment.table,
                canonical(base["owner"]),
                string(base["segment"]),
                string(base["partition"]),
            ]
        for row in source.rows:
            for owner in row_owners(fragment.table, row):
                info = owners.get(owner)
                keys = locations.setdefault(canonical(owner), set())
                keys.add(fragment.file)
                if info.card_id is not None and info.region is not None:
                    keys.update(contexts.get((info.face_id or "", info.region), set()))
    _source_references(owners, fragments, files, locations)


def _source_references(
    owners: TextOwners,
    fragments: list[Fragment],
    files: dict[str, Row],
    locations: dict[bytes, set[str]],
) -> None:
    expected: dict[str, set[str]] = {}
    for fragment in fragments:
        for row in fragment.rows:
            for pointer in source_pointers(row):
                owners.pointer(pointer)
                expected.setdefault(fragment.file, set()).update(
                    locations[canonical(pointer["owner"])]
                )
    for key, file in files.items():
        actual = {
            string(object_value(ref)["key"]) for ref in array(file["source_refs"])
        }
        if actual != expected.get(key, set()):
            raise ValueError("Source presentation closure mismatch")
