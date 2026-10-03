"""Code-owned immutable registry; a new behavior needs a new ID and retained old files."""

REGISTERED = {
    "physical-parser-v1": (
        "carddb/src/sve_carddb/template_semantics/manifests/physical-parser-v1.json",
        "sha256:24f2f1e7bf9539314bfdc340922a9262c0d0796775ac3af43dfdc58e4323c982",
    ),
    "template-effect-v1": (
        "carddb/src/sve_carddb/template_semantics/manifests/template-effect-v1.json",
        "sha256:7ffe00a9b92baad134ff6ac9d83277451ec6ee1f328586bd5ac93eb058a91fd4",
    ),
    "template-flavor-v1": (
        "carddb/src/sve_carddb/template_semantics/manifests/template-flavor-v1.json",
        "sha256:e1f568f1b3e0058031f012910b91edf9d7d96e19bb398bdbcfb00e3c75632be1",
    ),
    "semantic-output-v1": (
        "carddb/src/sve_carddb/template_semantics/manifests/semantic-output-v1.json",
        "sha256:a2a768fbf35d04245752399a7aa4b42c65208b9a8044cedf4f6f3e4bc3853be5",
    ),
}

RECIPES = {
    "translation-jp-v1": "carddb/src/sve_carddb/template_semantics/v1/projection.py",
    "classification-jp-v0-v1": "carddb/src/sve_carddb/template_sources/normalizer.py",
    "flavor-exact-v1": "carddb/src/sve_carddb/template_sources/flavor.py",
    "template-parameters-jp-candidate-v1": "carddb/src/sve_carddb/template_semantics/v1/candidates.py",
}

# These originals remain pinned by policies; execution uses fixed copies, not these imports.
METADATA_ONLY = frozenset(
    {
        "carddb/src/sve_carddb/template_parameters/references.py",
        "carddb/src/sve_carddb/template_parameters/analysis.py",
        "carddb/src/sve_carddb/template_parameters/numeric_rules.py",
        "carddb/src/sve_carddb/template_parameters/models.py",
        "carddb/src/sve_carddb/template_parameters/rule_candidates.py",
        "carddb/src/sve_carddb/template_parameters/candidate_matching.py",
        "carddb/src/sve_carddb/template_parameters/provenance.py",
    }
)

# Only explicit typed-model and reviewed I/O boundaries may evolve outside semantic files.
BOUNDARIES = frozenset(
    {
        "sve_carddb.text_observations.vocabulary",
        "sve_carddb.catalog.adoption_models",
        "sve_carddb.catalog.adoption_sources",
        "sve_carddb.template_semantics.registry",
        "sve_carddb.template_translations.flavor_models",
        "sve_carddb.template_translations.replay_models",
        "sve_carddb.text_observations.models",
        "sve_carddb.registry.records",
        "sve_carddb.template_parameter_rules.models",
        "sve_carddb.text_observations.presence",
        "sve_carddb.frozen_sources",
        "sve_carddb.template_parameter_rules.loader",
        "sve_carddb.translations.loader",
        "sve_carddb.template_translations.sources",
        "sve_carddb.build_inputs",
        "sve_carddb.text_observations.archive",
        "sve_carddb.translations.name_replay",
        "sve_carddb.translations.sources",
        "sve_carddb.template_sources.pins",
        "sve_carddb.template_parameter_rules.repository",
        "sve_carddb.template_parameters.models",
        "sve_carddb.translations.models",
        "sve_carddb.template_sources.models",
    }
)
