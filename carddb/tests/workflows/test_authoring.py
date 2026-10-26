"""Repository integration through the same public authoring entry as the CLI."""

import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- create a private fixed Git tree for regression tests
from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, digest, object_value, parse, string
from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.workflows import authoring

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

REPO = Path(__file__).resolve().parents[3]


@pytest.fixture(scope="module")
def workspace() -> Iterator[authoring.Workspace]:
    batch = string(
        object_value(parse((REPO / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    inputs = authoring.Inputs(
        profile="public",
        repo=REPO,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )
    with authoring.prepare(
        inputs, cards=frozenset({"BP09-005", "BP01-138", "BP19-P32", "BP01-048"})
    ) as prepared:
        yield prepared


@pytest.fixture
def output_root(tmp_path: Path) -> Path:
    return Path(shutil.copytree(REPO / "authored", tmp_path / "authored"))


def tree(path: Path) -> dict[str, JsonValue]:
    return object_value(JSON_VALUE.validate_python(parse_yaml(path.read_bytes())))


def handwriting(
    tmp_path: Path,
) -> tuple[Path, Path, dict[str, JsonValue], dict[str, JsonValue]]:
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


def test_repository_check_is_the_ci_entry(workspace: authoring.Workspace) -> None:
    report = authoring.check(workspace)
    authors = array(object_value(report["selection"])["authors"])
    assert len(authors) >= 14
    assert all(object_value(item)["status"] == "accepted" for item in authors)


def test_write_emits_legacy_sources_and_reuses_companions(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
) -> None:
    main, bodies, hand, _ = handwriting(tmp_path)
    report = authoring.write(
        workspace, main=main, bodies=bodies, output_root=output_root, all_cards=True
    )
    assert report["cards"] == ["BP01-110", "BP06-048", "BP10-122"]
    assert all(
        not string(path).startswith("rules/effect-bodies/")
        for path in array(report["written"])
    )
    document = tree(output_root / "rules/author-effects/BP10/000.yaml")
    assert document["format"] == 1
    card = object_value(object_value(document["cards"])["BP10-122"])
    meta = object_value(card["meta"])
    assert (
        meta["written_by"]
        == object_value(object_value(object_value(hand["cards"])["BP10-122"])["meta"])[
            "written_by"
        ]
    )
    assert meta["dsl"] == "1.0"
    assert meta["status"] == "draft"
    assert string(meta["source"]).startswith("effect-source-v1:sha256:")
    ability = object_value(array(card["abilities"])[0])
    assert object_value(ability["generated_by"])["params"] == {
        "n": {"type": "Nat", "value": 1}
    }
    source = object_value(array(ability["sources"])[0])
    assert source["ability_scope"] == "ability_0"
    assert object_value(source["occurrence"])["line_ordinal"] == 0
    index = tree(output_root / "rules/effects-index.yaml")
    assert (
        sum(
            object_value(item)["record_key"] == "BP10-122"
            for item in array(index["records"])
        )
        == 1
    )
    second = authoring.write(
        workspace, main=main, bodies=bodies, output_root=output_root, all_cards=True
    )
    assert second["written"] == []
    assert authoring.check(replace(workspace, authored=output_root))["status"] == "ok"


@pytest.mark.parametrize(
    ("mutation", "reason"),
    [
        ("line", "missing_source"),
        ("face", "missing_source"),
        ("section", "missing_source"),
        ("scope", "missing_source"),
        ("param", "source_param_value_mismatch"),
        ("missing_body", "body_missing"),
        ("body_author", "body_metadata_conflict"),
        ("body_registration", "body_metadata_conflict"),
        ("body_review", "body_metadata_conflict"),
        ("body_syntax", "invalid_syntax_tree"),
        ("body_source", "missing_parameter_source"),
        ("author", "author_metadata_conflict"),
        ("status", "author_metadata_conflict"),
        ("closure", "source_ability_closure_mismatch"),
        ("unknown", "ability_head_conflict"),
        ("duplicate_body", "duplicate_body_id"),
        ("editions", "ambiguous_body_version"),
    ],
)
def test_write_rejects_without_any_writes(  # ruff: ignore[complex-structure, too-many-branches] -- exercise independent rejected shorthand mutations against the same public writer
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    mutation: str,
    reason: str,
) -> None:
    main, bodies, main_tree, body_tree = handwriting(tmp_path)
    card = object_value(object_value(main_tree["cards"])["BP10-122"])
    ability = object_value(array(card["abilities"])[0])
    definition = object_value(array(body_tree["records"])[0])
    match mutation:
        case "line" | "face" | "section" | "scope":
            ability[mutation] = 99
        case "param":
            ability["params"] = {"n": 2}
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
        case "body_source":
            definition["params"] = {
                "n": {
                    "type": "Nat",
                    "source": {"slot": "leaf_0", "component": "constant"},
                }
            }
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
    if mutation in {"param", "body_source", "body_syntax"}:
        assert error.value.diagnostic.card_line == 1
        assert error.value.diagnostic.line is not None
    assert fingerprint(output_root) == before


def test_real_multiface_ambiguity_precedes_pending(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
) -> None:
    main, bodies, main_tree, _ = handwriting(tmp_path)
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
    assert error.value.diagnostic.reason == "ambiguous_source"
    assert error.value.diagnostic.expected == "face: 1, 2"
    ability = object_value(
        array(object_value(object_value(main_tree["cards"])["BP09-005"])["abilities"])[
            0
        ]
    )
    ability["face"] = "back"
    main.write_bytes(authoring.format_yaml(main_tree))
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=bodies,
            output_root=output_root,
            cards=frozenset({"BP09-005"}),
        )
    assert error.value.diagnostic.reason == "unsupported_source_projection"


def test_inspect_lists_pending_faces_and_exact_body_definitions(
    workspace: authoring.Workspace,
) -> None:
    report = authoring.inspect(workspace, "BP09-005")
    candidates = [object_value(item) for item in array(report["candidates"])]
    assert {(item["face"], item["line"]) for item in candidates} == {
        (1, 1),
        (2, 1),
        (2, 2),
    }
    assert report["gaps"]
    assert all(item["gap"] and item["state"] == "pending" for item in candidates)
    draw = authoring.inspect(workspace, "BP10-122")
    assert draw["bodies"]
    assert draw["existing"]
    candidate = object_value(array(draw["candidates"])[0])
    assert candidate["slots"]
    assert candidate["supported"] is True
    assert candidate["values"] == {"leaf_0": 1}
    alias = authoring.inspect(workspace, "BP06-P15")
    assert object_value(array(alias["existing"])[0])["record_key"] == "BP06-048"
    storm = authoring.inspect(workspace, "BP01-048")
    assert any(
        object_value(item)["gap"] == "unsupported_source_keyword"
        for item in array(storm["gaps"])
    )


def test_alias_cannot_create_a_second_index_record(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
) -> None:
    main, bodies, value, _ = handwriting(tmp_path)
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
) -> None:
    main, bodies, _, _ = handwriting(tmp_path)
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
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    main, bodies, _, _ = handwriting(tmp_path)
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


def test_explicit_new_edition_retains_old_body_and_reuses_the_new_version(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
) -> None:
    main, bodies, main_tree, body_tree = handwriting(tmp_path)
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
    main.write_bytes(authoring.format_yaml(main_tree))
    bodies.write_bytes(authoring.format_yaml(body_tree))
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=bodies,
            output_root=output_root,
            cards=frozenset({"BP10-122"}),
        )
    assert error.value.diagnostic.reason == "new_body_requires_explicit_edition"
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


@pytest.mark.parametrize("uppercase", [False, True])
def test_keyword_shorthand_keeps_the_existing_lowercase_boundary(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
    uppercase: bool,
) -> None:
    prototype = output_root / "rules/effects/BP01.yaml"
    value = tree(prototype)
    object_value(value["cards"]).pop("BP01-138", None)
    prototype.write_bytes(authoring.format_yaml(value))
    main, _, main_tree, _ = handwriting(tmp_path)
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
        declarations = array(object_value(selected["input"])["declaration_sidecar"])
        declaration = object_value(declarations[0])
        assert object_value(declaration["declaration"])["keywords"] == ["ward", "aura"]
        assert "ability_scope" not in object_value(declaration["source_use"])


def test_wrong_owner_and_symlink_inputs_are_rejected(
    workspace: authoring.Workspace,
    output_root: Path,
    tmp_path: Path,
) -> None:
    main, bodies, _, _ = handwriting(tmp_path)
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


def git_tree(repo: Path) -> str:
    executable = shutil.which("git")
    assert executable is not None

    def run(*args: str) -> bytes:
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git argv in a temporary repository
            [executable, "-C", str(repo), *args], check=True, capture_output=True
        ).stdout

    run("init", "--quiet")
    run("add", ".")
    tree_id = run("write-tree").decode().strip()
    commit = run(
        "-c",
        "user.name=Synthetic author",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit-tree",
        tree_id,
        "-m",
        "Frozen regression fixture",
    )
    return commit.decode().strip()


