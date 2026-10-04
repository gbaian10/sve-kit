"""Complete name override evidence and current-owner applicability, without rendering."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed immutable Git tree enumeration, never a shell
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Literal

from sve_carddb.build_inputs import SourceUse, uses_sorted
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import read_yaml
from sve_carddb.registry.transitions.files import read_transition_files
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.sources import official_en, official_jp
from sve_carddb.text_observations.archive import FrozenTexts
from sve_carddb.text_observations.models import FaceObservation, candidate_revision_id
from sve_carddb.translations.loader import record_hash
from sve_carddb.translations.models import (
    AssignmentRecord,
    ConceptRecord,
    IdentityBasis,
    PrintingOwner,
    TermRecord,
)
from sve_carddb.translations.name_build import NameOwner, NameSource, name_source

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import Source
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.translations.importer import Inputs
    from sve_carddb.translations.loader import Snapshot
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class ResolvedName:
    source: NameSource
    term_id: str | None
    variant: str
    reason: Literal["selected", "missing_name_concept", "ambiguous_name_concept"]
    record_hashes: tuple[str, ...]
    decision_ids: tuple[str, ...]


@dataclass(frozen=True)
class NameReplay:
    snapshot: Snapshot
    originals: tuple[tuple[str, str], ...]
    uses: tuple[SourceUse, ...]
    assignment_languages: tuple[tuple[str, str], ...] = ()
    assignment_identities: tuple[tuple[str, str, str], ...] = ()

    def resolve(self, db: Database, owner: NameOwner) -> ResolvedName | None:
        """Recheck every owner; matching another owner never grants its eligibility."""
        source = name_source(db, owner)
        if source is None:
            return None
        identities = {
            key: (card, face) for key, card, face in self.assignment_identities
        }
        assignments = [
            (r, d)
            for r, d in self.snapshot.effective()
            if isinstance(r, AssignmentRecord)
            and r.data.owner.model_dump(mode="json") == owner.payload()
            and r.data.source_hash == source.source_hash
            and identities.get(r.record_key) == (source.card_id, source.face_id)
            and dict(self.assignment_languages).get(r.record_key) == source.lang
        ]
        associations = [
            (r, d)
            for r, d in self.snapshot.effective()
            if isinstance(r, ConceptRecord)
            and r.data.term_id is not None
            and r.data.subject.model_dump(mode="json")
            == {
                "card_id": source.card_id,
                "face_id": source.face_id,
                "source_lang": source.lang,
                "source_hash": source.source_hash,
            }
        ]
        candidates = tuple(
            identifier
            for identifier, text in self.originals
            if source.lang == "ja" and text == source.text
        )
        hashes: list[str] = []
        decisions: list[str] = []
        variant = "default"
        if associations:
            record, decision = associations[0]
            candidates = (str(record.data.term_id),)
            hashes.append(record_hash(record))
            decisions.append(decision)
        if assignments:
            assignment, decision = assignments[0]
            selected = "term:" + assignment.data.concept_key
            if associations and selected not in candidates:
                raise ValueError("Name assignment contradicts its concept association")
            if not associations and selected not in candidates:
                raise ValueError(
                    "Name assignment differs from exact adopted name concepts"
                )
            for other, _ in self.snapshot.effective():
                if (
                    isinstance(other, AssignmentRecord)
                    and dict(self.assignment_languages).get(other.record_key)
                    == source.lang
                    and other.data.source_hash == source.source_hash
                    and other.data.variant == assignment.data.variant
                    and other.data.concept_key != assignment.data.concept_key
                ):
                    raise ValueError(
                        "Name semantic variant selects conflicting concepts"
                    )
            candidates = (selected,)
            variant = assignment.data.variant
            hashes.append(record_hash(assignment))
            decisions.append(decision)
        # A permanent association does not itself allocate a semantic variant.
        if len(dict(self.originals).keys() & set(candidates)) != 1 or (
            len(
                [
                    text
                    for _, text in self.originals
                    if source.lang == "ja" and text == source.text
                ]
            )
            > 1
            and variant == "default"
        ):
            reason: Literal[
                "selected", "missing_name_concept", "ambiguous_name_concept"
            ] = "ambiguous_name_concept" if candidates else "missing_name_concept"
            return ResolvedName(
                source, None, variant, reason, tuple(hashes), tuple(decisions)
            )
        return ResolvedName(
            source, candidates[0], variant, "selected", tuple(hashes), tuple(decisions)
        )


class IdentityEvidence:
    def __init__(self, sources: Sources, authored_revision: str) -> None:
        if re.fullmatch(r"[0-9a-f]{40}", authored_revision) is None:
            raise ValueError("Name identity consumer revision must be a full Git SHA")
        self.sources = sources
        self.authored_revision = authored_revision
        self.cache: dict[bytes, RegistrySnapshot] = {}
        self.providers: dict[tuple[str, str, str], FrozenTexts] = {}
        self.uses: list[SourceUse] = []
        self.authored_uses: list[tuple[str, str, str]] = []
        self.record_revisions: dict[str, str] = {}
        self.checked: set[bytes] = set()
        self.observations: dict[bytes, tuple[Source, ...]] = {}
        self.assignment_identities: dict[str, tuple[str, str]] = {}
        self.observation_cache_hits = 0
        self.parsed_versions = 0

    def costs(self) -> dict[str, int]:
        """Expose actual work counts; identical inputs do not predict changed-input cost."""
        return {
            "registry_bases": len(self.cache),
            "observation_keys": len(self.observations),
            "observation_cache_hits": self.observation_cache_hits,
            "parsed_versions": self.parsed_versions,
            "basis_observation_uses": len(self.uses),
        }

    def registry(self, basis: IdentityBasis) -> RegistrySnapshot:
        """An absent transition index must be absent in the immutable tree, not disk."""
        key = canonical(basis.model_dump(mode="json"))
        if key in self.cache:
            return self.cache[key]
        repository = self.sources.repository
        result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- full SHA and fixed tree roots are closed inputs
            [
                repository.executable,
                "-C",
                str(repository.root),
                "ls-tree",
                "-r",
                "--format=%(objectmode)%x09%(path)",
                "-z",
                basis.authored_revision,
                "--",
                "authored/ids",
                "authored/registry",
                "authored/identity-transitions",
            ],
            check=False,
            capture_output=True,
        )
        if result.returncode:
            raise ValueError("Name identity immutable tree is unavailable")
        entries = [
            entry.split("\t", 1)
            for entry in result.stdout.decode().split("\0")
            if entry
        ]
        if any(mode not in {"100644", "100755"} for mode, _ in entries):
            raise ValueError("Name identity immutable tree contains a nonregular input")
        if not _ancestor(
            self.sources.repository, basis.authored_revision, self.authored_revision
        ):
            raise ValueError("Name identity basis is not a consumer ancestor")
        names = tuple(name for _, name in entries)
        files = repository.read_many(basis.authored_revision, names)
        with tempfile.TemporaryDirectory(prefix="name-identity-") as folder:
            root = Path(folder)
            for name, raw in files.items():
                target = root / name.removeprefix("authored/")
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            index = root / "ids/index.yaml"
            if (
                not index.is_file()
                or digest(canonical(read_yaml(index))) != basis.registry_index_hash
            ):
                raise ValueError("Name identity registry index hash mismatch")
            transitions = read_transition_files(root)
            checksum = (
                None
                if transitions.index_content is None
                else digest(transitions.index_content)
            )
            if checksum != basis.transition_index_hash:
                raise ValueError("Name identity transition index pin mismatch")
            if transitions.shards:
                raise ValueError(
                    "Name identity transitions require complete effective evidence replay"
                )
            registry = load_registry(root)
        for path, raw in files.items():
            # Authored evidence pins are included separately by the importer audit.
            self.authored_uses.append((basis.authored_revision, path, digest(raw)))
        self.cache[key] = registry
        return registry

    def complete(
        self, basis: IdentityBasis, batches: tuple[tuple[str, str], ...]
    ) -> None:
        """Verify every base printing observation, including unrelated source claims."""
        key = canonical(
            [
                basis.model_dump(mode="json"),
                [[store, batch] for store, batch in batches],
            ]
        )
        if key in self.checked:
            return
        registry = self.registry(basis)
        for record in registry.records.values():
            if not isinstance(record.data, PrintingData):
                continue
            printing = record.data
            matched = self._observations(printing, batches)
            # Reuse parsing, never the per-basis audit or the owner's source-face check.
            for source in matched:
                self.uses.append(
                    SourceUse(
                        source=source,
                        usage="name_identity_observation",
                        locator=canonical(
                            [basis.authored_revision, printing.id]
                        ).decode(),
                    )
                )
        self.checked.add(key)

    def _observations(
        self, printing: PrintingData, batches: tuple[tuple[str, str], ...]
    ) -> tuple[Source, ...]:
        provider = official_jp if printing.region == "jp" else official_en
        parser = "translation-" + printing.region + "-v1"
        config = object_value(parse(self.sources.build.configuration.encode()))
        recipe = object_value(config.get("translation_recipes")).get(parser)
        observation_key = canonical(
            [
                printing.observation.model_dump(mode="json"),
                provider.card_url(printing.card_no),
                [[store, batch] for store, batch in batches],
                parser,
                recipe,
            ]
        )
        if observation_key in self.observations:
            self.observation_cache_hits += 1
        if observation_key not in self.observations:
            found: list[Source] = []
            for store, batch in batches:
                lookup = store, batch, printing.region
                if lookup not in self.providers:
                    self.providers[lookup] = FrozenTexts(
                        self.sources.stores[store],
                        store,
                        batch,
                        region=printing.region,
                        parser_version=parser,
                    )
                frozen = self.providers[lookup]
                for version in frozen.versions(printing.region, printing.card_no):
                    self.sources.projection(store, batch, version, parser)
                    card = frozen.version(printing.region, printing.card_no, version)
                    self.parsed_versions += 1
                    if (
                        card.observation == printing.observation
                        and card.source.url == provider.card_url(printing.card_no)
                    ):
                        found.append(card.source)
            if not found:
                raise ValueError(
                    "Name identity complete frozen observation closure is absent"
                )
            self.observations[observation_key] = tuple(found)
        return self.observations[observation_key]

    def association(  # ruff: ignore[complex-structure,too-many-branches,too-many-locals] -- immutable identity, raw face and revision owner guards are independent
        self,
        basis: IdentityBasis,
        ref: SourceRef,
        *,
        card_id: str | None = None,
        face_id: str | None = None,
        owner: NameOwner | None = None,
    ) -> tuple[str, str, str, str]:
        """Bind raw URL, complete observation and physical source index to permanent keys."""
        registry = self.registry(basis)
        lang, text, source = self.sources.text(ref)
        expected_region = "jp" if lang == "ja" else "en"
        if ref.parser != "translation-" + expected_region + "-v1" or lang not in {
            "ja",
            "en",
        }:
            raise ValueError("Name override must locate a physical name source")
        mappings: list[tuple[PrintingData, int, str]] = []
        for record in registry.records.values():
            if not isinstance(record.data, PrintingData):
                continue
            printing = record.data
            provider = official_jp if printing.region == "jp" else official_en
            if (
                printing.region != expected_region
                or provider.card_url(printing.card_no) != source.url
            ):
                continue
            mappings.extend(
                (printing, mapping.source_index, mapping.face_id)
                for mapping in printing.source_face_map
                if ref.locator == f"/faces/{mapping.source_index}/name"
            )
        if len(mappings) != 1:
            raise ValueError(
                "Name override frozen source has no unique physical face mapping"
            )
        printing, index, face = mappings[0]
        if (card_id is not None and printing.card_id != card_id) or (
            face_id is not None and face != face_id
        ):
            raise ValueError(
                "Name override frozen evidence belongs to another card or face"
            )
        cards = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, CardData)
        }
        faces = {
            r.data.id: r.data
            for r in registry.records.values()
            if isinstance(r.data, FaceData)
        }
        if (
            cards[printing.card_id].identity_state != "confirmed"
            or faces[face].card_id != printing.card_id
        ):
            raise ValueError("Name override requires confirmed physical identity")
        key = ref.store_id, ref.batch_id, printing.region
        if key not in self.providers:
            self.providers[key] = FrozenTexts(
                self.sources.stores[ref.store_id],
                ref.store_id,
                ref.batch_id,
                region=printing.region,
                parser_version=ref.parser,
            )
        frozen = self.providers[key].version(
            printing.region, printing.card_no, ref.source_version_id
        )
        if frozen.observation != printing.observation:
            raise ValueError(
                "Name override physical observation differs from identity basis"
            )
        content = frozen.projected(index)
        if owner is not None:
            if owner.kind == "printing_face":
                if (owner.identifier, owner.face_id) != (printing.id, face):
                    raise ValueError(
                        "Name assignment evidence belongs to another printing owner"
                    )
            else:
                item = FaceObservation(
                    card_id=printing.card_id,
                    printing_id=printing.id,
                    face_id=face,
                    region=printing.region,
                    card_no=printing.card_no,
                    source_index=index,
                    card=frozen,
                    content=content,
                )
                if candidate_revision_id(item) != owner.identifier:
                    raise ValueError(
                        "Name assignment frozen evidence does not reproduce revision owner"
                    )
        self.uses.append(
            SourceUse(
                source=source,
                usage="name_identity",
                locator=canonical([printing.id, face, ref.locator]).decode(),
            )
        )
        return lang, text, printing.card_id, face


def _ancestor(repository: PinnedRepository, earlier: str, later: str) -> bool:
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- immutable SHA arguments, no ref or network lookup
        [
            repository.executable,
            "-C",
            str(repository.root),
            "merge-base",
            "--is-ancestor",
            earlier,
            later,
        ],
        check=False,
        capture_output=True,
    )
    if result.returncode not in {0, 1}:
        raise ValueError("Name identity Git ancestry is unavailable")
    if result.returncode == 1:
        shallow = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- inspect local history completeness, never fetch
            [
                repository.executable,
                "-C",
                str(repository.root),
                "rev-parse",
                "--is-shallow-repository",
            ],
            check=False,
            capture_output=True,
        )
        # A shallow boundary can hide an actual ancestor even when both commits exist.
        if shallow.returncode or shallow.stdout.strip() != b"false":
            raise ValueError("Name identity Git ancestry is unavailable")
    return result.returncode == 0


def verify_name_adoption_base(inputs: Inputs, base_revision: str) -> None:
    """Adoption callers supply the trusted PR base; the loader never guesses main."""
    if re.fullmatch(r"[0-9a-f]{40}", base_revision) is None:
        raise ValueError("Name adoption base revision must be a full Git SHA")
    snapshot = inputs.load()
    repository = PinnedRepository(inputs.repository)
    _ancestor(repository, base_revision, base_revision)
    for record, _ in snapshot.records():
        if isinstance(record, (AssignmentRecord, ConceptRecord)) and not _ancestor(
            repository, record.data.identity_basis.authored_revision, base_revision
        ):
            raise ValueError("Name adoption basis is outside explicit base history")


def replay_names(  # ruff: ignore[too-many-locals] -- retain full historical evidence and separate current applicability
    snapshot: Snapshot, originals: dict[str, str], inputs: Inputs, sources: Sources
) -> tuple[NameReplay, IdentityEvidence]:
    """Validate even superseded/withdrawn records before retaining effective applicability."""
    evidence = IdentityEvidence(sources, inputs.authored_revision)
    overrides = [
        (r, d)
        for r, d in snapshot.records()
        if isinstance(r, (AssignmentRecord, ConceptRecord))
    ]
    terms = {
        r.data.id: r
        for r, _ in snapshot.records()
        if isinstance(r, TermRecord) and r.data.category == "card_name"
    }
    for record, _ in overrides:
        if isinstance(record, AssignmentRecord) and not any(
            proof.source_ref.text_hash == record.data.source_hash
            for proof in record.evidence
        ):
            raise ValueError("Name assignment requires its exact frozen name evidence")
    batches = {
        (ref.store_id, ref.batch_id)
        for record, _ in overrides
        for ref in (
            [record.data.source_ref]
            if isinstance(record, ConceptRecord)
            else [proof.source_ref for proof in record.evidence]
        )
    }
    config = object_value(parse(sources.build.configuration.encode()))
    offline = config.get("offline_recipe")
    if isinstance(offline, dict):
        for pin in array(offline.get("sources")):
            item = object_value(pin)
            store, batch = offline.get("store_id"), item.get("card_batch")
            if not isinstance(store, str) or not isinstance(batch, str):
                raise ValueError("Name identity offline source batch is malformed")  # ruff: ignore[type-check-without-type-error] -- domain refusal for malformed build configuration
            batches.add((store, batch))
    assignment_languages: dict[str, str] = {}
    for record, _ in overrides:
        basis = record.data.identity_basis
        evidence.complete(basis, tuple(sorted(batches)))
        evidence.record_revisions[record.record_key] = basis.authored_revision
        if isinstance(record, ConceptRecord):
            lang, _, _, _ = evidence.association(
                record.data.identity_basis,
                record.data.source_ref,
                card_id=record.data.subject.card_id,
                face_id=record.data.subject.face_id,
            )
            if lang != record.data.subject.source_lang:
                raise ValueError("Name concept language differs from physical source")
        else:
            wire = record.data.owner
            owner = (
                NameOwner("printing_face", wire.printing_id, wire.face_id)
                if isinstance(wire, PrintingOwner)
                else NameOwner("face_revision", wire.revision_id)
            )
            refs = [
                proof.source_ref
                for proof in record.evidence
                if proof.source_ref.text_hash == record.data.source_hash
            ]
            for ref in refs:
                language, _, card, face = evidence.association(basis, ref, owner=owner)
                evidence.assignment_identities[record.record_key] = card, face
                assignment_languages[record.record_key] = language
    uses = uses_sorted((*sources.uses, *evidence.uses))
    return NameReplay(
        snapshot,
        tuple(sorted((k, v) for k, v in originals.items() if k in terms)),
        uses,
        tuple(sorted(assignment_languages.items())),
        tuple(
            sorted(
                (key, card, face)
                for key, (card, face) in evidence.assignment_identities.items()
            )
        ),
    ), evidence
