"""Materialize shorthand into the existing indexed author inventory."""

from copy import deepcopy
from dataclasses import replace
from io import StringIO
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError
from ruamel.yaml import YAML

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.authored import check_path
from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_handwritten import (
    ConfigurationError,
    Handwritten,
    Locator,
    decode,
    materialize,
    positions,
    read,
    resolve,
)
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    Diagnostic,
    parse_document,
)
from sve_carddb.domains.effects.author_set import (
    AuthorInputSet,
    InputFile,
    load_author_set,
)
from sve_carddb.domains.effects.author_sites import NodeSites, SiteError
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import SourceContext, SourceError

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CurrentCard
    from sve_carddb.domains.effects.author_validation import PathParts
    from sve_carddb.workflows.authoring import Workspace

_EMPTY_PORTS: dict[str, JsonValue] = {"imports": {}, "exports": {}}
_LOCATORS = frozenset({"line", "face", "section", "scope"})
_USE = frozenset({"use", "params", "edition", "registration"})
_CARD_KEYS = frozenset({"meta", "abilities", "note", "deck_rule"})


def format_yaml(tree: JsonValue) -> bytes:
    """Use one deterministic formatter for both sides of a round-trip comparison."""
    formatter = YAML(typ="safe")
    formatter.default_flow_style = False
    formatter.allow_unicode = True
    stream = StringIO()
    formatter.dump(tree, stream)
    return stream.getvalue().encode("utf-8")


def _file(path: str, tree: JsonValue) -> InputFile:
    raw = format_yaml(tree)
    if len(raw) >= MAX_BYTES:
        raise AuthorInputError(Diagnostic("yaml", "size_limit", (path,)))
    return InputFile(path, raw, digest(raw))


def _new_path(files: dict[str, InputFile], area: str, owner: str) -> str:
    for number in range(1_000_000):
        path = f"rules/{area}/{owner}/{number:03}.yaml"
        if path not in files:
            return path
    raise ValueError("Author shard numbers exhausted")


def _body(
    value: dict[str, JsonValue],
    handwritten: Handwritten,
    source: SourceContext,
    scope: str,
    sites: NodeSites,
) -> tuple[dict[str, JsonValue], int]:
    name = string(value["use"])
    editions = [
        edition for identifier, edition in handwritten.bodies if identifier == name
    ]
    edition = value.get("edition")
    if edition is None:
        if len(editions) != 1:
            raise SourceError("ambiguous_body_version" if editions else "body_missing")
        edition = editions[0]
    if type(edition) is not int or edition < 1:
        raise SourceError("invalid_body_edition")
    definition = handwritten.bodies.get((name, edition))
    if definition is None:
        raise SourceError("body_missing")
    if definition.ability_scope != scope:
        raise SourceError("body_scope_mismatch")
    try:
        return materialize(definition, source, sites), edition
    except SiteError as error:
        raise AuthorInputError(Diagnostic("source", error.reason, error.path)) from None
    except ValidationError as error:
        path = tuple(error.errors(include_input=False, include_url=False)[0]["loc"])
        raise AuthorInputError(
            Diagnostic("schema", "invalid_syntax_tree", path)
        ) from None


