"""Verify current permanent identities against each frozen physical source face."""

import re
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fixed immutable Git tree enumeration, never a shell
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.build_inputs import SourceUse
from sve_carddb.registry.records import CardData, FaceData, PrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.registry.storage import read_yaml
from sve_carddb.registry.transitions.files import read_transition_files
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.sources import official_en, official_jp
from sve_carddb.text_observations.archive import FrozenTexts

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.registry.snapshot import RegistrySnapshot
    from sve_carddb.translations.models import IdentityBasis
    from sve_carddb.translations.sources import Sources


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
        if basis.authored_revision != self.authored_revision:
            raise ValueError(
                "Name identity basis differs from current authored revision"
            )
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

    def association(
        self,
        basis: IdentityBasis,
        ref: SourceRef,
        *,
        card_id: str | None = None,
        face_id: str | None = None,
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
        printing, _, face = mappings[0]
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
        self.uses.append(
            SourceUse(
                source=source,
                usage="name_identity",
                locator=canonical([printing.id, face, ref.locator]).decode(),
            )
        )
        return lang, text, printing.card_id, face
