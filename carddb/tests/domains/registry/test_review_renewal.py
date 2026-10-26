"""Complete review renewal against small synthetic Git histories and sealed pages."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.domains.registry.build import build
from sve_carddb.domains.registry.preview import (
    FrozenRegions,
    import_preview,
    plan_preview,
)
from sve_carddb.domains.registry.preview.review_inputs import (
    RegistryReviewInputs,
    ReviewInputPin,
)
from sve_carddb.domains.registry.records import MappingReviewData
from sve_carddb.domains.registry.renew_commands import app
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import plan_files, write_files
from sve_carddb.domains.registry.transitions.renewal import load_renewed_registry
from sve_carddb.domains.registry.transitions.review_sources import RegionalSource
from sve_carddb.domains.registry.transitions.writer import (
    Decisions,
    Recipe,
    plan,
    write,
)

from ...support.adoption_fixtures import commit, git
from ...support.registry_preview_fixtures import BUILD, parents
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic registry input dependency
from .test_registry_preview_en_sources import frozen_registry as frozen_registry  # ruff: ignore[useless-import-alias] -- small sealed EN/JP source fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs


@dataclass(frozen=True)
class Case:
    recipe: Recipe
    decisions: Decisions
    provider: FrozenRegions

    @property
    def authored(self) -> Path:
        return self.recipe.repo / "authored"


def artifact(root: Path, raw: bytes, *, name: str, as_of: str) -> ReviewInputPin:
    batch = root / name
    batch.mkdir(parents=True)
    (batch / "jp.jsonl").write_bytes(raw)
    inventory = canonical(
        {"files": {"jp.jsonl": {"sha256": digest(raw), "bytes": len(raw)}}}
    )
    (batch / "inventory.json").write_bytes(inventory)
    (batch / "SHA256SUMS").write_text(
        digest(raw).removeprefix("sha256:") + "  jp.jsonl\n"
    )
    return ReviewInputPin(
        root=root,
        batch_id=name,
        inventory_sha256=digest(inventory),
        coverage_hash=digest(raw),
        bytes=len(raw),
        as_of=as_of,
    )


@pytest.fixture
def case(tmp_path: Path, frozen_registry: tuple[Path, FrozenRegions]) -> Case:
    authored, provider = frozen_registry
    repo = tmp_path / "repository"
    repo.mkdir()
    authored.rename(repo / "authored")
    (repo / "carddb").mkdir()
    (repo / "carddb/uv.lock").write_text("Synthetic lock\n")
    (repo / ".gitignore").write_text("carddb/.cache/\n")
    git(repo, "init", "-q")
    revision = commit(repo)
    raw = tmp_path / "jp.jsonl"
    placeholder = artifact(
        tmp_path / "reviews",
        b'{"number":"PLACEHOLDER"}\n',
        name="placeholder",
        as_of="2026-10-01",
    )
    recipe = Recipe(
        repo=repo,
        archive=provider.jp._sources.root,
        store_id=provider.jp._sources.store_id,
        sources=(
            RegionalSource(
                region="en", card_batch=provider.en.batch_id, parser_version="en-pin"
            ),
            RegionalSource(
                region="jp", card_batch=provider.jp.batch_id, parser_version="jp-pin"
            ),
        ),
        registry_review_inputs=placeholder,
        program_revision=revision,
    )
    recipe_file = tmp_path / "recipe.json"
    recipe_file.write_bytes(canonical(recipe.model_dump(mode="json")))
    result = CliRunner().invoke(
        app, ["review-jp-input", "--inputs", str(recipe_file), "--output", str(raw)]
    )
    assert result.exit_code == 0, result.output
    pin = artifact(
        tmp_path / "reviews", raw.read_bytes(), name="renewed", as_of="2026-10-01"
    )
    snapshot = load_registry(repo / "authored")
    cards = tuple(
        sorted(
            r.data.card_id
            for r in snapshot.records.values()
            if isinstance(r.data, MappingReviewData)
        )
    )
    return Case(
        recipe.model_copy(update={"registry_review_inputs": pin}),
        Decisions(
            card_ids=cards,
            conclusion="confirmed_none",
            reason="Synthetic complete review",
        ),
        provider,
    )


def test_complete_renewal_preserves_history_imports_f1_and_is_idempotent(
    case: Case,
) -> None:
    before = {p: p.read_bytes() for p in case.authored.rglob("*.yaml")}
    planned = plan(case.recipe, case.decisions)
    assert planned.shard is not None
    assert not (case.authored / "identity-transitions").exists()
    write(case.authored, planned)
    assert all(p.read_bytes() == raw for p, raw in before.items())
    snapshot = load_renewed_registry(case.authored, case.recipe.evidence())
    provider = FrozenRegions(
        jp=case.provider.jp,
        en=case.provider.en,
        review_inputs=RegistryReviewInputs(case.recipe.registry_review_inputs),
    )
    projected = plan_preview(
        case.authored, provider, regions=("en", "jp"), snapshot=snapshot
    )
    assert len(projected.included("region_mapping_review")) == 1
    selected_review = projected.included("region_mapping_review")[0].data
    assert isinstance(selected_review, MappingReviewData)
    assert selected_review.as_of == "2026-10-01"
    assert {use.usage for use in projected.source_uses()} >= {
        "identity_transition_coverage",
        "identity_transition_observation",
    }
    assert str(case.recipe.repo).encode() not in planned.shard.exact_content
    with create_database(compile_build(("en", "related"))) as db:
        parents(db, projected)
        uses = import_preview(
            db, projected, authored_revision=case.recipe.program_revision, build=BUILD
        )
        assert set(uses.uses) == set(projected.source_uses())
        review = db.rows("region_mapping_review")[0].values
        source = next(
            r.values
            for r in db.rows("source_record")
            if r.values["id"] == review["source_id"]
        )
        assert source["parser_version"] == "identity-transition-v1"
        assert not db.rows("identity_change")
    assert plan(case.recipe, case.decisions).shard is None


def test_second_renewal_uses_exact_previous_effective_review(case: Case) -> None:
    write(case.authored, plan(case.recipe, case.decisions))
    commit(case.recipe.repo)
    pin = case.recipe.registry_review_inputs
    successor = artifact(
        pin.root,
        (pin.root / pin.batch_id / "jp.jsonl").read_bytes(),
        name="next",
        as_of="2026-10-02",
    )
    recipe = case.recipe.model_copy(update={"registry_review_inputs": successor})
    planned = plan(recipe, case.decisions)
    assert planned.shard is not None
    transition = planned.shard.envelope().records[0]
    assert transition.sequence == 2
    assert transition.updates[0].before is not None
    assert transition.updates[0].before.transition_key == '["identity_transition",1]'
    write(case.authored, planned)
    assert plan(recipe, case.decisions).shard is None
    assert len(load_renewed_registry(case.authored, recipe.evidence()).transitions) == 2


@pytest.mark.parametrize(
    "damage", ["before", "basis", "observation", "evidence", "dependency"]
)
def test_resigned_claims_cannot_replace_independent_reconstruction(
    case: Case, damage: str
) -> None:
    planned = plan(case.recipe, case.decisions)
    assert planned.shard is not None
    raw = object_value(parse(planned.shard.content))
    records = raw["records"]
    assert isinstance(records, list)
    assert isinstance(records[0], dict)
    record = records[0]
    if damage in {"before", "observation"}:
        updates = record["updates"]
        assert isinstance(updates, list)
        assert isinstance(updates[0], dict)
        if damage == "before":
            before = updates[0]["before"]
            assert isinstance(before, dict)
            before["record_hash"] = "sha256:" + "0" * 64
        else:
            after = updates[0]["after"]
            assert isinstance(after, dict)
            assert isinstance(after["data"], dict)
            after["data"]["observations"] = []
    elif damage == "basis":
        basis = record["registry_basis"]
        assert isinstance(basis, dict)
        basis["index_hash"] = "sha256:" + "0" * 64
    elif damage == "evidence":
        evidence = record["evidence"]
        assert isinstance(evidence, list)
        assert isinstance(evidence[0], dict)
        evidence[0]["source_version_id"] = "src:v1:" + "0" * 64
    else:
        context = record["review_context"]
        assert isinstance(context, dict)
        assert isinstance(context["context"], dict)
        context["context"]["program_revision"] = "0" * 40
    path = case.authored / planned.shard.path
    path.parent.mkdir()
    path.write_bytes(canonical(raw))
    with pytest.raises(
        ValueError, match=r"renewal|Renewal|registry|Registry|Git|identity transition"
    ):
        load_renewed_registry(case.authored, case.recipe.evidence())


def test_modified_historical_bytes_are_rejected_even_without_semantic_change(
    case: Case,
) -> None:
    write(case.authored, plan(case.recipe, case.decisions))
    path = next((case.authored / "registry/printing").rglob("*.yaml"))
    path.write_bytes(path.read_bytes() + b"\n")
    commit(case.recipe.repo)
    with pytest.raises(ValueError, match="exact historical shard bytes"):
        load_renewed_registry(case.authored, case.recipe.evidence())


def test_modified_permanent_jp_input_is_not_a_renewal(case: Case) -> None:
    pin = case.recipe.registry_review_inputs
    (pin.root / pin.batch_id / "jp.jsonl").write_bytes(b'{"number":"CHANGED"}\n')
    with pytest.raises(ValueError, match="bytes or hash mismatch"):
        plan(case.recipe, case.decisions)
    assert not (case.authored / "identity-transitions").exists()


def test_uncommitted_index_bytes_are_not_an_immutable_basis(case: Case) -> None:
    index = case.authored / "ids/index.yaml"
    index.write_bytes(index.read_bytes() + b"\n")
    with pytest.raises(ValueError, match="committed exact Git bytes"):
        plan(case.recipe, case.decisions)


def test_cli_check_write_and_repeated_write(case: Case, tmp_path: Path) -> None:
    recipe = tmp_path / "request.json"
    decisions = tmp_path / "decisions.json"
    recipe.write_bytes(canonical(case.recipe.model_dump(mode="json")))
    decisions.write_bytes(canonical(case.decisions.model_dump(mode="json")))
    args = ["renew-reviews", "--inputs", str(recipe), "--decisions", str(decisions)]
    runner = CliRunner()
    assert runner.invoke(app, [*args, "--check"]).exit_code != 0
    assert not (case.authored / "identity-transitions").exists()
    assert runner.invoke(app, [*args, "--write"]).exit_code == 0
    assert runner.invoke(app, [*args, "--check"]).exit_code == 0
    repeated = runner.invoke(app, [*args, "--write"])
    assert repeated.exit_code == 0
    assert object_value(parse(repeated.output.encode()))["written"] is False


def test_original_approval_is_not_selected_with_an_old_date_pin(case: Case) -> None:
    write(case.authored, plan(case.recipe, case.decisions))
    snapshot = load_renewed_registry(case.authored, case.recipe.evidence())
    pin = case.recipe.registry_review_inputs.model_copy(update={"as_of": "2026-09-28"})
    provider = FrozenRegions(
        jp=case.provider.jp,
        en=case.provider.en,
        review_inputs=RegistryReviewInputs(pin),
    )
    projected = plan_preview(
        case.authored, provider, regions=("en", "jp"), snapshot=snapshot
    )
    assert not projected.included("region_mapping_review")
    assert any(
        "missing_review_coverage" in row.reasons for row in projected.projections
    )


def test_untracked_program_code_cannot_claim_the_old_program_revision(
    case: Case,
) -> None:
    path = case.recipe.repo / "carddb/src/new.py"
    path.parent.mkdir(parents=True)
    path.write_text("NEW_CODE = True\n")
    with pytest.raises(ValueError, match="program worktree must be committed"):
        plan(case.recipe, case.decisions)


def test_verified_renewal_allows_next_append_but_does_not_approve_new_jp_scope(
    case: Case, inputs: Inputs
) -> None:
    write(case.authored, plan(case.recipe, case.decisions))
    verified = load_renewed_registry(case.authored, case.recipe.evidence())
    original = {
        entry.record_key: entry
        for shard in verified.files.shards
        for entry in shard.envelope().records
    }
    inputs.jp["PR-002"] = inputs.jp["PR-001"].model_copy(
        update={
            "number": "PR-002",
            "faces": [
                face.model_copy(update={"name": "New synthetic card"})
                for face in inputs.jp["PR-001"].faces
            ],
        }
    )
    entries = build(inputs, original)
    planned = plan_files(case.authored, entries, reviewed=verified)
    assert planned
    write_files(planned)
    commit(case.recipe.repo)
    renewed = load_renewed_registry(case.authored, case.recipe.evidence())
    provider = FrozenRegions(
        jp=case.provider.jp,
        en=case.provider.en,
        review_inputs=RegistryReviewInputs(case.recipe.registry_review_inputs),
    )
    projected = plan_preview(
        case.authored, provider, regions=("en", "jp"), snapshot=renewed
    )
    assert projected.uncovered_jp_card_numbers == ("PR-002",)
    assert not projected.included("region_mapping_review")
