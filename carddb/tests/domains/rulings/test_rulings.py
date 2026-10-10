"""Real builder and SQLite boundaries preserve pending references without active edges."""

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.contracts.rulings import Resolution
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.reader import document, load
from sve_carddb.domains.rulings.resolution import build
from sve_carddb.domains.rulings.storage import write
from sve_carddb.domains.translations.four_layer_storage import write_binding

from ..translations.test_four_layer_storage import compiled as compiled  # ruff: ignore[useless-import-alias] -- shared SQLite schema fixture
from ..translations.test_four_layer_storage import stored as unbound  # ruff: ignore[unused-import] -- register the parent fixture under a distinct name

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.rulings.reader import Document


@pytest.fixture
def stored(request: pytest.FixtureRequest) -> tuple[Database, Frame, SourceBinding]:
    parent: tuple[Database, Frame, SourceBinding] = request.getfixturevalue("unbound")
    db, _, binding = parent
    with db.transaction():
        write_binding(db, "use:synthetic", binding, {})
    return parent


def ruling(**changes: JsonValue) -> Document:
    data: dict[str, JsonValue] = {
        "format": 2,
        "kind": "ruling",
        "id": "R-0001",
        "revision": 1,
        "question": "A synthetic question",
        "decision": "A synthetic answer",
        "evidence": [],
        "strength": "inferred",
        "decided_on": "2026-10-10",
        "applies_to": ["T0123456789", "T0123456789", "E.synthetic"],
        "hint": {"ja": "合成"},
    }
    return document("authored/rules/rulings/R-0001.yaml", canonical(data | changes))


@pytest.mark.parametrize("revision", [None, False, True, "1", 0, -1, 9007199254740992])
def test_bad_revision_is_rejected(revision: JsonValue) -> None:
    raw = ruling().raw.replace(
        b'"revision":1',
        b'"revision":' + str(revision).lower().encode()
        if isinstance(revision, int)
        else b'"revision":null'
        if revision is None
        else b'"revision":"1"',
    )
    with pytest.raises(ValueError, match="Invalid versioned ruling"):
        document("authored/rules/rulings/R-0001.yaml", raw)


@pytest.mark.parametrize(
    "change", [{"format": 1}, {"version": 1}, {"extra": None}, {"kind": "other"}]
)
def test_old_envelope_and_unknown_fields_are_rejected(
    change: dict[str, JsonValue],
) -> None:
    with pytest.raises(ValueError, match="Invalid versioned ruling"):
        ruling(**change)


def test_missing_revision_duplicate_keys_and_unsafe_path_are_rejected() -> None:
    found = ruling()
    for raw in (found.raw.replace(b'"revision":1,', b""), b'{"format":2,"format":2}'):
        with pytest.raises(ValueError, match="Invalid versioned ruling"):
            document(found.source.path, raw)
    with pytest.raises(ValueError, match="Invalid versioned ruling"):
        document("authored/rules/rulings/../R-0001.yaml", found.raw)


