"""Text-free private diagnostics; plans are not database applications or publications."""

from collections import Counter
from dataclasses import replace
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import input_record
from sve_carddb.digital_links.candidates import sve_inventory
from sve_carddb.digital_links.catalogue import complete_inventory
from sve_carddb.digital_links.evidence import Evidence
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_name_policies.current_evaluate import (
    catalogue as current_catalogue,
)
from sve_carddb.digital_name_policies.evaluate import (
    FrozenName,
    NameOwner,
    catalogue,
    historical_sources,
    name_result,
    owner_text,
    rule_links,
)
from sve_carddb.digital_name_policies.runtime import require_runtime
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.digital_name_policies.current_evaluate import (
        Catalogue as CurrentCatalogue,
    )
    from sve_carddb.digital_name_policies.evaluate import Catalogue, OwnerEvidence
    from sve_carddb.digital_name_policies.loader import Snapshot
    from sve_carddb.translations.sources import Sources

RECIPE = "digital-name-policy-report-v1"


def _baseline(content: bytes | None) -> frozenset[str]:
    if content is None:
        return frozenset()
    raw = object_value(parse(content))
    checksum = raw.pop("report_hash", None)
    if raw.get("recipe") != RECIPE or checksum != digest(canonical(raw)):
        raise ValueError("Digital-name report baseline recipe or hash mismatch")
    members = array(raw.get("owners"))
    keys = [object_value(row).get("owner_key") for row in members]
    if any(not isinstance(key, str) for key in keys) or len(set(keys)) != len(keys):
        raise ValueError("Digital-name report baseline owner keys are invalid")
    return frozenset(str(k) for k in keys)


def _configuration(sources: Sources) -> None:
    raw = object_value(parse(sources.build.configuration.encode()))
    if set(raw) != {"catalog_registry", "digital_link_sources", "translation_recipes"}:
        raise ValueError(
            "Digital-name report context must contain only source configuration"
        )


def generate(  # ruff: ignore[too-many-locals] -- diagnostic report retains independent policies, owners and optional comparison evidence
    snapshot: Snapshot,
    sources: Sources,
    baseline: bytes | None,
    comparison: Sources | None = None,
) -> dict[str, JsonValue]:
    """Use exact frozen sources, with explicit empty or previously sealed report baseline."""
    require_runtime(sources)
    _configuration(sources)
    previous = _baseline(baseline)
    review = review_context(sources)
    index = Evidence(sources).index(review)
    physical = sve_inventory(sources, review)
    names: Catalogue | CurrentCatalogue
    if snapshot.current_names:
        historic = sources
        names = current_catalogue(snapshot.current_names[0], sources)
    else:
        names_policy = snapshot.effective("names")
        historic = historical_sources(
            names_policy, sources.stores, sources.repository.root
        )
        names = catalogue(names_policy, historic)
    links_policy = snapshot.effective("links")
    links_sources = historical_sources(
        links_policy, sources.stores, sources.repository.root
    )
    links = catalogue(links_policy, links_sources)
    owners = []
    seen = set()
    rows: list[JsonValue] = []
    counts: Counter[str] = Counter()
    games: Counter[str] = Counter()
    for number, entries in sorted(physical.items()):
        for sve, _ in entries:
            printing = index.printings[sve.printing_id]
            owner = NameOwner(
                kind="face_revision",
                owner_id="diagnostic:"
                + digest(canonical(sve.model_dump(mode="json")))[7:],
                card_id=printing.card_id,
                face_id=sve.face_id,
                printing_id=sve.printing_id,
                state="known",
                name_ref=sve.name_ref,
            )
            if owner in seen:
                continue
            seen.add(owner)
            evidence = owner_text(owner, sources, review)
            owners.append(evidence)
            result = name_result(evidence, names)
            key = digest(canonical(owner.model_dump(mode="json")))
            counts[result.status] += 1
            if result.game:
                games[result.game] += 1
            rows.append(
                {
                    "owner_key": key,
                    "owner": owner.model_dump(mode="json"),
                    "card_no": number,
                    "source_name_hash": result.source_name_hash,
                    "status": result.status,
                    "condition": result.condition,
                    "selected_game": result.game,
                    "policy_hash": result.policy_hash,
                    "context_hash": result.context_hash,
                    "name_evidence_refs": [
                        r.model_dump(mode="json") for r in result.refs
                    ],
                    "target_name_hash": digest(result.text.encode())
                    if result.text is not None
                    else None,
                    "new_owner": key not in previous,
                }
            )
    plans = rule_links(tuple(owners), links)
    targets: list[JsonValue] = [
        {
            "subject": p.subject(),
            "owners": [o.owner_id for o in p.owners],
            "refs": [r.model_dump(mode="json") for r in p.refs],
            "policy_hash": p.policy_hash,
        }
        for p in plans
    ]
    new_catalogue = (
        None if comparison is None else _comparison(names, owners, comparison)
    )
    link_games = Counter(p.game for p in plans)
    multiplicity = Counter((p.card_id, p.game) for p in plans)
    report: dict[str, JsonValue] = {
        "recipe": RECIPE,
        "scope": "frozen_source_diagnostic_not_publication",
        "policy_inputs": snapshot.pins(),
        "baseline_hash": None if baseline is None else digest(baseline),
        "baseline": "empty_first_run" if baseline is None else "previous_report",
        "summary": {
            "policies": len(snapshot.policies) + len(snapshot.current_names),
            "owners_eligible": counts["eligible"],
            "owners_untranslated": counts["untranslated"],
            "owners_excluded": counts["excluded"],
            "owners_unknown": counts["unknown"],
            "owners_human_first": 0,
            "rule_link_plans": len(plans),
            "by_game": {
                g: {"owners_eligible": games[g], "rule_link_plans": link_games[g]}
                for g in ("sv1", "svwb")
            },
            "new_owners": sum(1 for row in rows if object_value(row)["new_owner"]),
            "multi_target_card_games": sum(1 for n in multiplicity.values() if n > 1),
            "historical_warnings_unavailable": True,
            "human_precedence": "not_applied_diagnostic_plans_only",
            "published_links": 0,
            "published_translations": 0,
            "coverage_adopted": False,
        },
        "owners": rows,
        "rule_link_plans": targets,
        "owner_input_record": parse(
            input_record(sources.build, sources.uses).content()
        ),
        "policy_catalogue_input_records": [
            parse(input_record(s.build, s.uses).content())
            for s in (historic,)
            if s is links_sources
        ]
        if historic is links_sources
        else [
            parse(input_record(s.build, s.uses).content())
            for s in (historic, links_sources)
        ],
        "newer_catalogue": new_catalogue,
    }
    report["report_hash"] = digest(canonical(report))
    return report


