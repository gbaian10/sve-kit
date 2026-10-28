"""Admit each author card against the exact current JP database and source registry."""

from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import CardKeywordSourceUse, SourceUse
from sve_carddb.contracts.author_handoff import SourceInventory
from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.effects.author_expansion import (
    ExpansionCandidate,
    ExpansionError,
    ability_nodes,
    check_keyword_declaration,
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
    PrintedKeywordObligation,
    SourceContext,
    SourceError,
    card_keyword_values,
    entry_heads,
    entry_payload,
)
from sve_carddb.domains.physical_sources import page
from sve_carddb.domains.text_observations.archive import verify_card
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
    from sve_carddb.contracts.source_binding import SourceDescriptor
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
        if self.source_version == "direct":
            return {
                "admission": "direct",
                "tree": self.candidate.tree,
                "card_id": self.record.card_id,
                "card_no": self.record.record_key,
                "source_payload": parse(self.source_payload),
            }
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
            "declaration_sidecar": [
                item.lowering_input() for item in self.candidate.declarations
            ],
        }


@dataclass(frozen=True)
class CardRejected:
    record: IndexRecord
    diagnostic: Diagnostic


type CardResult = CardAccepted | CardRejected


@dataclass(frozen=True)
class CurrentCard:
    payload: AuthorSourcePayload
    fields: tuple[CompiledField, ...]
    revisions: dict[str, str]
    face_positions: dict[str, tuple[int, str]]


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

    def printing_card(self, number: str, *, prototype: bool = False) -> str:
        """Resolve an official JP printing only when all rows agree on identity."""
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

    def printing_numbers(self, card_id: str) -> list[str]:
        """Resolve official JP aliases through permanent identity, never number spelling."""
        rows = self.db.select(
            "printing",
            self.db.columns("printing"),
            where={"region": "jp", "card_id": card_id, "card_no_state": "official"},
        )
        numbers = sorted({_text(row.values["card_no"]) for row in rows})
        for number in numbers:
            if self.printing_card(number) != card_id:
                raise SourceError("record_key_identity_mismatch")
        return numbers

    def keyword_obligations(self, card_id: str) -> tuple[PrintedKeywordObligation, ...]:
        """Reconstruct complete current sources; this grants no author or D0 admission."""
        card = self.card_payload(card_id)
        return self._keyword_obligations(card, self.checked_sources(card))

    def _keyword_obligations(
        self, card: CurrentCard, sources: tuple[SourceContext, ...]
    ) -> tuple[PrintedKeywordObligation, ...]:
        self._require_known(card)
        if card.fields and any(
            "token" in (face.attributes.special_kinds.value or ())
            for face in card.payload.faces
        ):
            raise SourceError("token_source_contract_unavailable")
        result: list[PrintedKeywordObligation] = []
        used: set[tuple[str, str]] = set()
        bindings: set[str] = set()
        for source in sources:
            if source.frame.projection.projection_kind != "card_field":
                continue
            owner = source.binding.source.owner
            if (
                not isinstance(owner, FaceRevisionOwner)
                or owner.revision_id not in card.revisions
            ):
                raise SourceError("source_keyword_owner_mismatch")
            face_id = card.revisions[owner.revision_id]
            keywords = card_keyword_values(source)
            if source.binding.id in bindings or any(
                (face_id, keyword) in used for keyword in keywords
            ):
                raise SourceError("unsupported_duplicate_source_keyword")
            bindings.add(source.binding.id)
            used.update((face_id, keyword) for keyword in keywords)
            result.append(PrintedKeywordObligation(face_id, source, keywords))
        return tuple(result)

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
                self.printing_card(number, prototype=True)
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
                and self.printing_card(record.record_key, prototype=True)
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
        return self.validate_inputs(inputs, expander)

    def validate_inputs(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> tuple[CardResult, ...]:
        """Validate an in-memory complete inventory before writing any authored file."""
        from sve_carddb.domains.effects.author_set import validate_author_set  # ruff: ignore[import-outside-top-level] -- share the disk reader's complete inventory boundary

        validate_author_set(inputs, expander.schemas)
        inputs, failures = self.resolve_inputs(inputs, expander)
        self._prepare(inputs, expander)
        return self._validate(inputs, expander, failures)

    def selection_input(
        self, authored_root: Path, expander: AuthorExpander
    ) -> JsonValue:
        """Select prototypes and complete current candidates from one checked inventory.

        Card-local failures remain explicit; consumers must not drop them or fall
        back to a prototype. This internal handoff grants no runtime qualification.
        """
        return self.selection_inputs(
            load_author_set(authored_root, expander.schemas), expander
        )

    def selection_inputs(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> JsonValue:
        """Select from a complete in-memory semantic inventory without loading storage."""
        from sve_carddb.domains.effects.author_set import validate_author_set  # ruff: ignore[import-outside-top-level] -- avoid the inventory/expansion initialization cycle

        validate_author_set(inputs, expander.schemas)
        inputs, failures = self.resolve_inputs(inputs, expander)
        self._prepare(inputs, expander)
        authors: list[JsonValue] = []
        for result in self._validate(inputs, expander, failures):
            if not isinstance(result.record, AuthorIndexRecord):
                continue
            numbers = self.printing_numbers(result.record.card_id)
            if isinstance(result, CardAccepted):
                authors.append(
                    {
                        "status": "accepted",
                        "printings": numbers,
                        "input": result.lowering_input(),
                    }
                )
            else:
                authors.extend(
                    {
                        "status": "rejected",
                        "card_no": number,
                        "reason": result.diagnostic.reason,
                    }
                    for number in numbers or [result.record.record_key]
                )
        return {
            "documents": [
                [file.path, file.raw.decode("utf-8")] for file in inputs.prototype_files
            ],
            "authors": authors,
        }

    def resolve_inputs(
        self, inputs: AuthorInputSet, expander: AuthorExpander
    ) -> tuple[AuthorInputSet, dict[str, Diagnostic]]:
        """Resolve authored coordinates without persisted machine anchors."""
        from sve_carddb.domains.effects.author_resolution import (  # ruff: ignore[import-outside-top-level] -- materialization consumes this current-source boundary
            is_handwritten,
            materialize_inputs,
        )

        if not is_handwritten(inputs):
            return inputs, {}
        result = materialize_inputs(inputs, self, expander)
        return result.inputs, result.failures

    def _validate(
        self,
        inputs: AuthorInputSet,
        expander: AuthorExpander,
        failures: dict[str, Diagnostic] | None = None,
    ) -> tuple[CardResult, ...]:
        files = {file.path: file for file in inputs.author_files}
        results: list[CardResult] = []
        for record in inputs.index.records:
            if failures and record.record_key in failures:
                results.append(CardRejected(record, failures[record.record_key]))
                continue
            try:
                supported = self._supported(record)
                file = files[record.file]
                tree = object_value(expander.schemas.author_tree(file.raw))
                card = object_value(tree["cards"])[record.record_key]
                selected: JsonValue = {**tree, "cards": {record.record_key: card}}
                if object_value(object_value(card)["meta"]).get("source") == "direct":
                    from sve_carddb.domains.effects.author_direct import (  # ruff: ignore[import-outside-top-level] -- direct admission consumes the resolved name table
                        candidate,
                        inventory,
                    )

                    payload = inventory(
                        self, record.record_key, card_id=supported.card_id
                    )
                    results.append(
                        CardAccepted(
                            supported,
                            candidate(selected, expander),
                            canonical(payload),
                            "direct",
                            b"{}",
                        )
                    )
                    continue
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
        if revision["type_code"] == "leader" and all(
            revision[key] is None for key in ("cost", "attack", "defense")
        ):
            location = OwnerField(
                owner=FaceRevisionOwner(
                    kind="face_revision", revision_id=_text(revision["id"])
                ),
                field="name",
                ordinal=None,
            )
            try:
                selected = card_field(self.db, location)
                source = descriptor(self.db, self.sources, location, self.jp_batch_id)
                face = object_value(
                    pointer(self._document(source), f"/faces/{selected.face_ordinal}")
                )
            except ValueError, TypeError:
                raise SourceError("source_reconstruction_failed") from None
            # Printed absence is separate from a leader's life during play.
            if all(face.get(key) == "-" for key in ("cost", "power", "hp")):
                for key in ("cost", "attack", "defense"):
                    result[key] = {"state": "known", "value": None}
        return result

    def _document(self, source: SourceDescriptor) -> JsonValue:
        ref = source.source_ref
        return self.sources.document(
            SourceRef(
                batch_id=ref.batch_id,
                source_version_id=ref.source_version_id,
                parser=ref.parser,
                locator=ref.locator,
                text_hash="sha256:" + ref.text_hash,
            )
        )[1]

    def _absent_effect(self, source: SourceDescriptor, ordinal: int) -> bool:
        ref = source.source_ref
        if ref.parser != "translation-jp-v1":
            return False
        card = page(
            self.sources.batch(ref.batch_id),
            ref.source_version_id,
            region="jp",
            parser_version=ref.parser,
        ).text.get()
        verify_card(card)
        if not card.effect_presence:
            return False
        if card.source.id != ref.source_version_id or ordinal >= len(card.faces):
            raise SourceError("source_current_mismatch")
        proof = card.effect_presence[ordinal].result
        if proof.source_version_id != card.source.id or proof.source_index != ordinal:
            raise SourceError("source_current_mismatch")
        projected = card.projected(ordinal).effect
        return proof.state == "absent" and projected is not None and not projected

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

    def card_payload(self, card_id: str, *, inspect: bool = False) -> CurrentCard:
        """Reconstruct all current fields; inspection alone may retain pending parts."""
        if _one(self.db, "card", {"id": card_id})["identity_state"] != "confirmed":
            raise SourceError("source_identity_unconfirmed")
        faces: list[JsonValue] = []
        fields: list[CompiledField] = []
        revisions: dict[str, str] = {}
        face_positions: dict[str, tuple[int, str]] = {}
        for face in self.db.select(
            "face", self.db.columns("face"), where={"card_id": card_id}
        ):
            face_id = _text(face.values["id"])
            ordinal = face.values["ordinal"]
            if type(ordinal) is not int:
                raise SourceError("source_database_shape")
            face_positions[face_id] = ordinal + 1, _text(face.values["side"])
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
        if not inspect:
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
        return CurrentCard(payload, tuple(fields), revisions, face_positions)

    def _texts(
        self, locations: list[OwnerField]
    ) -> tuple[list[JsonValue], list[CompiledField]]:
        texts: list[JsonValue] = []
        fields: list[CompiledField] = []
        for location in locations:
            section_count: int | None = None
            absent = False
            try:
                selected = card_field(self.db, location)
                source = descriptor(self.db, self.sources, location, self.jp_batch_id)
                if location.field == "name":
                    section_count = len(
                        array(
                            pointer(
                                self._document(source),
                                f"/faces/{selected.face_ordinal}/sections",
                            )
                        )
                    )
                elif location.field == "effect" and not selected.text:
                    absent = self._absent_effect(source, selected.face_ordinal)
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
                    "state": "present"
                    if selected.text
                    else "absent"
                    if absent
                    else "unknown",
                    "source_hash": digest(selected.text.encode())[7:]
                    if selected.text or absent
                    else None,
                }
            )
        return texts, fields

    def _no_tokens(self, fields: list[CompiledField]) -> None:
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
                frame, binding = field.derived.bind(index)
                if (
                    frame.projection.projection_kind == "none"
                    and frame.semantic_variant.state == "resolved"
                    and frame.semantic_variant.key in NO_TOKEN_METADATA
                ):
                    continue
                if frame.semantic_variant.key == "card_keywords.v2":
                    card_keyword_values(
                        SourceContext(
                            frame,
                            binding,
                            field.field.parts[index].canonical_source,
                            self.classifier,
                        )
                    )
                    continue
                try:
                    entry_heads(frame)
                except SourceError:
                    raise SourceError("unsupported_source_projection") from None

    def checked_sources(self, card: CurrentCard) -> tuple[SourceContext, ...]:
        """Verify every exact registry match; this is not an admission proof."""
        self._no_tokens(list(card.fields))
        result: list[SourceContext] = []
        for field in card.fields:
            for index, matched in enumerate(field.matches):
                if matched is None:
                    raise SourceError("source_registry_mismatch")
                if matched.binding.source != field.field.source:
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
                result.append(
                    SourceContext(
                        matched.frame,
                        matched.binding,
                        field.field.parts[index].canonical_source,
                        self.classifier,
                    )
                )
        return tuple(result)

    @staticmethod
    def contexts(
        sources: tuple[SourceContext, ...],
    ) -> dict[tuple[str, str], SourceContext]:
        """Expose exact binding/root contexts for checked source obligations."""
        result: dict[tuple[str, str], SourceContext] = {}
        for source in sources:
            if source.frame.projection.projection_kind == "none":
                continue
            if source.frame.projection.projection_kind == "card_field":
                continue
            heads = entry_heads(source.frame)
            for root in range(len(heads)):
                result[source.binding.id, f"ability_{root}"] = source
        return result

    def _validate_card(
        self,
        record: AuthorIndexRecord,
        tree: JsonValue,
        input_hash: str,
        inputs: AuthorInputSet,
        expander: AuthorExpander,
    ) -> CardAccepted:
        if self.printing_card(record.record_key) != record.card_id:
            raise SourceError("record_key_identity_mismatch")
        card = self.card_payload(record.card_id)
        payload = source_payload(card.payload)
        version = source_version(card.payload)
        self._require_known(card)
        authored = object_value(
            object_value(object_value(tree)["cards"])[record.record_key]
        )
        if object_value(authored["meta"])["source"] != version:
            raise SourceError("source_version_mismatch")
        candidate, inventory = self._reconcile_card(
            card, tree, input_hash, inputs, expander
        )
        return CardAccepted(record, candidate, payload, version, inventory)

    def _reconcile_card(
        self,
        card: CurrentCard,
        tree: JsonValue,
        input_hash: str,
        inputs: AuthorInputSet,
        expander: AuthorExpander,
    ) -> tuple[ExpansionCandidate, bytes]:
        checked_sources = self.checked_sources(card)
        expected = self.contexts(checked_sources)
        keywords = self._keyword_obligations(card, checked_sources)
        inventory = self.source_inventory(card, expected, keywords)
        declarations = self._declaration_uses(card, tree, keywords)
        contexts = self._uses(card, tree, expected)
        result = expander.expand_contexts(
            canonical(tree),
            (file.raw for file in inputs.body_files),
            contexts,
            declarations=declarations,
        )
        return self._checked_candidate(
            result, contexts, declarations, input_hash
        ), inventory

    @staticmethod
    def _checked_candidate(
        result: ExpansionCandidate,
        contexts: Mapping[PathParts, SourceContext],
        declarations: Mapping[PathParts, PrintedKeywordObligation],
        input_hash: str,
    ) -> ExpansionCandidate:
        checked_uses = {}
        for path, ability in ability_nodes(result.tree):
            if "keywords" in ability:
                check_keyword_declaration(ability, declarations[path], path)
                continue
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
        declaration_evidence = []
        for declaration in result.declarations:
            obligation = declarations[declaration.path]
            checked_declaration = replace(
                declaration,
                input_hash=input_hash,
                source_input_hash="sha256:"
                + obligation.source.binding.source.source_hash,
                current_source=obligation.source_use,
            )
            object.__setattr__(  # ruff: ignore[unnecessary-dunder-call] -- only the complete current boundary grants proof
                checked_declaration, "source_proof", "current_source_checked"
            )
            declaration_evidence.append(checked_declaration)
        return ExpansionCandidate(
            result.tree, tuple(evidence), tuple(declaration_evidence)
        )

    @staticmethod
    def _require_known(card: CurrentCard) -> None:
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

    @staticmethod
    def source_inventory(
        card: CurrentCard,
        expected: Mapping[tuple[str, str], SourceContext],
        keywords: tuple[PrintedKeywordObligation, ...],
    ) -> bytes:
        """Expose the entire typed inventory independently of the author tree."""
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
                "kind": "metadata",
                "binding_id": matched.binding.id,
                "occurrence": matched.binding.occurrence_key().model_dump(mode="json"),
                "semantic_key": matched.frame.semantic_variant.key,
            }
            for field in card.fields
            for matched in field.matches
            if matched is not None
            and matched.frame.projection.projection_kind == "none"
        ]
        non_body.extend(
            {
                "kind": "printed_keywords",
                "source_use": item.source_use.model_dump(mode="json"),
                "face_id": item.face_id,
                "frame_id": item.source.frame.id,
                "interface_key": item.source.frame.projection.interface_key(
                    item.source.frame.id
                ),
                "semantic_key": "card_keywords.v2",
                "declaration": item.declaration_payload(),
                "source_adapter_recipe": item.adapter_recipe,
            }
            for item in keywords
        )
        inventory = SourceInventory.model_validate_json(
            canonical({"abilities": abilities, "non_body": non_body}), strict=True
        )
        return canonical(inventory.model_dump(mode="json"))

    def _uses(
        self,
        card: CurrentCard,
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
            if "keywords" in ability:
                continue
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

    def _declaration_uses(
        self,
        card: CurrentCard,
        tree: JsonValue,
        keywords: tuple[PrintedKeywordObligation, ...],
    ) -> dict[PathParts, PrintedKeywordObligation]:
        expected = {item.source_use.binding_id: item for item in keywords}
        result: dict[PathParts, PrintedKeywordObligation] = {}
        used: set[str] = set()
        for path, ability in ability_nodes(tree):
            if "keywords" not in ability:
                continue
            use = CardKeywordSourceUse.model_validate_json(
                canonical(array(ability["sources"])[0]), strict=True
            )
            self._use_location(card, path, ability, use)
            obligation = expected.get(use.binding_id)
            if obligation is None or obligation.source_use != use:
                raise ExpansionError("source_keyword_occurrence_mismatch", path, use)
            if use.binding_id in used:
                raise ExpansionError("duplicate_source_keyword", path, use)
            check_keyword_declaration(ability, obligation, path)
            result[path] = obligation
            used.add(use.binding_id)
        if used != expected.keys():
            raise SourceError("source_keyword_closure_mismatch")
        return result

    @staticmethod
    def _use_context(
        card: CurrentCard,
        expected: Mapping[tuple[str, str], SourceContext],
        path: PathParts,
        ability: dict[str, JsonValue],
        use: SourceUse,
    ) -> SourceContext:
        CurrentAuthorSources._use_location(card, path, ability, use)
        key = use.binding_id, use.ability_scope
        source = expected.get(key)
        if source is None or source.binding.occurrence_key() != use.occurrence:
            raise ExpansionError("source_occurrence_mismatch", path)
        return source

    @staticmethod
    def _use_location(
        card: CurrentCard,
        path: PathParts,
        ability: dict[str, JsonValue],
        use: SourceUse | CardKeywordSourceUse,
    ) -> None:
        owner = use.occurrence.owner
        if not isinstance(owner, FaceRevisionOwner):
            raise ExpansionError("unsupported_source_owner_" + owner.kind, path, use)
        face = card.revisions.get(owner.revision_id)
        if face is None:
            raise ExpansionError("source_current_owner_mismatch", path, use)
        if "faces" in path and path[3] != face:
            raise ExpansionError("source_face_container_mismatch", path, use)
        if use.occurrence.field not in {"effect", "section"}:
            raise ExpansionError("unsupported_source_field", path, use)
        if (
            ability.get("section") != use.occurrence.ordinal
            or ability["line"] != use.occurrence.line_ordinal + 1
        ):
            raise ExpansionError("source_location_mismatch", path, use)
