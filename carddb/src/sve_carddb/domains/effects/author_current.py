"""Admit each author card against the exact current JP database and source registry."""

from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.effects.author_expansion import (
    ExpansionCandidate,
    ExpansionError,
    ability_nodes,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_inputs import AuthorInputError, Diagnostic
from sve_carddb.domains.effects.author_set import load_author_set
from sve_carddb.domains.effects.author_source_hash import (
    AuthorSourcePayload,
    source_payload,
    source_version,
)
from sve_carddb.domains.effects.author_sources import (
    NO_TOKEN_METADATA,
    SourceContext,
    SourceError,
    entry_heads,
    entry_payload,
)
from sve_carddb.domains.translations.four_layer_authored import FrameRecord
from sve_carddb.domains.translations.four_layer_matching import Frames
from sve_carddb.domains.translations.four_layer_normalizer import VERSION
from sve_carddb.domains.translations.four_layer_pipeline import (
    compile_card_field,
    prepare_classifier,
)
from sve_carddb.domains.translations.four_layer_sources import card_field, descriptor
from sve_carddb.domains.translations.inputs import Snapshot, validate_snapshot
from sve_carddb.domains.translations.sources import pointer

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.contracts.author_leaves import IndexRecord
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_set import AuthorInputSet
    from sve_carddb.domains.effects.author_validation import PathParts
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary
    from sve_carddb.domains.translations.four_layer_pipeline import CompiledField
    from sve_carddb.domains.translations.recognition.rules import Rules
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class CardAccepted:
    record: AuthorIndexRecord
    candidate: ExpansionCandidate
    source_payload: bytes
    source_version: str
    source_inventory: bytes

    def lowering_input(self) -> JsonValue:
        """Hand current identities to D0 separately from the expanded author tree."""
        return {
            "tree": self.candidate.tree,
            "card_id": self.record.card_id,
            "card_no": self.record.record_key,
            "source_version": self.source_version,
            "source_payload": parse(self.source_payload),
            "source_inventory": parse(self.source_inventory),
            "sidecar": [
                {
                    "path": list(item.path),
                    "source_proof": item.source_proof,
                    "input_hash": item.input_hash,
                    "source_input_hash": item.source_input_hash,
                    "source_use": item.current_source.model_dump(mode="json")
                    if item.current_source is not None
                    else None,
                }
                for item in self.candidate.sidecar
            ],
        }


@dataclass(frozen=True)
class CardRejected:
    record: IndexRecord
    diagnostic: Diagnostic


type CardResult = CardAccepted | CardRejected


@dataclass(frozen=True)
class _CurrentCard:
    payload: AuthorSourcePayload
    fields: tuple[CompiledField, ...]
    revisions: dict[str, str]


def _text(value: Value) -> str:
    if not isinstance(value, str) or not value:
        raise SourceError("source_database_shape")
    return value


def _one(db: Database, table: str, where: Mapping[str, Value]) -> Mapping[str, Value]:
    rows = db.select(table, db.columns(table), where=where)
    if len(rows) != 1:
        raise SourceError("source_current_missing")
    return rows[0].values


def _known(value: JsonValue) -> dict[str, JsonValue]:
    return {"state": "unknown" if value is None else "known", "value": value}


class CurrentAuthorSources:
    """Current proofs are produced only after reconstructing the entire indexed card."""

    def __init__(
        self,
        db: Database,
        sources: Sources,
        snapshot: Snapshot,
        vocabulary: Vocabulary,
        rules: Rules,
        jp_batch_id: str,
    ) -> None:
        self.db = db
        self.sources = sources
        self.jp_batch_id = jp_batch_id
        # Rebuild cached models from the immutable snapshot bytes at this boundary.
        snapshot = Snapshot(snapshot.shards, snapshot.closure)
        try:
            validate_snapshot(snapshot)
            self.registry = {
                record.data.id: record.data
                for record in snapshot.four_layer.records
                if isinstance(record, FrameRecord)
            }
            self.frames = Frames(self.registry.values())
        except ValueError, TypeError:
            raise AuthorInputError(
                Diagnostic("source", "source_registry_invalid")
            ) from None
        try:
            self.classifier = prepare_classifier(
                db, snapshot, sources, vocabulary, rules
            )
        except ValueError, TypeError:
            raise AuthorInputError(
                Diagnostic("source", "source_classifier_invalid")
            ) from None

    def _printing_card(self, number: str, *, prototype: bool = False) -> str:
        rows = self.db.select(
            "printing",
            self.db.columns("printing"),
            where={"region": "jp", "card_no": number},
        )
        identities = {_text(row.values["card_id"]) for row in rows}
        if len(identities) != 1:
            raise SourceError(
                "prototype_identity_unresolved"
                if prototype
                else "record_key_identity_mismatch"
            )
        if any(row.values["card_no_state"] != "official" for row in rows):
            raise SourceError(
                "prototype_identity_unresolved"
                if prototype
                else "provisional_record_key"
            )
        return identities.pop()

    def _prepare(self, inputs: AuthorInputSet, expander: AuthorExpander) -> None:
        try:
            self._inventory(inputs, expander)
            self._companions(inputs, expander)
        except SourceError as error:
            raise AuthorInputError(Diagnostic("source", error.reason)) from None

    def _inventory(self, inputs: AuthorInputSet, expander: AuthorExpander) -> None:
        prototypes: set[str] = set()
        for file in inputs.prototype_files:
            tree = object_value(expander.schemas.prototype_tree(file.raw))
            prototypes.update(
                self._printing_card(number, prototype=True)
                for number in object_value(tree["cards"])
            )
        if prototypes & {
            record.card_id
            for record in inputs.index.records
            if isinstance(record, AuthorIndexRecord) and record.scope == "shared"
        }:
            raise SourceError("prototype_author_overlap")
        for record in inputs.index.records:
            if (
                not isinstance(record, AuthorIndexRecord)
                and self._printing_card(record.record_key, prototype=True)
                != record.card_id
            ):
                raise SourceError("prototype_index_identity_mismatch")

    def _companions(self, inputs: AuthorInputSet, expander: AuthorExpander) -> None:
        for file in inputs.body_files:
            tree = object_value(expander.schemas.body_tree(file.raw))
            for value in array(tree["records"]):
                body = object_value(value)
                frame = self.registry.get(string(body["frame_id"]))
                if frame is None:
                    raise SourceError("companion_registry_frame_missing")
                try:
                    heads = entry_heads(frame)
                except SourceError:
                    raise SourceError(
                        "companion_registry_projection_unsupported"
                    ) from None
                if body["ability_scope"] not in {
                    f"ability_{i}" for i in range(len(heads))
                }:
                    raise SourceError("companion_registry_scope_mismatch")
                if body["ports"] != {"imports": {}, "exports": {}}:
                    raise SourceError("companion_registry_ports_mismatch")

    @staticmethod
    def _supported(record: IndexRecord) -> AuthorIndexRecord:
        if not isinstance(record, AuthorIndexRecord):
            raise SourceError("unsupported_prototype")
        if record.scope == "en_override":
            raise SourceError("en_source_contract_unavailable")
        return record

    def validate(
        self, authored_root: Path, expander: AuthorExpander
    ) -> tuple[CardResult, ...]:
        """Set corruption is global; ordinary admission failure retains every card result."""
        if not isinstance(authored_root, Path):
            raise AuthorInputError(Diagnostic("source", "author_root_required"))
        inputs = load_author_set(authored_root, expander.schemas)
        self._prepare(inputs, expander)
        return self._validate(inputs, expander)

    def selection_input(
        self, authored_root: Path, expander: AuthorExpander
    ) -> JsonValue:
        """Select prototypes and complete current candidates from one checked inventory.

        Card-local failures remain explicit; consumers must not drop them or fall
        back to a prototype. This internal handoff grants no runtime qualification.
        """
        inputs = load_author_set(authored_root, expander.schemas)
        self._prepare(inputs, expander)
        authors: list[JsonValue] = []
        for result in self._validate(inputs, expander):
            if not isinstance(result.record, AuthorIndexRecord):
                continue
            authors.append(
                {"status": "accepted", "input": result.lowering_input()}
                if isinstance(result, CardAccepted)
                else {
                    "status": "rejected",
                    "card_no": result.record.record_key,
                    "reason": result.diagnostic.reason,
                }
            )
        return {
            "documents": [
                [file.path, file.raw.decode("utf-8")] for file in inputs.prototype_files
            ],
            "authors": authors,
        }

    def _validate(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> tuple[CardResult, ...]:
        files = {file.path: file for file in inputs.author_files}
        results: list[CardResult] = []
        for record in inputs.index.records:
            try:
                supported = self._supported(record)
                file = files[record.file]
                tree = object_value(expander.schemas.author_tree(file.raw))
                card = object_value(tree["cards"])[record.record_key]
                selected: JsonValue = {**tree, "cards": {record.record_key: card}}
                results.append(
                    self._validate_card(
                        supported, selected, file.input_hash, inputs, expander
                    )
                )
            # Reader/schema failures remain global, including expanded-schema failures.
            # Only source admission and semantic expansion errors are card-local.
            except (SourceError, ExpansionError) as error:
                results.append(
                    CardRejected(
                        record,
                        Diagnostic(
                            "source",
                            error.reason,
                            getattr(error, "path", ()),
                            card_id=record.card_id,
                            card_no=record.record_key,
                            scope=record.scope,
                            source=error.source
                            if isinstance(error, ExpansionError)
                            else None,
                        ),
                    )
                )
        return tuple(results)

    def _attributes(self, revision: Mapping[str, Value]) -> dict[str, JsonValue]:
        result: dict[str, JsonValue] = {}
        for key in ("class_code", "type_code", "cost", "attack", "defense"):
            value = revision[key]
            if value is not None and not isinstance(value, str | int):
                raise SourceError("source_database_shape")
            result[key] = _known(value)
            # Spells have no combat stats; this is known absence under P1 §4.2.
            if (
                value is None
                and revision["type_code"] == "spell"
                and key in {"attack", "defense"}
            ):
                result[key] = {"state": "known", "value": None}
        for key, table, column in (
            ("traits", "face_trait", "trait_code"),
            ("titles", "face_title", "title_code"),
            ("special_kinds", "face_special_kind", "special_kind_code"),
        ):
            values = sorted(
                _text(row.values[column])
                for row in self.db.select(
                    table,
                    (column,),
                    where={"revision_id": revision["id"]},
                )
            )
            result[key] = _known(list[JsonValue](values))
            # An EVOLVE follower has no printed cost; field cost stays with its base (CR 5.16.1.2).
            if (
                key == "special_kinds"
                and "evolve" in values
                and revision["type_code"] == "follower"
                and revision["cost"] is None
            ):
                result["cost"] = {"state": "known", "value": None}
        return result

    def _rules_names(self, face: str) -> list[JsonValue]:
        result: list[tuple[str, str]] = []
        for row in self.db.select(
            "face_rules_name",
            self.db.columns("face_rules_name"),
            where={"face_id": face, "region": "jp"},
        ):
            identifier, role = (
                _text(row.values["rules_name_id"]),
                _text(row.values["role"]),
            )
            if _one(self.db, "rules_name", {"id": identifier})["region"] != "jp":
                raise SourceError("source_rules_name_mismatch")
            result.append((identifier, role))
        if sum(role == "primary" for _, role in result) != 1:
            raise SourceError("source_rules_name_mismatch")
        return [
            {"rules_name_id": identifier, "role": role}
            for identifier, role in sorted(result)
        ]

    def _card_payload(self, card_id: str) -> _CurrentCard:
        if _one(self.db, "card", {"id": card_id})["identity_state"] != "confirmed":
            raise SourceError("source_identity_unconfirmed")
        faces: list[JsonValue] = []
        fields: list[CompiledField] = []
        revisions: dict[str, str] = {}
        for face in self.db.select(
            "face", self.db.columns("face"), where={"card_id": card_id}
        ):
            face_id = _text(face.values["id"])
            current = _one(
                self.db, "face_current", {"face_id": face_id, "region": "jp"}
            )
            revision_id = _text(current["revision_id"])
            revision = _one(self.db, "face_revision", {"id": revision_id})
            if revision["face_id"] != face_id or revision["region"] != "jp":
                raise SourceError("source_current_mismatch")
            revisions[revision_id] = face_id
            sections = self.db.select(
                "face_text_section", ("ordinal",), where={"revision_id": revision_id}
            )
            if [row.values["ordinal"] for row in sections] != list(
                range(len(sections))
            ):
                raise SourceError("source_sections_incomplete")
            owner = FaceRevisionOwner(kind="face_revision", revision_id=revision_id)
            locations = [
                OwnerField(owner=owner, field="name", ordinal=None),
                OwnerField(owner=owner, field="effect", ordinal=None),
            ]
            locations.extend(
                OwnerField(owner=owner, field="section", ordinal=index)
                for index in range(len(sections))
            )
            texts, compiled = self._texts(locations)
            fields.extend(compiled)
            faces.append(
                {
                    "face_id": face_id,
                    "revision_id": revision_id,
                    "texts": texts,
                    "attributes": self._attributes(revision),
                    "rules_names": self._rules_names(face_id),
                }
            )
        if not faces:
            raise SourceError("source_current_missing")
        self._no_tokens(fields)
        try:
            payload = AuthorSourcePayload.model_validate_json(
                canonical(
                    {
                        "recipe": "effect-source-v1",
                        "normalizer": VERSION,
                        "region": "jp",
                        "faces": faces,
                        "token_links": [],
                    }
                )
            )
        except ValueError:
            raise SourceError("source_payload_invalid") from None
        return _CurrentCard(payload, tuple(fields), revisions)

    def _texts(
        self, locations: list[OwnerField]
    ) -> tuple[list[JsonValue], list[CompiledField]]:
        texts: list[JsonValue] = []
        fields: list[CompiledField] = []
        for location in locations:
            section_count: int | None = None
            try:
                selected = card_field(self.db, location)
                source = descriptor(self.db, self.sources, location, self.jp_batch_id)
                if location.field == "name":
                    ref = source.source_ref
                    _, document, _ = self.sources.document(
                        SourceRef(
                            batch_id=ref.batch_id,
                            source_version_id=ref.source_version_id,
                            parser=ref.parser,
                            locator=ref.locator,
                            text_hash="sha256:" + ref.text_hash,
                        )
                    )
                    section_count = len(
                        array(
                            pointer(
                                document, f"/faces/{selected.face_ordinal}/sections"
                            )
                        )
                    )
                if location.field != "name" and selected.text:
                    fields.append(
                        compile_card_field(
                            self.db, self.sources, source, self.classifier, self.frames
                        )
                    )
            except SourceError:
                raise
            except ValueError, TypeError:
                raise SourceError("source_reconstruction_failed") from None
            if location.field == "name" and not selected.text:
                raise SourceError("source_name_missing")
            if location.field == "name" and section_count != len(locations) - 2:
                raise SourceError("source_sections_incomplete")
            texts.append(
                {
                    "field": location.field,
                    "ordinal": location.ordinal,
                    "state": "present" if selected.text else "unknown",
                    "source_hash": digest(selected.text.encode())[7:]
                    if selected.text
                    else None,
                }
            )
        return texts, fields

    @staticmethod
    def _no_tokens(fields: list[CompiledField]) -> None:
        if any(
            part.source_span.role == "token_header"
            for field in fields
            for part in field.field.parts
        ):
            raise SourceError("token_source_contract_unavailable")
        for field in fields:
            if not field.field.parts:
                raise SourceError("token_source_contract_unavailable")
            for index, recognized in enumerate(field.derived.recognized):
                if recognized.issues:
                    raise SourceError("source_unresolved_issue")
                if field.derived.pending_causes[index]:
                    raise SourceError("source_pending")
                # Dependency analysis precedes registry admission and never supplies its binding.
                frame, _ = field.derived.bind(index)
                if (
                    frame.projection.projection_kind == "none"
                    and frame.semantic_variant.state == "resolved"
                    and frame.semantic_variant.key in NO_TOKEN_METADATA
                ):
                    continue
                try:
                    entry_heads(frame)
                except SourceError:
                    raise SourceError("unsupported_source_projection") from None

    def _contexts(self, card: _CurrentCard) -> dict[tuple[str, str], SourceContext]:
        result: dict[tuple[str, str], SourceContext] = {}
        for field in card.fields:
            for index, matched in enumerate(field.matches):
                if matched is None:
                    raise SourceError("source_registry_mismatch")
                try:
                    field.derived.verify_binding(
                        field.source.text,
                        self.classifier,
                        matched.frame,
                        matched.binding,
                    )
                except ValueError:
                    raise SourceError("source_registry_mismatch") from None
                if matched.frame.projection.projection_kind == "none":
                    continue
                heads = entry_heads(matched.frame)
                source = SourceContext(
                    matched.frame,
                    matched.binding,
                    field.field.parts[index].canonical_source,
                    self.classifier,
                )
                for root in range(len(heads)):
                    result[matched.binding.id, f"ability_{root}"] = source
        return result

    def _validate_card(
        self,
        record: AuthorIndexRecord,
        tree: JsonValue,
        input_hash: str,
        inputs: AuthorInputSet,
        expander: AuthorExpander,
    ) -> CardAccepted:
        if self._printing_card(record.record_key) != record.card_id:
            raise SourceError("record_key_identity_mismatch")
        card = self._card_payload(record.card_id)
        payload = source_payload(card.payload)
        version = source_version(card.payload)
        for face in array(object_value(card.payload.model_dump(mode="json"))["faces"]):
            data = object_value(face)
            if any(
                object_value(value)["state"] == "unknown"
                for value in object_value(data["attributes"]).values()
            ) or any(
                object_value(value)["state"] == "unknown"
                for value in array(data["texts"])
            ):
                raise SourceError("source_unknown")
        authored = object_value(
            object_value(object_value(tree)["cards"])[record.record_key]
        )
        if object_value(authored["meta"])["source"] != version:
            raise SourceError("source_version_mismatch")
        expected = self._contexts(card)
        inventory = self._source_inventory(card, expected)
        contexts = self._uses(card, tree, expected)
        result = expander.expand_contexts(
            canonical(tree), (file.raw for file in inputs.body_files), contexts
        )
        checked_uses = {}
        for path, ability in ability_nodes(result.tree):
            use = SourceUse.model_validate_json(
                canonical(array(ability["sources"])[0]), strict=True
            )
            if equivalent_ability(ability) != canonical(
                entry_payload(contexts[path], use.ability_scope)
            ):
                raise ExpansionError("source_head_payload_mismatch", path, use)
            checked_uses[path] = use
        evidence = []
        for item in result.sidecar:
            checked = replace(
                item,
                input_hash=input_hash,
                source_input_hash="sha256:"
                + contexts[item.path].binding.source.source_hash,
                current_source=checked_uses[item.path],
            )
            object.__setattr__(checked, "source_proof", "current_source_checked")  # ruff: ignore[unnecessary-dunder-call] -- only the complete current boundary grants this proof
            evidence.append(checked)
        return CardAccepted(
            record,
            ExpansionCandidate(result.tree, tuple(evidence)),
            payload,
            version,
            inventory,
        )

    @staticmethod
    def _source_inventory(
        card: _CurrentCard, expected: Mapping[tuple[str, str], SourceContext]
    ) -> bytes:
        # Derive obligations from current sources, never from the supplied author tree.
        abilities: list[JsonValue] = [
            {
                "source_use": SourceUse(
                    binding_id=binding,
                    occurrence=source.binding.occurrence_key(),
                    ability_scope=scope,
                ).model_dump(mode="json"),
                "body": entry_payload(source, scope),
            }
            for (binding, scope), source in expected.items()
        ]
        non_body: list[JsonValue] = [
            {
                "binding_id": matched.binding.id,
                "occurrence": matched.binding.occurrence_key().model_dump(mode="json"),
                "semantic_key": matched.frame.semantic_variant.key,
            }
            for field in card.fields
            for matched in field.matches
            if matched is not None
            and matched.frame.projection.projection_kind == "none"
        ]
        return canonical({"abilities": abilities, "non_body": non_body})

    def _uses(
        self,
        card: _CurrentCard,
        tree: JsonValue,
        expected: Mapping[tuple[str, str], SourceContext],
    ) -> dict[PathParts, SourceContext]:
        for value in object_value(object_value(tree)["cards"]).values():
            authored = object_value(value)
            if "deck_rule" in authored:
                raise SourceError("unsupported_source_deck_rule")
            if object_value(authored.get("faces", {})).keys() - set(
                card.revisions.values()
            ):
                raise SourceError("source_face_container_mismatch")
        used: set[tuple[str, str]] = set()
        contexts: dict[PathParts, SourceContext] = {}
        for path, ability in ability_nodes(tree):
            use = SourceUse.model_validate_json(
                canonical(array(ability["sources"])[0]), strict=True
            )
            try:
                source = self._use_context(card, expected, path, ability, use)
            except ExpansionError as error:
                error.source = use
                raise
            key = use.binding_id, use.ability_scope
            if key in used:
                raise ExpansionError("duplicate_source_ability", path, use)
            used.add(key)
            contexts[path] = source
        if used != expected.keys():
            raise SourceError("source_ability_closure_mismatch")
        return contexts

    @staticmethod
    def _use_context(
        card: _CurrentCard,
        expected: Mapping[tuple[str, str], SourceContext],
        path: PathParts,
        ability: dict[str, JsonValue],
        use: SourceUse,
    ) -> SourceContext:
        owner = use.occurrence.owner
        if not isinstance(owner, FaceRevisionOwner):
            raise ExpansionError("unsupported_source_owner_" + owner.kind, path)
        face = card.revisions.get(owner.revision_id)
        if face is None:
            raise ExpansionError("source_current_owner_mismatch", path)
        if "faces" in path and path[3] != face:
            raise ExpansionError("source_face_container_mismatch", path)
        if use.occurrence.field not in {"effect", "section"}:
            raise ExpansionError("unsupported_source_field", path)
        if (
            ability.get("section") != use.occurrence.ordinal
            or ability["line"] != use.occurrence.line_ordinal + 1
        ):
            raise ExpansionError("source_location_mismatch", path)
        key = use.binding_id, use.ability_scope
        source = expected.get(key)
        if source is None or source.binding.occurrence_key() != use.occurrence:
            raise ExpansionError("source_occurrence_mismatch", path)
        return source