def _ability(
    value: dict[str, JsonValue],
    card: CurrentCard,
    handwritten: Handwritten,
    workspace: Workspace,
    sites: NodeSites,
) -> tuple[str, dict[str, JsonValue], dict[str, JsonValue] | None, int]:
    locator = decode(
        parse_document(
            canonical({key: item for key, item in value.items() if key in _LOCATORS})
        ),
        Locator,
    )
    if (
        isinstance(locator.face, str)
        and sum(side == locator.face for _, side in card.face_positions.values()) > 1
    ):
        raise SourceError("ambiguous_face_alias")
    located = resolve(
        positions(card, workspace.current.classifier),
        locator,
        keywords="keywords" in value,
    )
    occurrence = located.source.binding.occurrence_key()
    source: dict[str, JsonValue] = {
        "binding_id": located.source.binding.id,
        "occurrence": occurrence.model_dump(mode="json"),
    }
    result: dict[str, JsonValue] = {"line": locator.line, "sources": [source]}
    if occurrence.ordinal is not None:
        result["section"] = occurrence.ordinal
    body = None
    edition = 1
    if "use" in value:
        if value.keys() - (_LOCATORS | _USE):
            raise SourceError("ability_head_conflict")
        assert located.ability_scope is not None
        # Obtain the checked classifier, not the inspection-only derived context.
        contexts = workspace.current.contexts(workspace.current.checked_sources(card))
        context = contexts[located.source.binding.id, located.ability_scope]
        body, edition = _body(value, handwritten, context, located.ability_scope, sites)
        source["ability_scope"] = located.ability_scope
        params = object_value(value.get("params", {}))
        declarations = object_value(body["params"])
        if params.keys() != declarations.keys():
            raise SourceError("parameter_name_mismatch")
        result["generated_by"] = {
            "macro": {
                "frame_id": body["frame_id"],
                "ability_scope": body["ability_scope"],
                "applicability_scope": value.get("registration", "jp"),
            },
            "version": body["body_version"],
            "params": {
                name: {"type": object_value(declarations[name])["type"], "value": item}
                for name, item in params.items()
            },
        }
    else:
        if value.keys() & _USE:
            raise SourceError("ability_head_conflict")
        if located.ability_scope is not None:
            source["ability_scope"] = located.ability_scope
        result.update(
            {key: item for key, item in value.items() if key not in _LOCATORS}
        )
        result.setdefault("ports", deepcopy(_EMPTY_PORTS))
    return located.face_id, result, body, edition


def _card(  # ruff: ignore[complex-structure, too-many-branches, too-many-locals] -- keep the complete ordered boundary in one operation
    workspace: Workspace,
    number: str,
    handwritten: Handwritten,
    previous: dict[str, JsonValue] | None,
) -> tuple[
    str,
    dict[str, JsonValue],
    list[tuple[dict[str, JsonValue], int]],
    dict[PathParts, int],
]:
    value = handwritten.cards[number]
    if value.keys() - _CARD_KEYS or not {"meta", "abilities"} <= value.keys():
        raise SourceError("invalid_card_keys")
    meta = object_value(value["meta"])
    if meta.keys() - {
        "written_by",
        "reviewed_by",
        "status",
        "verified_by_exam",
        "rulings",
    } or not meta.get("written_by"):
        raise SourceError("invalid_card_metadata")
    card_id = workspace.current.printing_card(number)
    card = workspace.current.card_payload(card_id, inspect=True)
    generated_meta: dict[str, JsonValue] = {
        "dsl": "1.0",
        "source": source_version(card.payload),
        "written_by": meta["written_by"],
        "reviewed_by": meta.get("reviewed_by", []),
        "status": meta.get("status", "draft"),
        "verified_by_exam": meta.get("verified_by_exam", []),
        "rulings": meta.get("rulings", []),
    }
    if previous is not None:
        if "abilities" in previous and "faces" in previous:
            raise SourceError("unsupported_legacy_container_shape")
        original = object_value(previous["meta"])
        for key in ("written_by", "reviewed_by", "verified_by_exam", "rulings"):
            if original.get(key) != generated_meta[key]:
                raise SourceError("author_metadata_conflict")
        if "note" in previous and previous.get("note") != value.get("note"):
            raise SourceError("author_note_conflict")
    result: dict[str, JsonValue] = {"meta": generated_meta}
    for key in ("note", "deck_rule"):
        if key in value:
            result[key] = value[key]
    use_faces = previous is None or "faces" in previous
    result["faces" if use_faces else "abilities"] = {} if use_faces else []
    bodies: list[tuple[dict[str, JsonValue], int]] = []
    locations: dict[PathParts, int] = {}
    sites = NodeSites(workspace.inputs.repo / "dsl")
    for index, ability in enumerate(array(value["abilities"])):
        try:
            face, expanded, body, edition = _ability(
                object_value(ability), card, handwritten, workspace, sites
            )
            if use_faces:
                container = object_value(result["faces"])
                face_card = object_value(container.setdefault(face, {"abilities": []}))
                locations[
                    "cards",
                    number,
                    "faces",
                    face,
                    "abilities",
                    len(array(face_card["abilities"])),
                ] = index
                array(face_card["abilities"]).append(expanded)
            else:
                locations["cards", number, "abilities", index] = index
                array(result["abilities"]).append(expanded)
            if body is not None:
                bodies.append((body, edition))
        except (AuthorInputError, SourceError) as error:
            diagnostic = (
                error.diagnostic
                if isinstance(error, AuthorInputError)
                else Diagnostic("source", error.reason)
            )
            path = ("cards", number, "abilities", index, *diagnostic.path)
            cursor = handwritten.documents[number].diagnostic(
                diagnostic.stage,
                diagnostic.reason,
                path,
                diagnostic.expected,
                diagnostic.actual,
            )
            line = object_value(ability).get("line")
            raise AuthorInputError(
                replace(
                    cursor,
                    card_no=number,
                    card_id=card_id,
                    card_line=line if type(line) is int else None,
                )
            ) from None
    return card_id, result, bodies, locations


