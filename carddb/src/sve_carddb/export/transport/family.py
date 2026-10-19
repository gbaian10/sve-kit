"""Family files and finite regional rows, independent of release chronology."""

from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import quote

from pydantic import JsonValue

from sve_carddb.contracts.snapshot import definition, row_type
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.export.references import DICTIONARIES, source_pointers
from sve_carddb.export.references import references as public_references
from sve_carddb.export.text_owners import TextOwners, row_owners
from sve_carddb.export.transport.layout import (
    IMAGE_TABLES,
    Layout,
    concept_references,
    ordered,
)
from sve_carddb.export.transport.wire import container, encode

if TYPE_CHECKING:
    from sve_carddb.export.transport import _Files


def file_reference(file: dict[str, JsonValue]) -> dict[str, JsonValue]:
    """Pin the sealed file without including its hash in its own payload."""
    return {"key": file["key"], "sha256": file["sha256"]}


type Row = dict[str, JsonValue]
CATALOG = frozenset(
    {
        "card",
        "face",
        "face_revision",
        "product_family",
        "vocabulary",
        "keyword",
        "search_alias",
    }
)
PRINT = frozenset(
    {"printing", "printing_product", "art", "artist", "stamp", "digital_art_link"}
)
ENGINE = frozenset(
    {
        "card_related",
        "card_mentioned_by",
        "face_rules_name",
        "card_engine_support",
        "rules_name",
        "rules_profile",
        "restriction",
        "mechanic_projection",
        "card_mechanic_coverage",
        "identity_change",
        "digital_link",
        "digital_link_coverage",
        "digital_card",
        "digital_art",
        "voice",
        "card_voice",
        "card_route_alias",
        "route_override",
    }
)


@dataclass(frozen=True, order=True)
class Scope:
    purpose: str
    region: str
    family: str
    band: int = 0

    @property
    def segment(self) -> str:
        """A family is stable; only background uses a fixed band suffix."""
        family = "family-" + self.family if self.family else "shared"
        return (
            family if self.purpose == "catalog" else family + ":band-" + str(self.band)
        )

    @property
    def key(self) -> str:
        """Separate File scope from permanent row owners."""
        family = "family-" + quote(self.family, safe="") if self.family else "shared"
        return "/".join((self.purpose, self.region or "shared", family, str(self.band)))


@dataclass
class Piece:
    table: str
    family: str
    partition: str
    segment: str
    rows: list[Row]
    base: tuple[Scope, tuple[str, str, str, str]] | None = None

    @property
    def owner(self) -> Row:
        """Encode the permanent owner independently of File scope."""
        return {
            "kind": "home_set" if self.family else "global",
            "id": self.family or None,
        }


