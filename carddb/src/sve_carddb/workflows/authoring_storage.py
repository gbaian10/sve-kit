"""Write handwritten author shards and explicitly refresh generated anchors."""

from dataclasses import replace
from io import StringIO
from pathlib import Path
from typing import TYPE_CHECKING, Literal, override

from pydantic import JsonValue
from ruamel.yaml import YAML
from ruamel.yaml.emitter import RoundTripEmitter
from ruamel.yaml.nodes import ScalarNode
from ruamel.yaml.representer import RoundTripRepresenter

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.authored import check_path
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.yaml import MAX_BYTES
from sve_carddb.domains.effects.author_anchors import handwriting, materialize_inputs
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_handwritten import (
    Body,
    BodyFile,
    ConfigurationError,
    Handwritten,
    decode,
    read,
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

if TYPE_CHECKING:
    from sve_carddb.workflows.authoring import Workspace


class _YamlEmitter(RoundTripEmitter):
    @override
    def choose_scalar_style(self) -> str:
        """Match the repository formatter's quote policy."""
        style = super().choose_scalar_style()
        if not isinstance(style, str):
            raise TypeError("Unsupported YAML scalar style")
        return '"' if style == "'" else style


class _YamlRepresenter(RoundTripRepresenter):
    pass


def _represent_null(_representer: RoundTripRepresenter, _value: None) -> ScalarNode:
    return ScalarNode("tag:yaml.org,2002:null", "null")


_YamlRepresenter.add_representer(type(None), _represent_null)


def format_yaml(tree: JsonValue) -> bytes:
    """Use one deterministic formatter for comparison and persisted inputs."""
    formatter = YAML(typ="rt")
    formatter.Emitter = _YamlEmitter
    formatter.Representer = _YamlRepresenter
    formatter.indent(mapping=2, sequence=4, offset=2)
    formatter.default_flow_style = False
    formatter.allow_unicode = True
    # Keep fingerprints on one line; wrapping can leave trailing spaces.
    formatter.width = MAX_BYTES
    stream = StringIO()
    formatter.dump(parse(canonical(tree)), stream)
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


class WriteError(OSError):
    """Retain exact completed writes when a filesystem operation fails."""

    def __init__(self, written: tuple[str, ...], failed: str) -> None:
        self.written = written
        self.failed = failed
        super().__init__(
            "Author write interrupted; completed files: "
            + ", ".join(written)
            + "; failed file (may be partial): "
            + failed
        )


def _facts(
    before: dict[str, JsonValue], after: dict[str, JsonValue], number: str
) -> None:
    defaults: tuple[tuple[str, JsonValue], ...] = (
        ("written_by", None),
        ("reviewed_by", []),
        ("status", "draft"),
        ("verified_by_exam", []),
        ("rulings", []),
    )
    for key, default in defaults:
        if before.get(key, default) != after.get(key, default):
            raise AuthorInputError(
                Diagnostic("source", "author_metadata_conflict", card_no=number)
            )


_BODY_RECORD_PATH_LENGTH = 3


def _located_error(
    inputs: AuthorInputSet, supplied: Handwritten, diagnostic: Diagnostic
) -> AuthorInputError:
    hand = handwriting(inputs)
    number = diagnostic.card_no
    ability_index = 0
    path = diagnostic.path
    if (
        number is None
        and len(path) >= _BODY_RECORD_PATH_LENGTH
        and isinstance(path[0], str)
        and path[1] == "records"
        and type(path[2]) is int
    ):
        file = next((f for f in inputs.body_files if f.path == path[0]), None)
        if file is not None:
            definition = decode_bodies(file)[path[2]]
            selected = [
                (n, i)
                for n, card in hand.cards.items()
                for i, ability in enumerate(array(card["abilities"]))
                if object_value(ability).get("use") == definition.id
            ]
            if selected:
                number, ability_index = selected[0]
    if number is None or number not in hand.cards:
        return AuthorInputError(diagnostic)
    values = array(hand.cards[number]["abilities"])
    if "abilities" in path:
        offset = path.index("abilities") + 1
        if offset < len(path) and type(path[offset]) is int:
            selected_index = path[offset]
            assert type(selected_index) is int
            ability_index = selected_index
    ability_index = _face_ability_index(inputs, number, path, ability_index)
    if ability_index >= len(values):
        return AuthorInputError(diagnostic)
    ability = object_value(values[ability_index])
    line = ability.get("line")
    document = supplied.documents.get(number, hand.documents[number])
    cursor_path = ("cards", number, "abilities", ability_index)
    cursor = document.diagnostic(diagnostic.stage, diagnostic.reason, cursor_path)
    return AuthorInputError(
        replace(
            diagnostic,
            card_no=number,
            card_line=diagnostic.card_line or (line if type(line) is int else None),
            line=cursor.line,
            column=cursor.column,
        )
    )


def _face_ability_index(
    inputs: AuthorInputSet, number: str, path: tuple[str | int, ...], ability_index: int
) -> int:
    if "faces" in path:
        face = path[path.index("faces") + 1]
        for file in inputs.anchor_files:
            cards = object_value(object_value(parse_document(file.raw).value)["cards"])
            if number in cards:
                uses = array(object_value(cards[number])["uses"])
                matching = [
                    index
                    for index, use in enumerate(uses)
                    if object_value(use)["face_id"] == face
                ]
                if ability_index < len(matching):
                    return matching[ability_index]
                break
    return ability_index


def write(  # ruff: ignore[complex-structure, too-many-arguments, too-many-branches, too-many-statements, too-many-locals] -- validate the complete candidate before performing any write
    workspace: Workspace,
    *,
    main: Path,
    bodies: Path | None,
    output_root: Path,
    cards: frozenset[str] = frozenset(),
    all_cards: bool = False,
    storage: Literal["anchors"] = "anchors",
    remove_prototype: bool = False,
    base: str | None = None,
    baseline_root: Path | None = None,
) -> dict[str, JsonValue]:
    """Refresh only explicitly selected cards, retaining every recorded review literal."""
    if bool(cards) == all_cards or storage != "anchors":
        raise ConfigurationError("Specify cards or --all and storage anchors")
    for configured in (main, bodies, output_root):
        if configured is not None and (
            not configured.is_absolute() or ".." in configured.parts
        ):
            raise ConfigurationError(
                "Author write requires absolute paths without traversal"
            )
    if remove_prototype and (base is None or baseline_root is None):
        raise ConfigurationError("Prototype removal requires base and baseline_root")
    supplied = read(main, bodies)
    selected = frozenset(supplied.cards) if all_cards else cards
    if selected - supplied.cards.keys():
        raise AuthorInputError(Diagnostic("schema", "handwritten_card_missing"))
    schemas = workspace.expander.schemas
    original = load_author_set(output_root, schemas)
    if output_root != workspace.authored:
        reference = load_author_set(workspace.authored, schemas)
        expected = {
            f.path
            for f in (
                *reference.author_files,
                *reference.body_files,
                *reference.prototype_files,
                *reference.anchor_files,
            )
        }
        actual = {
            f.path
            for f in (
                *original.author_files,
                *original.body_files,
                *original.prototype_files,
                *original.anchor_files,
            )
        }
        if expected - actual or {
            (r.card_id, r.scope) for r in reference.index.records
        } - {(r.card_id, r.scope) for r in original.index.records}:
            raise AuthorInputError(Diagnostic("index", "output_inventory_incomplete"))
    previous = handwriting(original)
    authors = {f.path: f for f in original.author_files}
    companions = {f.path: f for f in original.body_files}
    prototypes = {f.path: f for f in original.prototype_files}
    records = list(original.index.records)
    changed = set()
    removed_prototypes: set[str] = set()
    prototype_ids: dict[str, str] = {}
    removed_cards: set[str] = set()
    for number in sorted(selected):
        card_id = workspace.current.printing_card(number)
        old = next(
            (r for r in records if r.card_id == card_id and r.scope == "shared"), None
        )
        if old is not None and old.record_key != number:
            raise AuthorInputError(
                Diagnostic(
                    "index",
                    "card_alias_already_authored",
                    card_no=number,
                    expected=old.record_key,
                )
            )
        if old is not None and old.kind != "author_effect_set" and not remove_prototype:
            raise AuthorInputError(
                Diagnostic("source", "prototype_author_overlap", card_no=number)
            )
        if number in previous.cards:
            _facts(
                object_value(previous.cards[number]["meta"]),
                object_value(supplied.cards[number]["meta"]),
                number,
            )
            if "note" in previous.cards[number] and previous.cards[number].get(
                "note"
            ) != supplied.cards[number].get("note"):
                raise AuthorInputError(
                    Diagnostic("source", "author_note_conflict", card_no=number)
                )
        path = (
            old.file
            if isinstance(old, AuthorIndexRecord)
            else _new_path(authors, "author-effects", number.split("-", 1)[0])
        )
        tree: dict[str, JsonValue] = (
            object_value(parse_document(authors[path].raw).value)
            if path in authors
            else {"format": 2, "kind": "author_effect_set", "cards": {}}
        )
        object_value(tree["cards"])[number] = supplied.cards[number]
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
        if remove_prototype:
            for path, file in tuple(prototypes.items()):
                tree = object_value(schemas.prototype_tree(file.raw))
                mapping = object_value(tree["cards"])
                for alias in mapping:
                    if alias not in prototype_ids:
                        prototype_ids[alias] = workspace.current.printing_card(
                            alias, prototype=True
                        )
                aliases = {
                    alias for alias in mapping if prototype_ids[alias] == card_id
                }
                if aliases:
                    for alias in aliases:
                        del mapping[alias]
                    removed_prototypes.update(aliases)
                    removed_cards.add(card_id)
                    if mapping:
                        prototypes[path] = _file(path, tree)
                    else:
                        del prototypes[path]
                    changed.add(path)
            if card_id not in removed_cards:
                # A requested removal must find a prototype for its own card.
                raise AuthorInputError(
                    Diagnostic(
                        "source", "prototype_removal_set_mismatch", card_no=number
                    )
                )
    for key, definition in supplied.bodies.items():
        used = [
            number
            for number in selected
            if any(
                object_value(a).get("use") == key[0]
                and object_value(a).get("edition", definition.edition)
                == definition.edition
                for a in array(supplied.cards[number]["abilities"])
            )
        ]
        if not used and key not in previous.bodies:
            if all_cards:
                raise AuthorInputError(Diagnostic("source", "unused_body_definition"))
            continue
        if not used and not all_cards:
            continue
        body_path = next(
            (
                f.path
                for f in companions.values()
                if any((b.id, b.edition) == key for b in decode_bodies(f))
            ),
            None,
        )
        raw_definition = definition.model_dump(mode="json", exclude_unset=True)
        if key in previous.bodies:
            old_body = previous.bodies[key]
            if (
                old_body.written_by != definition.written_by
                or old_body.registrations != definition.registrations
                or old_body.note != definition.note
            ):
                raise AuthorInputError(Diagnostic("source", "body_metadata_conflict"))
        if body_path is None:
            path = _new_path(companions, "effect-bodies", used[0].split("-", 1)[0])
            tree = {
                "format": 2,
                "kind": "effect_body_shard",
                "records": [raw_definition],
            }
        else:
            path = body_path
            tree = object_value(parse_document(companions[path].raw).value)
            definitions = array(tree["records"])
            for body_index, body in enumerate(decode_bodies(companions[path])):
                if (body.id, body.edition) == key:
                    definitions[body_index] = raw_definition
        companions[path] = _file(path, tree)
        changed.add(path)
    index = original.index.model_copy(update={"records": tuple(records)})
    index_file = _file("rules/effects-index.yaml", index.model_dump(mode="json"))
    candidate = AuthorInputSet(
        index,
        index_file,
        tuple(authors.values()),
        tuple(companions.values()),
        tuple(prototypes.values()),
        original.anchor_files,
    )
    from sve_carddb.domains.effects.author_set import validate_author_set  # ruff: ignore[import-outside-top-level] -- candidate inventory validation follows complete construction

    try:
        validate_author_set(candidate, schemas)
    except AuthorInputError as error:
        raise _located_error(candidate, supplied, error.diagnostic) from None
    materialized = materialize_inputs(
        candidate,
        workspace.current,
        workspace.expander,
        refresh=selected,
        refresh_unused=all_cards,
    )
    old_pins = {
        key: object_value(pin)
        for file in original.anchor_files
        for key, pin in object_value(
            object_value(parse_document(file.raw).value)["bodies"]
        ).items()
    }
    for anchor in materialized.anchors.values():
        for pin_key, value in object_value(anchor["bodies"]).items():
            if pin_key in old_pins and any(
                old_pins[pin_key][field] != object_value(value)[field]
                for field in ("frame_id", "ability_scope", "body_version")
            ):
                raise AuthorInputError(
                    Diagnostic("body", "new_body_requires_explicit_edition")
                )
    if materialized.failures:
        raise _located_error(
            candidate, supplied, next(iter(materialized.failures.values()))
        )
    anchors = tuple(
        _file(f"rules/effect-anchors/{owner}.yaml", tree)
        for owner, tree in sorted(materialized.anchors.items())
    )
    candidate = AuthorInputSet(
        index,
        index_file,
        candidate.author_files,
        candidate.body_files,
        candidate.prototype_files,
        anchors,
    )
    results = workspace.current.validate_inputs(candidate, workspace.expander)
    rejected = [
        r
        for r in results
        if isinstance(r, CardRejected) and isinstance(r.record, AuthorIndexRecord)
    ]
    if rejected:
        raise _located_error(candidate, supplied, rejected[0].diagnostic)
    if remove_prototype:
        from sve_carddb.workflows.authoring_regression import check_regression  # ruff: ignore[import-outside-top-level] -- regression is optional for explicit prototype removal

        assert base is not None
        assert baseline_root is not None
        check_regression(
            workspace,
            base,
            baseline_root,
            materialized.inputs,
            results,
            expected_removed=frozenset(removed_prototypes),
        )
    outputs = {
        **authors,
        **companions,
        **prototypes,
        index_file.path: index_file,
        **{f.path: f for f in anchors},
    }
    changed.update(f.path for f in anchors)
    changed.add(index_file.path)
    for path in sorted(changed):
        check_path(Path(output_root.anchor), output_root / path)
    written: list[str] = []
    try:
        for path in sorted(changed):
            destination = output_root / path
            if path not in outputs:
                destination.unlink()
                written.append(path)
                continue
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
        "storage": storage,
    }


def decode_bodies(file: InputFile) -> list[Body]:
    """Decode the name table for one already validated handwritten shard."""
    return decode(parse_document(file.raw), BodyFile).records
