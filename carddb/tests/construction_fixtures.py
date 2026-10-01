"""Tiny construction graphs and frozen-source metadata; all text is synthetic."""

from sve_carddb.build_inputs import BuildContext, SourceUse
from sve_carddb.construction import (
    Clause,
    Construction,
    Coverage,
    CRVersion,
    Member,
    Profile,
    ProfileRevision,
    Restriction,
)
from sve_carddb.products.models import LocalizedText

from .card_extras_fixtures import REVISION, source


def evidence(locator: str = "synthetic:rules", *, hour: int = 0) -> SourceUse:
    return SourceUse(
        source=source(hour=hour).model_copy(
            update={"url": "https://example.invalid/rules"}
        ),
        usage="synthetic_construction",
        locator=locator,
    )


def plan() -> Construction:
    use = evidence()
    return Construction(
        profiles=(
            Profile(
                id="profile",
                region="jp",
                format_code="synthetic",
                name=LocalizedText(lang="ja", text="Synthetic format"),
                evidence=use,
            ),
        ),
        revisions=(
            ProfileRevision(
                id="profile_revision",
                profile_id="profile",
                effective_from="2026-09-01",
                effective_until=None,
                cr_version_id="cr",
                default_copy_limit=3,
                construction_rules_ref="synthetic-construction-v1",
                evidence=use,
            ),
        ),
        restrictions=(
            Restriction(
                id="limit",
                profile_id="profile",
                announced_on=None,
                effective_from="2026-09-01",
                effective_until=None,
                kind="copy_limit",
                state="confirmed",
                max_copies=0,
                max_selected_groups=None,
                decision_id=None,
                evidence=use,
                members=(
                    Member(
                        rules_name_id="rules_name", choice_option=0, deck_scope="all"
                    ),
                ),
            ),
            Restriction(
                id="choice",
                profile_id="profile",
                announced_on="2026-08-31",
                effective_from="2026-09-01",
                effective_until=None,
                kind="choice_group",
                state="confirmed",
                max_copies=None,
                max_selected_groups=1,
                decision_id=None,
                evidence=use,
                members=(
                    Member(
                        rules_name_id="rules_name", choice_option=0, deck_scope="main"
                    ),
                    Member(
                        rules_name_id="rules_name2", choice_option=1, deck_scope="main"
                    ),
                ),
            ),
        ),
        coverage=(
            Coverage(
                profile_id="profile",
                from_date="2026-09-01",
                until_date="2026-10-03",
                state="complete",
                evidence=use,
            ),
        ),
        cr_versions=(
            CRVersion(
                id="cr",
                region="jp",
                version="synthetic-1",
                published_on="2026-08-31",
                effective_on="2026-09-01",
                evidence=use,
                clauses=(
                    Clause(
                        id="clause",
                        number="9000.1",
                        locator="synthetic:clause",
                        text=LocalizedText(
                            lang="ja", text="Synthetic construction clause."
                        ),
                    ),
                ),
            ),
        ),
    )


def context(staging: Construction) -> BuildContext:
    return BuildContext.from_inputs(
        REVISION, {"synthetic.lock": b"synthetic"}, staging.configuration()
    )