def test_prototype_removal_checks_fixed_ast_positions_and_unchanged_baselines(
    workspace: authoring.Workspace,
    tmp_path: Path,
) -> None:
    repo = tmp_path / "repository"
    authored = Path(shutil.copytree(REPO / "authored", repo / "authored"))
    shutil.copytree(REPO / "dsl", repo / "dsl")
    baseline = repo / "carddb/tests/fixtures/effect_baseline"
    baseline.mkdir(parents=True)
    prototype_path = authored / "rules/effects/BP10.yaml"
    prototype = tree(prototype_path)
    object_value(prototype["cards"])["BP10-122"] = deepcopy(
        next(iter(object_value(prototype["cards"]).values()))
    )
    frozen = object_value(
        parse((REPO / "carddb/tests/fixtures/effect_baseline/BP10.json").read_bytes())
    )
    frozen = {"BP10-122": object_value(frozen["BP10-122"])}
    object_value(frozen["BP10-122"])["ast_sha256"] = digest(
        canonical(object_value(prototype["cards"])["BP10-122"])
    ).removeprefix("sha256:")
    (baseline / "BP10.json").write_bytes(canonical(frozen))
    prototype_path.write_bytes(authoring.format_yaml(prototype))
    index_path = authored / "rules/effects-index.yaml"
    index = tree(index_path)
    index["records"] = [
        item
        for item in array(index["records"])
        if object_value(item)["record_key"] != "BP10-122"
    ]
    index_path.write_bytes(authoring.format_yaml(index))
    (authored / "rules/author-effects/BP10/000.yaml").unlink()
    base = git_tree(repo)
    main, bodies, _, _ = handwriting(tmp_path)
    configured = replace(
        workspace,
        inputs=workspace.inputs.model_copy(update={"repo": repo}),
        authored=authored,
    )
    before_baseline = fingerprint(baseline)
    prototype = object_value(parse((baseline / "BP10.json").read_bytes()))
    object_value(array(object_value(prototype["BP10-122"])["positions"])[0])["face"] = (
        99
    )
    (baseline / "BP10.json").write_bytes(canonical(prototype))
    base = git_tree(repo)
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.write(
            configured,
            main=main,
            bodies=bodies,
            output_root=authored,
            cards=frozenset({"BP10-122"}),
            remove_prototype=True,
            base=base,
            baseline_root=baseline,
        )
    assert error.value.diagnostic.reason == "prototype_position_mismatch"
    (baseline / "BP10.json").write_bytes(canonical(frozen))
    base = git_tree(repo)
    authoring.write(
        configured,
        main=main,
        bodies=bodies,
        output_root=authored,
        cards=frozenset({"BP10-122"}),
        remove_prototype=True,
        base=base,
        baseline_root=baseline,
    )
    report = authoring.check(configured, base=base, baseline_root=baseline)
    assert object_value(report["regression"])["removed"] == ["BP10-122"]
    assert fingerprint(baseline) == before_baseline
    prototype_path = authored / "rules/effects/BP01.yaml"
    prototype = tree(prototype_path)
    object_value(prototype["cards"]).pop(next(iter(object_value(prototype["cards"]))))
    prototype_path.write_bytes(authoring.format_yaml(prototype))
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.check(configured, base=base, baseline_root=baseline)
    assert error.value.diagnostic.reason == "prototype_replacement_not_current"
    (baseline / "BP10.json").write_bytes((baseline / "BP10.json").read_bytes() + b"\n")
    with pytest.raises(authoring.AuthorInputError) as error:
        authoring.check(configured, base=base, baseline_root=baseline)
    assert error.value.diagnostic.reason == "prototype_baseline_bytes_changed"


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
