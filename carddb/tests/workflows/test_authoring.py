"""Repository integration through the same public authoring entry as the CLI."""

import shutil
from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, digest, object_value
from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.workflows import authoring

if TYPE_CHECKING:
    from pydantic import JsonValue

REPO = Path(__file__).resolve().parents[3]


def tree(path: Path) -> dict[str, JsonValue]:
    return object_value(JSON_VALUE.validate_python(parse_yaml(path.read_bytes())))


@pytest.fixture(scope="session")
def handwritten() -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    cards: dict[str, JsonValue] = {}
    bodies: list[JsonValue] = []
    for owner, number, name in (
        ("BP10", "BP10-122", "draw_exact_attack_time.v1"),
        ("BP06", "BP06-048", "search_lastword_self_deck_named_hand.v1"),
        ("BP01", "BP01-110", ""),
    ):
        filename = "001.yaml" if owner == "BP01" else "000.yaml"
        previous = object_value(
            object_value(
                tree(REPO / f"authored/rules/author-effects/{owner}/{filename}")[
                    "cards"
                ]
            )[number]
        )
        meta = object_value(previous["meta"])
        ability: dict[str, JsonValue] = {"line": 1}
        if name:
            definition = object_value(
                array(
                    tree(REPO / f"authored/rules/effect-bodies/{owner}/000.yaml")[
                        "records"
                    ]
                )[0]
            )
            bodies.append(
                {
                    "id": name,
                    "params": {"n": "Nat"},
                    "body": definition["body"],
                    "written_by": definition["written_by"],
                }
            )
            ability.update({"use": name, "params": {"n": 1}})
        else:
            ability["evolve"] = {"cost": {"pp": 1}}
        cards[number] = {
            "meta": {"written_by": meta["written_by"]},
            "abilities": [ability],
        }
        if "admission" in previous:
            object_value(cards[number])["admission"] = previous["admission"]
    main_tree: dict[str, JsonValue] = {
        "format": 2,
        "kind": "author_effect_set",
        "cards": cards,
    }
    body_tree: dict[str, JsonValue] = {
        "format": 2,
        "kind": "effect_body_shard",
        "records": bodies,
    }
    return main_tree, body_tree


