"""Independent P1 evidence examples and required-field matrix."""

from dataclasses import dataclass

from pydantic import JsonValue, TypeAdapter

from sve_carddb.contracts import author_evidence as evidence

from .author_entry_cases import dependencies, meta, review, source_use


@dataclass(frozen=True)
class EvidenceCase:
    name: str
    adapter: TypeAdapter[object]
    value: dict[str, JsonValue]
    required: tuple[str, ...]


def cases() -> tuple[EvidenceCase, ...]:
    actor: dict[str, JsonValue] = {"who": "fixture", "model": None}
    pin: dict[str, JsonValue] = {"id": "fixture", "hash": "sha256:" + "0" * 64}
    imported: dict[str, JsonValue] = {
        "binding_id": "fixture",
        "ability_scope": "ability_0",
        "port": "output",
    }
    local: dict[str, JsonValue] = {"path": [], "bind": "binding"}
    body: dict[str, JsonValue] = {
        "frame_id": "frame:" + "1" * 64,
        "ability_scope": "ability_0",
        "body_version": "effect-body-v1:sha256:" + "2" * 64,
        "applicability_scope": "fixture",
    }
    reviewed = source_use() | {"source": "effect-source-v1:sha256:" + "3" * 64}
    body_dependencies = body | {
        "domain_hash": "sha256:" + "4" * 64,
        "interfaces": [],
        "sources": [reviewed],
        "rulings": [],
        "expansion_recipe": pin,
        "adapter_recipe": pin,
    }
    original: dict[str, JsonValue] = {
        "input_hash": "sha256:" + "0" * 64,
        "record_key": "fixture",
    }
    body_rekey: dict[str, JsonValue] = {
        "original": original,
        "before": body_dependencies,
        "after": body_dependencies,
        "mapping_recipe": pin,
    }
    use_rekey: dict[str, JsonValue] = {
        "original": original,
        "before": dependencies(),
        "after": dependencies(),
        "mapping_recipe": pin,
    }
    body_review = review() | {"kind": "body", "dependencies": body_dependencies}
    macro = dependencies() | {
        "applicability_scope": "fixture",
        "domain_hash": "sha256:" + "4" * 64,
        "params": {"offset": {"type": "Int", "value": 5}},
    }
    use_fields = (
        "sources",
        "frame_id",
        "ability_scope",
        "semantic_variant",
        "body_version",
        "interfaces",
        "source",
        "rulings",
        "expansion_recipe",
        "adapter_recipe",
        "imports",
        "applicability_scope",
        "domain_hash",
        "params",
    )
    review_fields = ("kind", "reviewer", "date", "verdict", "reason", "dependencies")
    body_fields = ("frame_id", "ability_scope", "body_version", "applicability_scope")
    return (
        EvidenceCase("Actor", TypeAdapter(evidence.Actor), actor, ("who", "model")),
        EvidenceCase(
            "WrittenBy",
            TypeAdapter(evidence.WrittenBy),
            {"actor": actor, "date": "2026-10-21"},
            ("actor", "date"),
        ),
        EvidenceCase(
            "Domain", TypeAdapter(evidence.Domain), {"regions": ["jp"]}, ("regions",)
        ),
        EvidenceCase("RecipePin", TypeAdapter(evidence.RecipePin), pin, ("id", "hash")),
        EvidenceCase(
            "InterfaceDependency",
            TypeAdapter(evidence.InterfaceDependency),
            {"frame_id": "frame:" + "1" * 64, "interface_key": "2" * 64},
            ("frame_id", "interface_key"),
        ),
        EvidenceCase(
            "RulingDependency",
            TypeAdapter(evidence.RulingDependency),
            {"id": "R-0001", "revision": 1},
            ("id", "revision"),
        ),
        EvidenceCase(
            "ImportUse",
            TypeAdapter(evidence.ImportUse),
            imported,
            ("binding_id", "ability_scope", "port"),
        ),
        EvidenceCase(
            "ImportedDependency",
            TypeAdapter(evidence.ImportedDependency),
            imported | {"dependency_key": "sha256:" + "0" * 64},
            ("binding_id", "ability_scope", "port", "dependency_key"),
        ),
        EvidenceCase(
            "LocalBinding", TypeAdapter(evidence.LocalBinding), local, ("path", "bind")
        ),
        EvidenceCase(
            "ImportBinding",
            TypeAdapter(evidence.ImportBinding),
            local,
            ("path", "bind"),
        ),
        EvidenceCase(
            "Ports",
            TypeAdapter(evidence.Ports),
            {"imports": {}, "exports": {}},
            ("imports", "exports"),
        ),
        EvidenceCase(
            "SourceUse",
            TypeAdapter(evidence.SourceUse),
            source_use(),
            ("binding_id", "occurrence", "ability_scope"),
        ),
        EvidenceCase(
            "ResolvedVariant",
            TypeAdapter(evidence.ResolvedVariant),
            {"state": "resolved", "key": "fixture", "scope": None},
            ("state", "key", "scope"),
        ),
        EvidenceCase(
            "InlineDependencies",
            TypeAdapter(evidence.InlineDependencies),
            dependencies(),
            use_fields,
        ),
        EvidenceCase(
            "MacroDependencies",
            TypeAdapter(evidence.MacroDependencies),
            macro,
            use_fields,
        ),
        EvidenceCase(
            "ReviewedSource",
            TypeAdapter(evidence.ReviewedSource),
            reviewed,
            ("binding_id", "occurrence", "ability_scope", "source"),
        ),
        EvidenceCase(
            "BodyDependencies",
            TypeAdapter(evidence.BodyDependencies),
            body_dependencies,
            (
                *body_fields,
                "domain_hash",
                "interfaces",
                "sources",
                "rulings",
                "expansion_recipe",
                "adapter_recipe",
            ),
        ),
        EvidenceCase(
            "OriginalEvidence",
            TypeAdapter(evidence.OriginalEvidence),
            original,
            ("input_hash", "record_key"),
        ),
        EvidenceCase(
            "BodyRekey",
            TypeAdapter(evidence.BodyRekey),
            body_rekey,
            ("original", "before", "after", "mapping_recipe"),
        ),
        EvidenceCase(
            "UseRekey",
            TypeAdapter(evidence.UseRekey),
            use_rekey,
            ("original", "before", "after", "mapping_recipe"),
        ),
        EvidenceCase(
            "BodyReview", TypeAdapter(evidence.BodyReview), body_review, review_fields
        ),
        EvidenceCase(
            "UseReview", TypeAdapter(evidence.UseReview), review(), review_fields
        ),
        EvidenceCase(
            "BodyReference", TypeAdapter(evidence.BodyReference), body, body_fields
        ),
        EvidenceCase(
            "MacroMechanical",
            TypeAdapter(evidence.MacroMechanical),
            {"kind": "macro_mechanical", "body": body, "use_dependencies": macro},
            ("kind", "body", "use_dependencies"),
        ),
        EvidenceCase(
            "Registration",
            TypeAdapter(evidence.Registration),
            {"id": "fixture", "domain": {"regions": ["jp"]}, "reviews": [body_review]},
            ("id", "domain", "reviews"),
        ),
        EvidenceCase(
            "Meta",
            TypeAdapter(evidence.Meta),
            meta(),
            (
                "dsl",
                "source",
                "written_by",
                "reviewed_by",
                "status",
                "verified_by_exam",
                "rulings",
            ),
        ),
    )
