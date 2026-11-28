"""Lookups reused within one command still follow later database writes and caller edits."""

from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import english_only_cards
from sve_carddb.domains.effects.author_inputs import (
    AuthorInputError,
    SourceError,
    parse_document,
)
from sve_carddb.domains.registry.records import CardData, Observation, PrintingData
from sve_carddb.domains.registry.snapshot import RegistryRecord, RegistrySnapshot
from sve_carddb.domains.registry.storage import RegistryFiles
from sve_carddb.workflows import authoring

from ...support.direct_current_fixture import synthetic_current
from ...support.registry_preview_fixtures import mapping_review

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


@pytest.mark.parametrize("excluded", ["jp_printing", "unreviewed", "provisional"])
def test_english_only_requires_confirmed_identity_review_and_no_jp_printing(
    excluded: str,
) -> None:
    hash_value = "sha256:" + "a" * 64
    review = mapping_review(hash_value)
    card = CardData(
        id=review.card_id,
        layout="single",
        identity_state="provisional" if excluded == "provisional" else "confirmed",
        home_set_id="SYN",
    )
    en = PrintingData(
        id="p:" + "1" * 32,
        card_id=card.id,
        region="en",
        card_no="SYN-01EN",
        variant_key="synthetic",
        home_set_id="SYN",
        source_face_map=(),
        observation=Observation(
            region="en",
            card_no="SYN-01EN",
            recipe="registry-observation-v2",
            observation_hash=hash_value,
            rules_hash=hash_value,
        ),
    )
    records = {
        "card": RegistryRecord("card", "card", "test", card, "synthetic", b""),
        "en": RegistryRecord("en", "printing", "test", en, "synthetic", b""),
    }
    if excluded != "unreviewed":
        records["review"] = RegistryRecord(
            "review", "region_mapping_review", "test", review, "synthetic", b""
        )
    if excluded == "jp_printing":
        jp = en.model_copy(
            update={
                "id": "p:" + "2" * 32,
                "region": "jp",
                "card_no": "SYN-01",
                "observation": en.observation.model_copy(
                    update={"region": "jp", "card_no": "SYN-01"}
                ),
            }
        )
        records["jp"] = RegistryRecord("jp", "printing", "test", jp, "synthetic", b"")
    assert english_only_cards(RegistrySnapshot(RegistryFiles(b"", ()), records)) == (
        frozenset()
    )


def test_a_japanese_card_rejects_its_official_english_number(tmp_path: Path) -> None:
    with synthetic_current(tmp_path) as current:
        printing = current.db.rows("printing")[0].values
        with current.db.transaction():
            current.db.insert(
                "printing",
                dict(printing)
                | {"id": "english", "region": "en", "card_no": "SYN-01EN"},
            )
        assert current.printing_card("SYN-01") == printing["card_id"]
        with pytest.raises(SourceError, match="record_key_identity_mismatch"):
            current.printing_card("SYN-01EN")


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


def test_check_reads_files_added_after_an_earlier_check(
    workspace: authoring.Workspace,
) -> None:
    assert authoring.check(workspace)["status"] == "ok"
    stray = workspace.authored / "rules/author-effects/FIX/001.yaml"
    stray.parent.mkdir(parents=True)
    stray.write_bytes(b"{}")
    with pytest.raises(AuthorInputError):
        authoring.check(workspace)