class FamilyLayout:
    def __init__(self, layout: Layout) -> None:
        self.layout = layout
        self.pieces: dict[Scope, dict[tuple[str, str, str, str], Piece]] = {}
        self.locators: dict[Scope, dict[str, list[JsonValue]]] = {}
        self.owners = TextOwners(layout.view)
        self.dictionary = {
            t: {string(r["id"]): r for r in layout.view[t]} for t in DICTIONARIES
        }
        self.shared_dependencies: dict[Scope, set[Scope]] = {}
        self.visible: dict[str, set[str]] = {}
        for printing in layout.view["printing"]:
            self.visible.setdefault(string(printing["card_id"]), set()).add(
                string(printing["region"])
            )
        self.defaults = {
            object_value(r)["default_printing_id"]
            for c in layout.view["card"]
            for r in array(c["regions"])
        }

    def _add(
        self,
        scope: Scope,
        table: str,
        row: Row,
        part: str,
        *,
        coordinates: tuple[str, str] | None = None,
        base: tuple[Scope, tuple[str, str, str, str]] | None = None,
    ) -> None:
        owner, seg = coordinates or (scope.family, scope.segment)
        identity = (table, owner, part, seg)
        pieces = self.pieces.setdefault(scope, {})
        piece = pieces.setdefault(identity, Piece(table, owner, part, seg, [], base))
        if piece.base != base:
            raise ValueError("Conflicting exact fragment base")
        piece.rows.append(row)

    def scope(
        self, table: str, row: Row, part: str, purpose: str, region: str
    ) -> Scope:
        """Derive physical membership only from the fixed exception list."""
        group = self.layout.group(table, row, part)
        width = self.layout.profile.width(purpose, group.identifier)
        return Scope(
            purpose,
            region,
            group.identifier,
            0 if purpose == "catalog" else group.bucket // width,
        )

    def _prepare(self) -> None:
        mapping = object_value(
            definition("Container", self.layout.profile.version)["x-fragments"]
        )
        for table, rows in self.layout.view.items():
            if table in DICTIONARIES or table == "field_annotation":
                continue
            for row in rows:
                part = (
                    "history"
                    if table == "face_revision" and row["id"] not in self.layout.display
                    else "bootstrap"
                    if "bootstrap" in object_value(mapping[table])
                    else "detail"
                )
                if table in {"card", "face"}:
                    self._regional(table, row, part)
                else:
                    purpose, region = self._row_scope(table, row, part)
                    self._add(
                        self.scope(table, row, part, purpose, region), table, row, part
                    )
        for printing in self.layout.view["printing"]:
            if printing["id"] not in self.defaults:
                scope = self.scope(
                    "printing",
                    printing,
                    "bootstrap",
                    "catalog",
                    string(printing["region"]),
                )
                self.locators.setdefault(scope, {}).setdefault(scope.family, []).append(
                    [printing["id"], printing["card_id"], printing["card_no"]]
                )
                self.pieces.setdefault(scope, {})
        self._details()
        self._annotations()
        common = Scope("catalog", "", "")
        for row in self.layout.view["annotation_concept"]:
            self._add(common, "annotation_concept", row, "bootstrap")
        self._dictionaries()

    def _translation_scope(self, selection: Row) -> Scope | None:
        pointer = object_value(selection["source"])
        info = self.owners.get(object_value(pointer["owner"]))
        if (
            selection["field"] == "name"
            or selection["target_lang"] != "zh-Hant"
            or info.region != "jp"
            or selection["basis"] not in {"own_source", "jp_source"}
        ):
            return None
        if info.owner["kind"] == "printing_face":
            table = "printing"
            row = self.layout.printings[string(info.owner["id"])]
        else:
            table, row = "face_revision", info.row
        return self.scope(table, row, "detail", "translation", "")

    def _dictionaries(self) -> None:
        shared: dict[Scope, dict[str, Row]] = {}
        for scope, pieces in list(self.pieces.items()):
            roots = [
                self.layout.record(p.table, r, p.partition, i)
                for p in pieces.values()
                for i, r in enumerate(self._ordered(p))
            ]
            if scope.purpose == "catalog":
                roots.extend(
                    {"source_unit_id": w[1], "name_concept_id": w[2]}
                    for w in self._witnesses(scope)
                )
            external: set[str] = set()
            for root in roots:
                for selection in translation_selections(root):
                    target = self._translation_scope(selection)
                    if target is None:
                        continue
                    identifier = string(selection["translation_id"])
                    external.add(identifier)
                    shared.setdefault(target, {})[identifier] = self.dictionary[
                        "translation"
                    ][identifier]
                    self.shared_dependencies.setdefault(scope, set()).add(target)
            part = "bootstrap" if scope.purpose == "catalog" else "detail"
            self._local_dictionaries(scope, roots, external, part)
        for scope, translations in shared.items():
            for table, rows in self._closure(list(translations.values())).items():
                for row in rows.values():
                    self._add(
                        scope, table, row, "detail", coordinates=("", scope.segment)
                    )
            for row in translations.values():
                self._add(
                    scope, "translation", row, "detail", coordinates=("", scope.segment)
                )

    def _local_dictionaries(
        self,
        scope: Scope,
        roots: list[Row],
        external: set[str],
        part: str,
    ) -> None:
        # Only this file's selections may remove its local copies; new users of the shared file must not move them.
        supplied = self._closure(
            [
                self.dictionary["translation"][identifier]
                for identifier in sorted(external)
            ]
        )
        for table, rows in self._closure(roots, external).items():
            for identifier, row in rows.items():
                if identifier not in supplied[table]:
                    self._add(scope, table, row, part, coordinates=("", scope.segment))

    def _row_scope(self, table: str, row: Row, part: str) -> tuple[str, str]:
        purpose = (
            "images"
            if table in IMAGE_TABLES
            else "history"
            if part == "history"
            else "catalog"
            if table in CATALOG
            else "print"
            if table in PRINT
            else "product"
            if table == "product"
            else "engine"
            if table in ENGINE
            else "detail"
        )
        if table == "printing" and row["id"] in self.defaults:
            purpose = "catalog"
        if purpose == "images" or table == "card_mentioned_by":
            return purpose, ""
        if table in {"printing_product", "printing_image"}:
            return purpose, string(
                self.layout.printings[string(row["printing_id"])]["region"]
            )
        return purpose, string(row["region"]) if row.get("region") is not None else ""

    def _regional(self, table: str, row: Row, part: str) -> None:
        card = row if table == "card" else self.layout.cards[string(row["card_id"])]
        regions = {string(object_value(r)["region"]) for r in array(card["regions"])}
        for selected in sorted(regions):
            regional = row.copy()
            for field in ("regions",) if table == "card" else ("current", "wording"):
                regional[field] = [
                    r
                    for r in array(row[field])
                    if object_value(r)["region"] == selected
                ]
            target = (
                "catalog"
                if selected in self.visible.get(string(card["id"]), set())
                else "region-state"
            )
            self._add(
                self.scope(table, row, part, target, selected), table, regional, part
            )

    @staticmethod
    def _ordered(piece: Piece) -> list[Row]:
        return ordered(piece.table, piece.rows)

    def _details(self) -> None:
        for scope, pieces in list(self.pieces.items()):
            for identity, piece in list(pieces.items()):
                if (
                    piece.table not in {"face_revision", "printing"}
                    or piece.partition != "bootstrap"
                ):
                    continue
                purpose = "print-detail" if piece.table == "printing" else "detail"
                target = Scope(purpose, scope.region, scope.family, scope.band)
                # Two bases in one detail file retain distinct purpose/band coordinates.
                segment = scope.purpose + ":" + scope.segment
                for row in self._ordered(piece):
                    self._add(
                        target,
                        piece.table,
                        row,
                        "detail",
                        coordinates=(scope.family, segment),
                        base=(scope, identity),
                    )

    def _annotations(self) -> None:
        uses: dict[bytes, list[Row]] = {}
        for row in self.layout.view["field_annotation"]:
            uses.setdefault(canonical(row["owner"]), []).append(row)
        found: set[bytes] = set()
        for scope, pieces in list(self.pieces.items()):
            for piece in list(pieces.values()):
                selected = [
                    use
                    for row in piece.rows
                    for owner in row_owners(piece.table, row)
                    for use in uses.get(canonical(owner), [])
                    if self.layout.field_partition(use) == piece.partition
                ]
                for use in selected:
                    self._add(
                        scope,
                        "field_annotation",
                        use,
                        piece.partition,
                        coordinates=(piece.family, piece.segment),
                    )
                    found.add(canonical(use))
        if found != {canonical(r) for r in self.layout.view["field_annotation"]}:
            raise ValueError("Missing original annotation placement")

    def _closure(
        self, roots: list[Row], external: set[str] | None = None
    ) -> dict[str, dict[str, Row]]:
        selected: dict[str, dict[str, Row]] = {t: {} for t in DICTIONARIES}
        queue = roots.copy()
        while queue:
            row = queue.pop()
            refs = list(public_references(row))
            if row.get("name_concept_id") is not None:
                refs.append(("annotation_concept", string(row["name_concept_id"])))
            if "occurrences" in row:
                refs.extend(("annotation_concept", i) for i in concept_references(row))
            for table, identifier in refs:
                if table == "translation" and identifier in (external or set()):
                    continue
                if table in DICTIONARIES and identifier not in selected[table]:
                    if table == "annotation_concept":
                        continue
                    try:
                        value = self.dictionary[table][identifier]
                    except KeyError:
                        raise ValueError("Missing dictionary reference") from None
                    selected[table][identifier] = value
                    queue.append(value)
        return selected

    def _witnesses(self, scope: Scope) -> list[list[JsonValue]]:
        available: set[bytes] = set()
        pointers: dict[bytes, Row] = {}
        for piece in self.pieces[scope].values():
            for i, row in enumerate(self._ordered(piece)):
                encoded = self.layout.record(piece.table, row, piece.partition, i)
                pointers.update(
                    (canonical(pointer), pointer)
                    for pointer in source_pointers(encoded)
                )
                for owner in row_owners(piece.table, row):
                    available.update(
                        canonical({"owner": owner, "field": field, "ordinal": None})
                        for field in available_fields(piece.table, piece.partition)
                    )
                    if piece.table == "keyword":
                        available.update(
                            canonical(
                                {
                                    "owner": owner,
                                    "field": "action_label",
                                    "ordinal": ordinal,
                                }
                            )
                            for ordinal, _ in enumerate(array(row["actions"]))
                        )
        try:
            return [
                [
                    pointer,
                    self.owners.pointer(pointer),
                    self.owners.get(object_value(pointer["owner"])).row.get(
                        "name_concept_id"
                    ),
                ]
                for key, pointer in sorted(pointers.items())
                if key not in available
            ]
        except ValueError:
            raise ValueError("public-annotation/owner") from None

    def seal(self, files: _Files, config: Row) -> None:
        """Seal strong dependencies before publishing exact one-hop source references."""
        self._prepare()
        sealed: dict[Scope, Row] = {}
        locations: dict[str, dict[tuple[str, str], Row]] = {"printing": {}, "face": {}}
        ordered = sorted(
            self.pieces,
            key=lambda s: (
                -2
                if s == Scope("catalog", "", "")
                else -1
                if s.purpose == "translation"
                else 0
                if s.purpose == "catalog"
                else 1
                if s.purpose in {"print", "region-state"}
                else 3
                if s.purpose == "images"
                else 2,
                s,
            ),
        )
        for scope in ordered:
            pieces = self.pieces[scope]
            metadata: dict[str, list[JsonValue]] = {}
            dependencies: dict[str, Row] = {}
            for target in self.shared_dependencies.get(scope, set()):
                dependencies[target.key] = file_reference(sealed[target])
            if scope.purpose == "images":
                dependencies[string(config["key"])] = file_reference(config)
            elif scope != Scope("catalog", "", ""):
                # Manifest dependencies do not rewrite family payloads when the common dictionary changes.
                common = sealed[Scope("catalog", "", "")]
                dependencies[string(common["key"])] = file_reference(common)
            for _, piece in sorted(pieces.items()):
                metadata.setdefault(piece.table, []).append(
                    self._fragment(scope, piece, sealed, locations, dependencies)
                )
            value = container(
                dict(sorted(metadata.items())),
                self.layout.profile.version,
            )
            self._extras(scope, value)
            role = (
                "bootstrap"
                if scope.purpose == "catalog"
                else "images"
                if scope.purpose == "images"
                else "text"
            )
            file = files.add(
                scope.key,
                role,
                value,
                [dependencies[k] for k in sorted(dependencies)],
                scope=scope,
            )
            sealed[scope] = file
            for piece in pieces.values():
                if piece.partition == "bootstrap" and piece.table in locations:
                    for row in piece.rows:
                        locations[piece.table][string(row["id"]), scope.region] = (
                            file_reference(file)
                        )

        for key, targets in self._source_files().items():
            files.files[key]["source_refs"] = [
                file_reference(files.files[t]) for t in sorted(targets)
            ]

    def _extras(self, scope: Scope, value: Row) -> None:
        if scope in self.locators:
            value["card_locators"] = {
                family: sorted(rows, key=canonical)
                for family, rows in sorted(self.locators[scope].items())
            }
        if scope.purpose == "catalog":
            sources = self._witnesses(scope)
            if sources:
                value["name_sources"] = [
                    [
                        encode(
                            "PublicTextPointer",
                            object_value(w[0]),
                            self.layout.profile.version,
                        ),
                        w[1],
                        w[2],
                    ]
                    for w in sources
                ]

    def _fragment(
        self,
        scope: Scope,
        piece: Piece,
        sealed: dict[Scope, Row],
        locations: dict[str, dict[tuple[str, str], Row]],
        dependencies: dict[str, Row],
    ) -> Row:
        base: JsonValue = None
        if piece.base is not None:
            source, identity = piece.base
            file = sealed[source]
            original = self.pieces[source][identity]
            base = {
                "file": file_reference(file),
                "table": piece.table,
                "owner": original.owner,
                "partition": original.partition,
                "segment": original.segment,
            }
            dependencies[string(file["key"])] = file_reference(file)
        if scope.purpose == "images" and piece.table == "printing_image":
            for row in piece.rows:
                for table, field in (("printing", "printing_id"), ("face", "face_id")):
                    region = string(
                        self.layout.printings[string(row["printing_id"])]["region"]
                    )
                    ref = locations[table][string(row[field]), region]
                    dependencies[string(ref["key"])] = ref
        name = row_type(piece.table, piece.partition, self.layout.profile.version)
        return {
            "owner": piece.owner,
            "partition": piece.partition,
            "segment": piece.segment,
            "base": base,
            "rows": [
                encode(
                    name,
                    self.layout.record(piece.table, r, piece.partition, i),
                    self.layout.profile.version,
                )
                for i, r in enumerate(self._ordered(piece))
            ],
        }

    def _source_files(self) -> dict[str, frozenset[str]]:
        locations: dict[bytes, set[str]] = {}
        sources: dict[str, set[bytes]] = {}
        for scope, pieces in self.pieces.items():
            for piece in pieces.values():
                for i, row in enumerate(self._ordered(piece)):
                    for owner in row_owners(piece.table, row):
                        targets = locations.setdefault(canonical(owner), set())
                        targets.add(scope.key)
                        targets.update(self._source_context(owner))
                    value = self.layout.record(piece.table, row, piece.partition, i)
                    sources.setdefault(scope.key, set()).update(
                        canonical(pointer["owner"])
                        for pointer in source_pointers(value)
                    )
        return {
            key: frozenset(source for owner in owners for source in locations[owner])
            for key, owners in sources.items()
        }

    def _source_context(self, owner: Row) -> set[str]:
        info = self.owners.get(owner)
        if info.card_id is None or info.region is None:
            return set()
        purpose = (
            "catalog"
            if info.region in self.visible.get(info.card_id, set())
            else "region-state"
        )
        if info.face_id is None:
            return set()
        context = self.scope(
            "face", self.layout.faces[info.face_id], "bootstrap", purpose, info.region
        )
        return {context.key} if context in self.pieces else set()


def available_fields(table: str, partition: str) -> tuple[str, ...]:
    """Omit name witnesses when this same file already carries the original field."""
    if table == "face_revision":
        return (
            ("name",)
            if partition == "bootstrap"
            else ("effect",)
            if partition == "detail"
            else ("name", "effect")
        )
    if table == "printing":
        return () if partition == "bootstrap" else ("name", "effect", "flavor")
    if table == "keyword":
        return ("label", "effect")
    return ("label",) if table in {"product", "product_family", "vocabulary"} else ()


def translation_selections(value: JsonValue) -> list[Row]:
    """Route JP-origin translations before any English printing exists."""
    if isinstance(value, list):
        return [s for item in value for s in translation_selections(item)]
    if not isinstance(value, dict):
        return []
    return ([value] if "translation_id" in value and "source" in value else []) + [
        s for item in value.values() for s in translation_selections(item)
    ]
