"""Exact companion selection and leaf substitution; no execution admission."""

from copy import deepcopy
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Literal

from pydantic import ConfigDict, JsonValue, TypeAdapter

from sve_carddb.contracts.author_evidence import Ports, SourceUse
from sve_carddb.contracts.author_leaves import ParamDeclarations, ParamValues
from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.domains.effects.author_bindings import (
    analyze_bindings,
    validate_author_ports,
)
from sve_carddb.domains.effects.author_inputs import AuthorSchemas
from sve_carddb.domains.effects.author_sites import (
    SITE_RULES,
    NodeSites,
    ParameterSite,
    site_rule,
)
from sve_carddb.domains.effects.author_sources import (
    ADAPTER_RECIPE,
    entry_payload,
    parameter_expected,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_damage import KEY as DAMAGE_KEY
from sve_carddb.domains.translations.four_layer_destroy import KEY as DESTROY_KEY
from sve_carddb.domains.translations.four_layer_fixed_actions import NUMERIC, SEARCH

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator, Mapping
    from pathlib import Path

    from sve_carddb.domains.effects.author_sources import SourceCandidate, SourceContext
    from sve_carddb.domains.effects.author_validation import PathParts

_DECLARATIONS = TypeAdapter(
    ParamDeclarations, config=ConfigDict(strict=True, regex_engine="python-re")
)
_VALUES = TypeAdapter(
    ParamValues, config=ConfigDict(strict=True, regex_engine="python-re")
)
_BODY_FIELDS = ("dsl", "ability_scope", "params", "ports", "body")


class ExpansionError(ValueError):
    def __init__(
        self, reason: str, path: PathParts = (), source: SourceUse | None = None
    ) -> None:
        self.reason = reason
        self.path = path
        self.source = source
        super().__init__(reason)


@dataclass(frozen=True)
class BodyRecord:
    tree: dict[str, JsonValue]
    payload: bytes
    version: str
    input_hash: str


@dataclass(frozen=True)
class ExpansionEvidence:
    path: PathParts
    input_hash: str
    body_version: str | None
    body_payload: bytes | None
    body_input_hash: str | None
    registration: str | None
    domain_hash: str | None
    frame_id: str | None
    interface_key: str | None
    expansion_recipe: str
    adapter_recipe: str
    source_proof: Literal[
        "none", "derived_self_consistent", "current_source_checked"
    ] = field(default="none", init=False)
    source_input_hash: str | None = None
    current_source: SourceUse | None = None


@dataclass(frozen=True)
class ExpansionCandidate:
    tree: JsonValue
    sidecar: tuple[ExpansionEvidence, ...]


def body_payload(record: dict[str, JsonValue]) -> bytes:
    """Hash only the P1 payload, from original strict-loader JSON, without defaults."""
    return canonical(
        {"recipe": "effect-body-v1", **{key: record[key] for key in _BODY_FIELDS}}
    )


def body_version(record: dict[str, JsonValue]) -> str:
    """Compute effect-body-v1 from an original JSON companion record."""
    return "effect-body-v1:" + digest(body_payload(record))


def equivalent_ability(ability: JsonValue) -> bytes:
    """Remove only top-level use locations; preserve ports and semantic descendants."""
    return canonical(
        {
            key: value
            for key, value in object_value(ability).items()
            if key not in {"line", "section", "sources"}
        }
    )


def ability_nodes(tree: JsonValue) -> Iterator[tuple[PathParts, dict[str, JsonValue]]]:
    """Visit explicit card/face containers without guessing metadata key names."""
    cards = object_value(object_value(tree)["cards"])
    for key, card_value in cards.items():
        card = object_value(card_value)
        for index, ability in enumerate(array(card.get("abilities", []))):
            yield ("cards", key, "abilities", index), object_value(ability)
        for face_id, face_value in object_value(card.get("faces", {})).items():
            face = object_value(face_value)
            for index, ability in enumerate(array(face["abilities"])):
                yield (
                    ("cards", key, "faces", face_id, "abilities", index),
                    object_value(ability),
                )


class BodyLibrary:
    """Reject bad versions, hash collisions and duplicates before exact lookup."""

    def __init__(self, schemas: AuthorSchemas, shards: Iterable[bytes]) -> None:
        self._records: dict[tuple[str, str, str], BodyRecord] = {}
        payloads: dict[str, bytes] = {}
        for raw in shards:
            shard = object_value(schemas.body_tree(raw))
            for value in array(shard["records"]):
                record = object_value(value)
                payload = body_payload(record)
                version = "effect-body-v1:" + digest(payload)
                if version in payloads and payloads[version] != payload:
                    raise ExpansionError("body_hash_collision")
                payloads[version] = payload
                if record["body_version"] != version:
                    raise ExpansionError("body_version_mismatch")
                key = (
                    string(record["frame_id"]),
                    string(record["ability_scope"]),
                    version,
                )
                if key in self._records:
                    raise ExpansionError("duplicate_body")
                self._records[key] = BodyRecord(record, payload, version, digest(raw))

    def select(self, frame_id: str, ability_scope: str, version: str) -> BodyRecord:
        """Select all three exact keys; order and newer versions have no meaning."""
        try:
            return self._records[frame_id, ability_scope, version]
        except KeyError:
            raise ExpansionError("body_not_found") from None


def _registration(
    record: BodyRecord, identifier: str
) -> tuple[dict[str, JsonValue], str]:
    registrations = [
        object_value(value) for value in array(record.tree["registrations"])
    ]
    matches = [value for value in registrations if value["id"] == identifier]
    if len(matches) != 1:
        raise ExpansionError("registration_not_found")
    registration = matches[0]
    domain = object_value(registration["domain"])
    if "jp" not in array(domain["regions"]):
        raise ExpansionError("domain_excludes_jp")
    return registration, digest(
        canonical({"recipe": "effect-domain-v1", "domain": domain})
    )


def _replace(
    tree: JsonValue, sites: tuple[ParameterSite, ...], values: dict[str, JsonValue]
) -> JsonValue:
    result = deepcopy(tree)
    replaced: set[PathParts] = set()
    for site in sites:
        parent = result
        for part in site.path[:-1]:
            parent = (
                object_value(parent)[part]
                if isinstance(part, str)
                else array(parent)[part]
            )
        last = site.path[-1]
        if isinstance(last, str):
            object_value(parent)[last] = values[site.name]
        else:
            array(parent)[last] = values[site.name]
        replaced.add(site.path)
    if replaced != {site.path for site in sites}:
        raise ExpansionError("param_replacement_drift")
    return result


def _substitute(
    record: BodyRecord, params: JsonValue, sites: NodeSites
) -> tuple[JsonValue, tuple[ParameterSite, ...]]:
    declarations = _DECLARATIONS.validate_json(
        canonical(record.tree["params"]), strict=True
    )
    supplied = _VALUES.validate_json(canonical(params), strict=True)
    if declarations.keys() != supplied.keys():
        raise ExpansionError("param_names_mismatch")
    uses = sites.parameters(record.tree["body"])
    if {site.name for site in uses} - declarations.keys():
        raise ExpansionError("undeclared_param")
    if declarations.keys() - {site.name for site in uses}:
        raise ExpansionError("unused_param_declaration")
    for site in uses:
        rule = site_rule(site)
        declaration = declarations[site.name]
        if (
            declaration.type != supplied[site.name].type
            or declaration.type != rule.type
        ):
            raise ExpansionError("param_type_mismatch", site.path)
        if declaration.source.component != rule.component:
            raise ExpansionError("param_component_mismatch", site.path)
    values: dict[str, JsonValue] = {key: value.value for key, value in supplied.items()}
    return _replace(record.tree["body"], uses, values), uses


def _verify_parameters(
    source: SourceContext,
    record: BodyRecord,
    params: JsonValue,
    sites: NodeSites,
    path: PathParts,
) -> None:
    declarations = _DECLARATIONS.validate_json(
        canonical(record.tree["params"]), strict=True
    )
    values = _VALUES.validate_json(canonical(params), strict=True)
    for site in sites.parameters(record.tree["body"]):
        expected = parameter_expected(
            source,
            declarations[site.name],
            site.owner,
            site.relative_path,
            site.fragment,
        )
        if expected != values[site.name].value:
            raise ExpansionError("source_param_value_mismatch", (*path, *site.path))


def _local_bindings(
    ability: dict[str, JsonValue], sites: NodeSites, path: PathParts
) -> None:
    body = {
        key: value
        for key, value in ability.items()
        if key not in {"line", "section", "sources", "imports", "ports"}
    }
    ports = Ports.model_validate_json(canonical(ability["ports"]), strict=True)
    analysis = analyze_bindings(
        sites.decode_head(body),
        sites,
        {local.bind: "Unknown" for local in ports.imports.values()},
    )
    violations = (*analysis.violations, *validate_author_ports(analysis, ports))
    if violations:
        raise ExpansionError(violations[0].reason, (*path, *violations[0].path))


class AuthorExpander:
    """Expand candidates, keeping provenance separate from the D0 JSON tree."""

    def __init__(self, schema_directory: Path) -> None:
        self.schemas = AuthorSchemas(schema_directory)
        self.sites = NodeSites(schema_directory)
        schemas: dict[str, JsonValue] = {
            filename: digest((schema_directory / filename).read_bytes())
            for filename in (
                "author-nodes.schema.json",
                "author.schema.json",
                "effect-bodies.schema.json",
                "effects-index.schema.json",
            )
        }
        rules: list[JsonValue] = [
            {
                "owner": rule.owner,
                "path": list(rule.path),
                "type": rule.type,
                "source_type": rule.source_type,
                "role": rule.role,
                "component": rule.component,
            }
            for rule in SITE_RULES
        ]
        self.expansion_recipe = digest(
            canonical(
                {
                    "recipe": "author-expansion-v1",
                    "schemas": schemas,
                    "sites": rules,
                    "substitution": "typed-leaf-only-v1",
                }
            )
        )
        self.adapter_recipe = digest(
            canonical(
                {
                    "recipe": "author-adapter-v1",
                    "sites": rules,
                    "source_adapter": ADAPTER_RECIPE,
                    "schemas": schemas,
                    "operand": "sealed-offset-only-v1",
                    "ports": "fail-closed-v1",
                }
            )
        )

    def expand(self, raw: bytes, shards: Iterable[bytes]) -> ExpansionCandidate:
        """Pure structural expansion; source_proof remains none for every ability."""
        tree = self.schemas.author_tree(raw)
        library = BodyLibrary(self.schemas, shards)
        sidecar: list[ExpansionEvidence] = []
        for path, ability in ability_nodes(tree):
            evidence = ExpansionEvidence(
                path,
                digest(raw),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                self.expansion_recipe,
                self.adapter_recipe,
            )
            if "generated_by" in ability:
                generated = object_value(ability["generated_by"])
                macro = object_value(generated["macro"])
                scope = string(macro["ability_scope"])
                source = SourceUse.model_validate_json(
                    canonical(array(ability["sources"])[0]), strict=True
                )
                if source.ability_scope != scope:
                    raise ExpansionError("source_scope_mismatch", path)
                record = library.select(
                    string(macro["frame_id"]), scope, string(generated["version"])
                )
                registration, domain_hash = _registration(
                    record, string(macro["applicability_scope"])
                )
                body, _ = _substitute(record, generated["params"], self.sites)
                ability.pop("generated_by")
                ability.update(object_value(body))
                ability["ports"] = deepcopy(record.tree["ports"])
                evidence = replace(
                    evidence,
                    body_version=record.version,
                    body_payload=record.payload,
                    body_input_hash=record.input_hash,
                    registration=string(registration["id"]),
                    domain_hash=domain_hash,
                    frame_id=string(record.tree["frame_id"]),
                )
            ports = object_value(ability["ports"])
            if (
                object_value(ports["imports"]).keys()
                != object_value(ability.get("imports", {})).keys()
            ):
                raise ExpansionError("import_names_mismatch", path)
            sidecar.append(evidence)
        self.schemas.expanded(canonical(tree))
        for path, ability in ability_nodes(tree):
            _local_bindings(ability, self.sites, path)
        return ExpansionCandidate(tree, tuple(sidecar))

    def expand_sources(
        self,
        raw: bytes,
        shards: Iterable[bytes],
        candidates: Mapping[PathParts, SourceCandidate],
    ) -> ExpansionCandidate:
        """Reconstruct supplied sources; this still does not prove current admission."""
        originals = dict(ability_nodes(self.schemas.author_tree(raw)))
        if candidates.keys() != originals.keys():
            raise ExpansionError("source_candidates_mismatch")
        contexts = {
            path: resolve_source(
                candidates[path],
                SourceUse.model_validate_json(
                    canonical(array(ability["sources"])[0]), strict=True
                ),
            )
            for path, ability in originals.items()
        }
        result = self.expand_contexts(raw, shards, contexts)
        sidecar = []
        for evidence in result.sidecar:
            checked = replace(
                evidence,
                source_input_hash=digest(candidates[evidence.path].raw.encode("utf-8")),
            )
            object.__setattr__(checked, "source_proof", "derived_self_consistent")  # ruff: ignore[unnecessary-dunder-call] -- replacing evidence resets proof; only this reconstruction path grants it
            sidecar.append(checked)
        return ExpansionCandidate(result.tree, tuple(sidecar))

    def expand_contexts(
        self,
        raw: bytes,
        shards: Iterable[bytes],
        contexts: Mapping[PathParts, SourceContext],
    ) -> ExpansionCandidate:
        """Expand supplied contexts without granting any source proof."""
        shard_inputs = tuple(shards)
        result = self.expand(raw, shard_inputs)
        expanded = dict(ability_nodes(result.tree))
        originals = dict(ability_nodes(self.schemas.author_tree(raw)))
        if contexts.keys() != originals.keys():
            raise ExpansionError("source_candidates_mismatch")
        library = BodyLibrary(self.schemas, shard_inputs)
        sidecar: list[ExpansionEvidence] = []
        for evidence in result.sidecar:
            ability = originals[evidence.path]
            source = contexts[evidence.path]
            generated_value = ability.get("generated_by")
            if generated_value is not None:
                generated = object_value(generated_value)
                macro = object_value(generated["macro"])
                if source.frame.id != macro["frame_id"]:
                    raise ExpansionError("source_frame_mismatch", evidence.path)
                record = library.select(
                    string(macro["frame_id"]),
                    string(macro["ability_scope"]),
                    string(generated["version"]),
                )
                _verify_parameters(
                    source, record, generated["params"], self.sites, evidence.path
                )
                ports = object_value(record.tree["ports"])
            else:
                ports = object_value(ability["ports"])
            if (
                source.frame.projection.imports
                or source.frame.projection.exports
                or object_value(ports["exports"])
                or object_value(ports["imports"])
                or ability.get("imports")
            ):
                raise ExpansionError("unsupported_port_source_mapping", evidence.path)
            if source.frame.semantic_variant.key in {
                DAMAGE_KEY,
                DESTROY_KEY,
                NUMERIC,
                SEARCH,
                "draw_exact_spell.v1",
                "draw_exact_attack_time.v1",
                "draw_exact_evolve_time.v1",
                "move_self_to_owner_cemetery_activated_pp.v1",
            }:
                use = SourceUse.model_validate_json(
                    canonical(array(ability["sources"])[0]), strict=True
                )
                if equivalent_ability(expanded[evidence.path]) != canonical(
                    entry_payload(source, use.ability_scope)
                ):
                    raise ExpansionError(
                        "source_head_payload_mismatch", evidence.path, use
                    )
            sidecar.append(
                replace(
                    evidence,
                    frame_id=source.frame.id,
                    interface_key=source.frame.projection.interface_key(
                        source.frame.id
                    ),
                )
            )
        return ExpansionCandidate(result.tree, tuple(sidecar))