def _comparison(
    names: Catalogue | CurrentCatalogue,
    owners: list[OwnerEvidence],
    comparison: Sources,
) -> dict[str, JsonValue]:
    """Diagnose a newer catalogue without certifying continuation or replacing pins."""
    require_runtime(comparison)
    _configuration(comparison)
    comparing = review_context(comparison)
    differences: list[JsonValue] = []
    compared_names: list[FrozenName] = []
    for game in ("sv1", "svwb"):
        complete = complete_inventory(comparison, comparing, game)
        compared_names.extend(
            FrozenName(n.game, n.official_id, n.phase, n.lang, n.text, n.ref)
            for n in complete.values()
        )
        old = {
            (n.official_id, n.phase, n.lang): n.text
            for n in names.names
            if n.game == game
        }
        current = {(n.official_id, n.phase, n.lang): n.text for n in complete.values()}
        for target_key in sorted(old.keys() | current.keys()):
            if old.get(target_key) != current.get(target_key):
                differences.append(  # ruff: ignore[manual-list-comprehension] -- absence and changed values have distinct conditions and hashes
                    {
                        "game": game,
                        "official_id": target_key[0],
                        "phase": target_key[1],
                        "lang": target_key[2],
                        "change": "added"
                        if target_key not in old
                        else "removed"
                        if target_key not in current
                        else "changed",
                        "old_name_hash": digest(old[target_key].encode())
                        if target_key in old
                        else None,
                        "new_name_hash": digest(current[target_key].encode())
                        if target_key in current
                        else None,
                    }
                )
    compared = replace(names, names=tuple(compared_names))
    owner_changes: list[JsonValue] = []
    for evidence in owners:
        original, proposed = (
            name_result(evidence, names),
            name_result(evidence, compared),
        )
        if (original.status, original.game, original.text) != (
            proposed.status,
            proposed.game,
            proposed.text,
        ):
            owner_changes.append(
                {
                    "owner_id": evidence.owner.owner_id,
                    "source_name_hash": original.source_name_hash,
                    "old_status": original.status,
                    "new_status": proposed.status,
                    "eligibility_lost": original.status == "eligible"
                    and proposed.status != "eligible",
                    "selected_game_changed": original.game != proposed.game,
                    "translation_changed": original.text != proposed.text,
                }
            )
    return {
        "owner_changes": owner_changes,
        "continuation_proof": "not_certified",
        "activation": "report_only_continuation_format_unsupported",
        "changes": differences,
        "input_record": parse(
            input_record(comparison.build, comparison.uses).content()
        ),
    }