def test_unknown_references_preserve_ordinals_and_ir_without_executable_edges(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    found = ruling()
    with db.transaction():
        report = write(db, (found,), "a" * 40)
    assert len(db.rows("ruling_resolution")) == 2
    assert all(row.values["frame_id"] is None for row in db.rows("ruling_resolution"))
    assert len(db.rows("ruling_retained_reference")) == 1
    assert [r.ruling_ref.reference_ordinal for r in report.resolutions] == [0, 1]
    assert all(
        r.level == "reference"
        and r.legacy.namespace is None
        and r.legacy.occurrence is None
        for r in report.resolutions
    )
    assert report.retained[0].reference_id == "E.synthetic"
    assert report.payload()["active_template_edges"] == 0


@pytest.mark.parametrize(
    "change",
    [
        {"status": "resolved"},
        {"reason": "no_candidate"},
        {"level": "occurrence"},
        {
            "legacy": {
                "namespace": "unknown",
                "template_id": "T-old",
                "occurrence": None,
            }
        },
    ],
)
def test_pending_shape_is_closed(change: dict[str, JsonValue]) -> None:
    found = ruling()
    report = build((found,))
    with pytest.raises(ValidationError):
        Resolution.model_validate_json(
            canonical(report.resolutions[0].model_dump(mode="json") | change)
        )


def fixed_cases() -> tuple[dict[str, JsonValue], ...]:
    import json  # ruff: ignore[import-outside-top-level] -- fixed public cases are loaded once during test collection
    from pathlib import Path  # ruff: ignore[import-outside-top-level] -- locate the repository-owned fixture independently of cwd

    data = json.loads(
        (
            Path(__file__).resolve().parents[4]
            / "docs/schema/domains/four-layer-cases.json"
        ).read_bytes()
    )
    return tuple(
        c | {"data": data["fixtures"][c["input"]["fixture"]]}
        for c in data["cases"]
        if c["operation"] in {"ruling_document_shape", "ruling_reference_shape"}
    )


@pytest.mark.parametrize("case", fixed_cases(), ids=lambda c: str(c["id"]))
def test_fixed_ruling_cases_use_closed_public_boundaries(
    case: dict[str, JsonValue],
) -> None:
    import copy  # ruff: ignore[import-outside-top-level] -- each mutation requires an independent fixture
    import json  # ruff: ignore[import-outside-top-level] -- oversized revision must reach the reader instead of the canonical writer

    data = copy.deepcopy(case["data"])
    request = case["input"]
    expected = case["expected"]
    assert isinstance(data, dict)
    assert isinstance(request, dict)
    assert isinstance(expected, dict)
    changes = request.get("changes", {})
    assert isinstance(changes, dict)
    for path, value in changes.items():
        target = data
        parts = path.lstrip("/").split("/")
        for name in parts[:-1]:
            child = target[name]
            assert isinstance(child, dict)
            target = child
        target[parts[-1]] = value
    removed = request.get("remove", [])
    assert isinstance(removed, list)
    for removed_path in removed:
        assert isinstance(removed_path, str)
        target = data
        parts = removed_path.lstrip("/").split("/")
        for name in parts[:-1]:
            child = target[name]
            assert isinstance(child, dict)
            target = child
        target.pop(parts[-1])
    raw = json.dumps(
        data, sort_keys=True, ensure_ascii=False, separators=(",", ":")
    ).encode()

    def validate() -> None:
        if case["operation"] == "ruling_document_shape":
            found = document("authored/rules/rulings/R-0001.yaml", raw)
            assert found.raw == raw
            assert found.source.sha256 == digest(raw)
        else:
            found_resolution = Resolution.model_validate_json(raw)
            assert found_resolution.model_dump(mode="json") == data
            assert found_resolution.status == "pending"
            assert found_resolution.target is None

    if expected["result"] == "accept":
        validate()
    else:
        with pytest.raises(
            ValueError, match=r"Invalid versioned ruling|validation error"
        ):
            validate()


@pytest.mark.parametrize(
    "reference", ["T-old", "T012345678", "T01234567890", "frame:" + "a" * 64, "E."]
)
def test_unsupported_reference_is_rejected(reference: str) -> None:
    with pytest.raises(ValueError, match="Unsupported ruling"):
        build((ruling(applies_to=[reference]),))


def test_load_requires_directory_and_preserves_multiple_current_documents(
    tmp_path: Path,
) -> None:
    with pytest.raises(ValueError, match="authored/rules/rulings"):
        load(tmp_path)
    root = tmp_path / "authored/rules/rulings"
    root.mkdir(parents=True)
    assert load(tmp_path) == ()
    (root / "a.yaml").write_bytes(ruling().raw)
    (root / "b.yaml").write_bytes(ruling(id="R-0002").raw)
    documents = load(tmp_path)
    assert [d.ruling.id for d in documents] == ["R-0001", "R-0002"]
    assert len(build(documents).resolutions) == 4
    (root / "b.yaml").write_bytes(ruling(revision=2).raw)
    with pytest.raises(ValueError, match="Duplicate current ruling"):
        load(tmp_path)
