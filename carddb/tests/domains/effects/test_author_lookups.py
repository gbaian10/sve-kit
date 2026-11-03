"""Lookups reused within one command still follow later database writes and caller edits."""

from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, parse_document
from sve_carddb.domains.effects.author_sources import SourceError
from sve_carddb.workflows import authoring

from .author_entry_cases import meta
from .test_author_current_absent import synthetic_current

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.domains.effects.author_expansion import AuthorExpander

REPO = Path(__file__).resolve().parents[4]


@pytest.fixture
def workspace(
    tmp_path: Path, expander: AuthorExpander
) -> Iterator[authoring.Workspace]:
    authored = tmp_path / "authored"
    authored.mkdir()
    with synthetic_current(tmp_path) as current:
        yield authoring.Workspace(
            authoring.Inputs(
                profile="public", repo=REPO, jp_batch_id="sha256:" + "0" * 64
            ),
            current,
            expander,
            authored,
        )


@pytest.fixture
def main(tmp_path: Path) -> Path:
    path = tmp_path / "main.yaml"
    path.write_bytes(
        canonical(
            {
                "format": 2,
                "kind": "author_effect_set",
                "cards": {
                    "SYN-01": {
                        "meta": {"written_by": meta()["written_by"]},
                        "abilities": [],
                    }
                },
            }
        )
    )
    return path


def test_repeated_documents_return_independent_values() -> None:
    raw = canonical({"cards": {"SYN-01": {"abilities": [{"line": 1}]}}})
    first = parse_document(raw)
    object_value(first.value)["cards"] = {}
    second = parse_document(raw)
    assert second.value == {"cards": {"SYN-01": {"abilities": [{"line": 1}]}}}
    assert second.value is not first.value
    assert second.locations.locate(("cards", "SYN-01")) is not None


def test_printing_lookup_sees_rows_written_after_the_first_lookup(
    tmp_path: Path,
) -> None:
    with synthetic_current(tmp_path) as current:
        card_id = current.printing_card("SYN-01")
        with pytest.raises(SourceError):
            current.printing_card("SYN-02")
        printing = dict(current.db.rows("printing")[0].values)
        with current.db.transaction():
            current.db.insert(
                "printing",
                printing
                | {"id": "SYN-02", "card_no": "SYN-02", "card_no_state": "official"},
            )
        assert current.printing_card("SYN-02") == card_id


@pytest.mark.parametrize("mutation", ["insert", "update", "delete"])
def test_printing_lookup_discards_rolled_back_changes(
    tmp_path: Path, mutation: str
) -> None:
    with synthetic_current(tmp_path) as current:
        card_id = current.printing_card("SYN-01")
        printing = dict(current.db.rows("printing")[0].values)
        number = "SYN-02" if mutation == "insert" else "SYN-01"
        with pytest.raises(RuntimeError, match="cancel transaction"):  # ruff: ignore[pytest-raises-with-multiple-statements] -- rollback must cover the reads and writes
            with current.db.transaction():
                if mutation == "insert":
                    current.db.insert(
                        "printing", printing | {"id": "SYN-02", "card_no": number}
                    )
                elif mutation == "update":
                    current.db.update(
                        "printing", {"id": printing["id"]}, {"card_no": "SYN-03"}
                    )
                else:
                    current.db.delete("printing", {"id": printing["id"]})
                if mutation == "insert":
                    assert current.printing_card(number) == card_id
                else:
                    with pytest.raises(SourceError):
                        current.printing_card(number)
                raise RuntimeError("cancel transaction")
        if mutation == "insert":
            with pytest.raises(SourceError):
                current.printing_card(number)
        else:
            assert current.printing_card(number) == card_id


def test_write_invalidates_inputs_shared_with_replaced_workspace(
    workspace: authoring.Workspace, main: Path, tmp_path: Path
) -> None:
    output_root = tmp_path / "other-authored"
    output_root.mkdir()
    other = replace(workspace, authored=output_root)
    assert object_value(authoring.check(workspace)["selection"])["authors"] == []
    assert object_value(authoring.check(other)["selection"])["authors"] == []
    authoring.write(
        workspace, main=main, bodies=None, output_root=output_root, all_cards=True
    )
    assert object_value(authoring.check(other)["selection"])["authors"]
    assert object_value(authoring.check(workspace)["selection"])["authors"] == []


def test_first_partial_write_invalidates_inputs(
    workspace: authoring.Workspace, main: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    assert authoring.check(workspace)["status"] == "ok"
    original_write = Path.write_bytes

    def interrupted(path: Path, data: bytes) -> int:
        if path.is_relative_to(workspace.authored):
            original_write(path, data[:1])
            raise OSError("Injected storage interruption")
        return original_write(path, data)

    monkeypatch.setattr(Path, "write_bytes", interrupted)
    with pytest.raises(authoring.WriteError) as error:
        authoring.write(
            workspace,
            main=main,
            bodies=None,
            output_root=workspace.authored,
            all_cards=True,
        )
    assert error.value.written == ()
    assert (workspace.authored / error.value.failed).stat().st_size == 1
    with pytest.raises(AuthorInputError):
        authoring.check(workspace)