def _merge_body(
    workspace: Workspace,
    files: dict[str, InputFile],
    body: dict[str, JsonValue],
    edition: int,
    owner: str,
) -> str | None:
    same_frame = False
    for file in files.values():
        tree = object_value(workspace.expander.schemas.body_tree(file.raw))
        for item in array(tree["records"]):
            record = object_value(item)
            if (record["frame_id"], record["ability_scope"]) != (
                body["frame_id"],
                body["ability_scope"],
            ):
                continue
            same_frame = True
            if record["body_version"] == body["body_version"]:
                if record != body:
                    raise SourceError("body_metadata_conflict")
                return None
    if same_frame and edition == 1:
        raise SourceError("new_body_requires_explicit_edition")
    path = _new_path(files, "effect-bodies", owner)
    files[path] = _file(
        path,
        {
            "format": 1,
            "kind": "effect_body_shard",
            "scope": "shared",
            "records": [body],
        },
    )
    return path


class WriteError(OSError):
    """Retain the exact completed writes when a filesystem operation fails."""

    def __init__(self, written: tuple[str, ...], failed: str) -> None:
        self.written = written
        self.failed = failed
        super().__init__(
            "Author write interrupted; completed files: "
            + ", ".join(written)
            + "; failed file (may be partial): "
            + failed
        )