def handwriting(
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> tuple[Path, Path, dict[str, JsonValue], dict[str, JsonValue]]:
    main_tree, body_tree = deepcopy(handwritten)
    main = tmp_path / "main.yaml"
    companion = tmp_path / "bodies.yaml"
    main.write_bytes(authoring.format_yaml(main_tree))
    companion.write_bytes(authoring.format_yaml(body_tree))
    return main, companion, main_tree, body_tree


def fingerprint(root: Path) -> dict[str, str]:
    return {
        path.relative_to(root).as_posix(): digest(path.read_bytes())
        for path in root.rglob("*")
        if path.is_file()
    }


def rename_draw_parameter(
    main_tree: dict[str, JsonValue], body_tree: dict[str, JsonValue]
) -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    ability = object_value(
        array(object_value(object_value(main_tree["cards"])["BP10-122"])["abilities"])[
            0
        ]
    )
    definition = object_value(array(body_tree["records"])[0])
    definition["params"] = {"N": "Nat"}
    draw = object_value(
        object_value(
            array(object_value(object_value(definition["body"])["attack_time"])["do"])[
                0
            ]
        )["draw"]
    )
    draw["n"] = {"param": "N"}
    ability["params"] = {"N": 1}
    return ability, definition


@pytest.mark.parametrize(
    ("number", "reprint"),
    [("BP01-046", "SD02-008"), ("BP01-176", "ETD01-019")],
)
def test_public_inspect_resolves_multi_printing_identity(
    workspace: authoring.Workspace, number: str, reprint: str
) -> None:
    report = authoring.inspect(workspace, number)
    assert report["fields"]
    other = authoring.inspect(workspace, reprint)
    assert other["card_id"] == report["card_id"]
    assert other["fields"] == report["fields"]
    assert other["payload"] == report["payload"]


def test_write_preserves_handwritten_storage_and_reuses_companions(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, hand, _ = handwriting(tmp_path, handwritten)
    report = authoring.write(
        workspace, main=main, bodies=bodies, output_root=output_root, all_cards=True
    )
    assert report["cards"] == ["BP01-110", "BP06-048", "BP10-122"]
    document = tree(output_root / "rules/author-effects/BP10/000.yaml")
    assert document["format"] == 2
    card = object_value(object_value(document["cards"])["BP10-122"])
    assert card == object_value(object_value(hand["cards"])["BP10-122"])
    assert "source" not in object_value(card["meta"])
    assert not tuple((output_root / "rules/effect-anchors").glob("*.yaml"))
    second = authoring.write(
        workspace, main=main, bodies=bodies, output_root=output_root, all_cards=True
    )
    assert second["written"] == []
    # Every sibling the writer did not own stays byte-identical; the rewritten
    # card file was already checked field by field above.
    rewritten = "rules/author-effects/BP10/000.yaml"
    written_tree = fingerprint(output_root)
    repository_tree = fingerprint(workspace.authored)
    del written_tree[rewritten], repository_tree[rewritten]
    assert written_tree == repository_tree


@pytest.mark.parametrize(
    ("mutation", "reason"),
    [
        ("face", "missing_face"),
        ("param", "param_type_mismatch"),
        ("body_author", "body_metadata_conflict"),
        ("body_review", "body_metadata_conflict"),
        ("body_syntax", "additionalProperties"),
        ("status", "author_metadata_conflict"),
        ("closure", "source_line_closure_mismatch"),
        ("duplicate_body", "duplicate_body_id"),
        ("editions", "ambiguous_body_version"),
    ],
)
def test_write_rejects_without_any_writes(  # ruff: ignore[complex-structure, too-many-branches] -- exercise independent rejected shorthand mutations against the same public writer
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
    mutation: str,
    reason: str,
) -> None:
    main, bodies, main_tree, body_tree = handwriting(tmp_path, handwritten)
    card = object_value(object_value(main_tree["cards"])["BP10-122"])
    ability = object_value(array(card["abilities"])[0])
    definition = object_value(array(body_tree["records"])[0])
    match mutation:
        case "line" | "face" | "section" | "scope":
            ability[mutation] = 99
        case "param":
            ability["params"] = {"n": "invalid"}
        case "missing_body":
            ability["use"] = "absent.v1"
        case "body_author":
            definition["written_by"] = [
                {"actor": {"who": "Another"}, "date": "2026-10-26"}
            ]
        case "body_registration":
            definition["registrations"] = [
                {"id": "jp", "domain": {"regions": ["en"]}, "reviews": []}
            ]
        case "body_review":
            definition["registrations"] = [
                {
                    "id": "jp",
                    "domain": {"regions": ["jp"]},
                    "reviews": [{"invalid": True}],
                }
            ]
        case "body_syntax":
            definition["body"] = {"unknown": {}}
        case "author":
            object_value(card["meta"])["written_by"] = [
                {"actor": {"who": "Another"}, "date": "2026-10-26"}
            ]
        case "status":
            object_value(card["meta"])["status"] = "disputed"
        case "closure":
            card["abilities"] = []
        case "unknown":
            ability["surprise"] = True
        case "duplicate_body":
            array(body_tree["records"]).append(deepcopy(definition))
        case "editions":
            array(body_tree["records"]).append({**definition, "edition": 2})
            card["abilities"] = [{**ability, "edition": 1}, ability]
    main.write_bytes(authoring.format_yaml(main_tree))
    bodies.write_bytes(authoring.format_yaml(body_tree))
    before = fingerprint(output_root)
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=bodies,
            output_root=output_root,
            cards=frozenset({"BP10-122"}),
        )
    assert error.value.diagnostic.reason == reason
    if mutation in {"param", "body_syntax", "editions"}:
        assert error.value.diagnostic.card_line == 1
        assert error.value.diagnostic.line is not None
    if mutation == "editions":
        assert error.value.diagnostic.path == ("cards", "BP10-122", "abilities", 1)
    assert fingerprint(output_root) == before


@pytest.mark.parametrize(
    ("edition", "expected_admission"), [(1, "light"), (2, "shared")]
)
def test_current_handoff_uses_selected_body_edition(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
    edition: int,
    expected_admission: str,
) -> None:
    main, companion, main_tree, body_tree = handwriting(tmp_path, handwritten)
    card = object_value(object_value(main_tree["cards"])["BP10-122"])
    card["admission"] = "light"
    object_value(array(card["abilities"])[0])["edition"] = edition
    main.write_bytes(authoring.format_yaml(main_tree))
    records = array(body_tree["records"])
    records.append({**object_value(records[0]), "edition": 2, "admission": "shared"})
    companion.write_bytes(authoring.format_yaml(body_tree))
    authoring.write(
        workspace,
        main=main,
        bodies=companion,
        output_root=output_root,
        cards=frozenset({"BP10-122"}),
    )

    report = authoring.check(replace(workspace, authored=output_root))
    authors = array(object_value(report["selection"])["authors"])
    handoff = next(
        item
        for row in authors
        if (item := object_value(object_value(row)["input"]))["card_no"] == "BP10-122"
    )
    expanded = object_value(
        object_value(object_value(handoff["tree"])["cards"])["BP10-122"]
    )
    face = object_value(next(iter(object_value(expanded["faces"]).values())))
    ability = object_value(array(face["abilities"])[0])
    assert object_value(array(ability["sources"])[0])["admission"] == expected_admission


def test_real_multiface_requires_a_face(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, main_tree, _ = handwriting(tmp_path, handwritten)
    original = object_value(object_value(main_tree["cards"])["BP10-122"])
    main_tree["cards"] = {
        "BP09-005": {
            **original,
            "abilities": [{"line": 1, "evolve": {"cost": {"pp": 1}}}],
        }
    }
    main.write_bytes(authoring.format_yaml(main_tree))
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=bodies,
            output_root=output_root,
            cards=frozenset({"BP09-005"}),
        )
    assert error.value.diagnostic.reason == "ambiguous_face"


def test_inspect_lists_rule_lines_and_exact_body_definitions(
    workspace: authoring.Workspace,
) -> None:
    report = authoring.inspect(workspace, "BP09-005")
    faces = array(object_value(report["payload"])["faces"])
    assert [object_value(face)["ordinal"] for face in faces] == [0, 1]
    assert report["fields"]
    draw = authoring.inspect(workspace, "BP10-122")
    assert draw["existing"]
    assert draw["handwritten"]
    assert draw["resolution_diagnostic"] is None
    assert draw["body_definitions"]
    alias = authoring.inspect(workspace, "BP06-P15")
    assert object_value(array(alias["existing"])[0])["record_key"] == "BP06-048"


def test_alias_cannot_create_a_second_index_record(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, value, _ = handwriting(tmp_path, handwritten)
    cards = object_value(value["cards"])
    cards["BP06-P15"] = cards.pop("BP06-048")
    main.write_bytes(authoring.format_yaml(value))
    before = fingerprint(output_root)
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=bodies,
            output_root=output_root,
            cards=frozenset({"BP06-P15"}),
        )
    assert error.value.diagnostic.reason == "card_alias_already_authored"
    assert error.value.diagnostic.expected == "BP06-048"
    assert fingerprint(output_root) == before


def test_output_requires_the_complete_existing_inventory(
    workspace: authoring.Workspace,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, _, _ = handwriting(tmp_path, handwritten)
    empty = tmp_path / "empty"
    empty.mkdir()
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace, main=main, bodies=bodies, output_root=empty, all_cards=True
        )
    assert error.value.diagnostic.reason == "output_inventory_incomplete"
    assert not tuple(empty.iterdir())


def test_io_failure_reports_completed_and_possibly_partial_files(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    main, bodies, _, _ = handwriting(tmp_path, handwritten)
    main_tree = tree(main)
    for number in ("BP06-048", "BP10-122"):
        object_value(object_value(main_tree["cards"])[number])["note"] = "I/O fixture"
    main.write_bytes(authoring.format_yaml(main_tree))
    original_write = Path.write_bytes
    completed: list[str] = []

    def interrupted(path: Path, data: bytes) -> int:
        if path.is_relative_to(output_root):
            if completed:
                original_write(path, data[:1])
                raise OSError("Injected storage interruption")
            completed.append(path.relative_to(output_root).as_posix())
        return original_write(path, data)

    monkeypatch.setattr(Path, "write_bytes", interrupted)
    with pytest.raises(authoring.WriteError) as error:
        authoring.write(
            workspace, main=main, bodies=bodies, output_root=output_root, all_cards=True
        )
    assert error.value.written == tuple(completed)
    assert len(completed) == 1
    assert (output_root / error.value.failed).stat().st_size == 1
    assert error.value.failed in str(error.value)


@pytest.mark.slow(
    reason="Exercise new-edition rejection, write, replay and all-card anchor refresh through the real source validator"
)
def test_explicit_new_edition_retains_old_body_and_reuses_the_new_version(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, main_tree, body_tree = handwriting(tmp_path, handwritten)
    ability, definition = rename_draw_parameter(main_tree, body_tree)
    main.write_bytes(authoring.format_yaml(main_tree))
    bodies.write_bytes(authoring.format_yaml(body_tree))
    ability["edition"] = 2
    definition["edition"] = 2
    main.write_bytes(authoring.format_yaml(main_tree))
    bodies.write_bytes(authoring.format_yaml(body_tree))
    original = (output_root / "rules/effect-bodies/BP10/000.yaml").read_bytes()
    authoring.write(
        workspace,
        main=main,
        bodies=bodies,
        output_root=output_root,
        cards=frozenset({"BP10-122"}),
    )
    assert (output_root / "rules/effect-bodies/BP10/000.yaml").read_bytes() == original
    assert (output_root / "rules/effect-bodies/BP10/001.yaml").is_file()
    second = authoring.write(
        workspace,
        main=main,
        bodies=bodies,
        output_root=output_root,
        cards=frozenset({"BP10-122"}),
    )
    assert second["written"] == []
    refreshed = authoring.write(
        workspace,
        main=output_root / "rules/author-effects",
        bodies=output_root / "rules/effect-bodies",
        output_root=output_root,
        all_cards=True,
    )
    assert refreshed["written"] == []


@pytest.mark.parametrize("uppercase", [False, True])
def test_keyword_shorthand_keeps_the_existing_lowercase_boundary(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
    uppercase: bool,
) -> None:
    prototype = output_root / "rules/effects/BP01.yaml"
    value = tree(prototype)
    object_value(value["cards"]).pop("BP01-138", None)
    prototype.write_bytes(authoring.format_yaml(value))
    main, _, main_tree, _ = handwriting(tmp_path, handwritten)
    meta = object_value(
        object_value(object_value(main_tree["cards"])["BP01-110"])["meta"]
    )
    main_tree["cards"] = {
        "BP01-138": {
            "meta": meta,
            "abilities": [
                {
                    "line": 1,
                    "keywords": ["Ward", "Aura"] if uppercase else ["ward", "aura"],
                }
            ],
        }
    }
    main.write_bytes(authoring.format_yaml(main_tree))
    before = fingerprint(output_root)
    if uppercase:
        with pytest.raises(authoring.AuthorInputError):
            authoring.write(
                workspace,
                main=main,
                bodies=None,
                output_root=output_root,
                all_cards=True,
            )
        assert fingerprint(output_root) == before
    else:
        authoring.write(
            workspace, main=main, bodies=None, output_root=output_root, all_cards=True
        )
        report = authoring.check(replace(workspace, authored=output_root))
        selected = next(
            object_value(item)
            for item in array(object_value(report["selection"])["authors"])
            if object_value(object_value(item)["input"])["card_no"] == "BP01-138"
        )
        lowered = object_value(selected["input"])
        card = object_value(
            object_value(object_value(lowered["tree"])["cards"])["BP01-138"]
        )
        face = object_value(next(iter(object_value(card["faces"]).values())))
        declaration = object_value(array(face["abilities"])[0])
        assert declaration["keywords"] == ["ward", "aura"]


def test_wrong_owner_and_symlink_inputs_are_rejected(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    main, bodies, _, _ = handwriting(tmp_path, handwritten)
    wrong = tmp_path / "mains/WRONG"
    wrong.mkdir(parents=True)
    shutil.copyfile(main, wrong / "000.yaml")
    before = fingerprint(output_root)
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=wrong.parent,
            bodies=bodies,
            output_root=output_root,
            all_cards=True,
        )
    assert error.value.diagnostic.reason == "wrong_owner"
    linked = tmp_path / "linked.yaml"
    linked.symlink_to(main)
    with pytest.raises(ValueError, match="Symlink"):
        authoring.write(
            workspace,
            main=linked,
            bodies=bodies,
            output_root=output_root,
            all_cards=True,
        )
    assert fingerprint(output_root) == before


@pytest.mark.parametrize(
    "raw",
    [
        b"format: 2\nformat: 2\nkind: author_effect_set\ncards: {}\n",
        b"format: true\nkind: author_effect_set\ncards: {}\n",
        b"format: 2\nkind: author_effect_set\ncards: {}\nunknown: 1\n",
        b"format: 2\nkind: author_effect_set\ncards: {x: {meta: {written_by: [1]}, abilities: [{line: 9007199254740992}]}}\n",
    ],
    ids=["duplicate", "bool-format", "unknown-key", "unsafe-integer"],
)
def test_strict_handwritten_yaml_rejects_without_writes(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    raw: bytes,
) -> None:
    path = tmp_path / "invalid.yaml"
    path.write_bytes(raw)
    before = fingerprint(output_root)
    with pytest.raises(authoring.AuthorInputError):
        authoring.write(
            workspace, main=path, bodies=None, output_root=output_root, all_cards=True
        )
    assert fingerprint(output_root) == before


def test_output_and_handwriting_copies_cannot_change_session_inputs(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    handwritten: tuple[dict[str, JsonValue], dict[str, JsonValue]],
) -> None:
    original = fingerprint(workspace.authored)
    path = output_root / "rules/author-effects/BP10/000.yaml"
    path.write_bytes(b"invalid private output")
    main, bodies, cards, definitions = handwriting(tmp_path, handwritten)
    object_value(cards["cards"]).clear()
    array(definitions["records"]).clear()
    assert fingerprint(workspace.authored) == original
    assert main.read_bytes() == authoring.format_yaml(handwritten[0])
    assert bodies.read_bytes() == authoring.format_yaml(handwritten[1])