def write(  # ruff: ignore[complex-structure, too-many-arguments, too-many-branches, too-many-statements, too-many-locals] -- keep the complete ordered boundary in one operation
    workspace: Workspace,
    *,
    main: Path,
    bodies: Path | None,
    output_root: Path,
    cards: frozenset[str] = frozenset(),
    all_cards: bool = False,
    remove_prototype: bool = False,
    base: str | None = None,
    baseline_root: Path | None = None,
) -> dict[str, JsonValue]:
    """Validate a complete candidate in memory, then directly write selected files."""
    if bool(cards) == all_cards:
        raise ConfigurationError("Specify cards or --all")
    for configured_path in (main, bodies, output_root):
        if configured_path is not None and not configured_path.is_absolute():
            raise ConfigurationError("Author write requires absolute paths")
    if remove_prototype and (base is None or baseline_root is None):
        raise ConfigurationError("Prototype removal requires base and baseline_root")
    handwritten = read(main, bodies)
    selected = frozenset(handwritten.cards) if all_cards else cards
    if selected - handwritten.cards.keys():
        raise AuthorInputError(Diagnostic("schema", "handwritten_card_missing"))
    original = load_author_set(output_root, workspace.expander.schemas)
    if output_root != workspace.authored:
        reference = load_author_set(workspace.authored, workspace.expander.schemas)
        expected_files = {
            file.path
            for file in (
                *reference.author_files,
                *reference.body_files,
                *reference.prototype_files,
            )
        }
        actual_files = {
            file.path
            for file in (
                *original.author_files,
                *original.body_files,
                *original.prototype_files,
            )
        }
        expected_cards = {
            (record.card_id, record.scope) for record in reference.index.records
        }
        actual_cards = {
            (record.card_id, record.scope) for record in original.index.records
        }
        if expected_files - actual_files or expected_cards - actual_cards:
            raise AuthorInputError(Diagnostic("index", "output_inventory_incomplete"))
    authors = {file.path: file for file in original.author_files}
    companions = {file.path: file for file in original.body_files}
    prototypes = {file.path: file for file in original.prototype_files}
    records = list(original.index.records)
    changed: set[str] = set()
    body_bindings: dict[tuple[str, int], tuple[JsonValue, JsonValue, JsonValue]] = {}
    locations: dict[PathParts, int] = {}
    for number in sorted(selected):
        try:
            card_id = workspace.current.printing_card(number)
        except SourceError as error:
            raise AuthorInputError(
                Diagnostic("source", error.reason, card_no=number)
            ) from None
        alias = next(
            (
                record
                for record in records
                if record.card_id == card_id
                and record.scope == "shared"
                and record.record_key != number
            ),
            None,
        )
        if alias is not None:
            raise AuthorInputError(
                Diagnostic(
                    "index",
                    "card_alias_already_authored",
                    card_no=number,
                    card_id=card_id,
                    expected=alias.record_key,
                )
            )
        old = next((record for record in records if record.record_key == number), None)
        previous = None
        if old is not None and isinstance(old, AuthorIndexRecord):
            if old.file.split("/")[-2] != number.split("-", 1)[0]:
                raise AuthorInputError(
                    Diagnostic("index", "wrong_owner", card_no=number)
                )
            tree = object_value(
                workspace.expander.schemas.author_tree(authors[old.file].raw)
            )
            previous = object_value(object_value(tree["cards"])[number])
            path = old.file
        else:
            path = _new_path(authors, "author-effects", number.split("-", 1)[0])
            tree = {
                "format": 1,
                "kind": "author_effect_set",
                "scope": "shared",
                "cards": {},
            }
        try:  # ruff: ignore[too-many-statements-in-try-clause] -- attach the selected card identity to materialization failures
            card_id, value, materialized, card_locations = _card(
                workspace, number, handwritten, previous
            )
            locations.update(card_locations)
            if (
                old is not None
                and not isinstance(old, AuthorIndexRecord)
                and not remove_prototype
            ):
                raise SourceError("prototype_author_overlap")  # ruff: ignore[raise-within-try] -- add the selected card to this source diagnostic
            for body, edition in materialized:
                added = _merge_body(
                    workspace, companions, body, edition, number.split("-", 1)[0]
                )
                if added is not None:
                    changed.add(added)
            for ability, (body, _) in zip(
                (
                    object_value(a)
                    for a in array(handwritten.cards[number]["abilities"])
                    if "use" in object_value(a)
                ),
                materialized,
                strict=True,
            ):
                name = string(ability["use"])
                editions = [
                    edition
                    for identifier, edition in handwritten.bodies
                    if identifier == name
                ]
                resolved_edition = ability.get(
                    "edition", editions[0] if len(editions) == 1 else None
                )
                assert type(resolved_edition) is int
                key = name, resolved_edition
                identity = body["frame_id"], body["ability_scope"], body["body_version"]
                if key in body_bindings and body_bindings[key] != identity:
                    raise SourceError("body_source_frame_mismatch")  # ruff: ignore[raise-within-try] -- add the selected card to this source diagnostic
                body_bindings[key] = identity
        except SourceError as error:
            raise AuthorInputError(
                Diagnostic("source", error.reason, card_no=number)
            ) from None
        if remove_prototype:
            for prototype_path, file in tuple(prototypes.items()):
                prototype = object_value(
                    workspace.expander.schemas.prototype_tree(file.raw)
                )
                mapping = object_value(prototype["cards"])
                if number in mapping:
                    del mapping[number]
                    prototypes[prototype_path] = _file(prototype_path, prototype)
                    changed.add(prototype_path)
        object_value(tree["cards"])[number] = value
        authors[path] = _file(path, tree)
        changed.add(path)
        replacement = AuthorIndexRecord(
            card_id=card_id,
            scope="shared",
            file=path,
            kind="author_effect_set",
            record_key=number,
        )
        if old is None:
            records.append(replacement)
        else:
            records[records.index(old)] = replacement
    if all_cards and handwritten.bodies.keys() - body_bindings.keys():
        raise AuthorInputError(Diagnostic("source", "unused_body_definition"))
    index = original.index.model_copy(update={"records": tuple(records)})
    index_file = _file("rules/effects-index.yaml", index.model_dump(mode="json"))
    candidate = AuthorInputSet(
        index,
        index_file,
        tuple(authors.values()),
        tuple(companions.values()),
        tuple(prototypes.values()),
    )
    results = workspace.current.validate_inputs(candidate, workspace.expander)
    rejected = [
        result
        for result in results
        if isinstance(result, CardRejected)
        and isinstance(result.record, AuthorIndexRecord)
    ]
    if rejected:
        diagnostic = rejected[0].diagnostic
        rejected_number = diagnostic.card_no
        if rejected_number is not None and rejected_number in handwritten.documents:
            input_path: PathParts = ("cards", rejected_number)
            line = (
                diagnostic.source.occurrence.line_ordinal + 1
                if diagnostic.source is not None
                else None
            )
            for legacy_path, index_in_handwritten in locations.items():
                if diagnostic.path[: len(legacy_path)] == legacy_path:
                    input_path = (
                        "cards",
                        rejected_number,
                        "abilities",
                        index_in_handwritten,
                        *diagnostic.path[len(legacy_path) :],
                    )
                    selected_ability = object_value(
                        array(handwritten.cards[rejected_number]["abilities"])[
                            index_in_handwritten
                        ]
                    )
                    selected_line = selected_ability["line"]
                    assert type(selected_line) is int
                    line = selected_line
                    break
            cursor = handwritten.documents[rejected_number].diagnostic(
                diagnostic.stage, diagnostic.reason, input_path
            )
            diagnostic = replace(
                diagnostic,
                path=input_path,
                line=cursor.line,
                column=cursor.column,
                card_line=line,
            )
        raise AuthorInputError(diagnostic)
    if remove_prototype:
        from sve_carddb.workflows.authoring_regression import check_regression  # ruff: ignore[import-outside-top-level] -- regression is only needed for explicitly requested prototype removal

        assert base is not None
        assert baseline_root is not None
        check_regression(
            workspace,
            base,
            baseline_root,
            candidate,
            results,
            expected_removed=selected,
        )
    changed.add(index_file.path)
    outputs = {**authors, **companions, **prototypes, index_file.path: index_file}
    written: list[str] = []
    for path in sorted(changed):
        check_path(Path(output_root.anchor), output_root / path)
    try:
        for path in sorted(changed):
            destination = output_root / path
            if destination.exists() and destination.read_bytes() == outputs[path].raw:
                continue
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(outputs[path].raw)
            written.append(path)
    except OSError:
        raise WriteError(tuple(written), path) from None
    return {
        "status": "ok",
        "cards": list[JsonValue](sorted(selected)),
        "written": list(written),
        "storage": "legacy",
    }
