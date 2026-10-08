"""Strict build-stage records matching the existing construction tables."""

from typing import Literal

from pydantic import JsonValue, model_validator

from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import Date, RecordData, Text, UInt
from sve_carddb.core.provenance import SourceUse, uses_sorted
from sve_carddb.core.regions import Region
from sve_carddb.domains.products.models import LocalizedText


class Evidence(RecordData):
    evidence: SourceUse

    @model_validator(mode="after")
    def official_source(self) -> Evidence:
        """Exclude media and third-party assertions from official-rule staging."""
        if self.evidence.source.kind not in {
            "official_page",
            "official_api",
            "official_pdf",
        }:
            raise ValueError("Construction staging requires official frozen evidence")
        return self


class Profile(Evidence):
    id: Text
    region: Region
    format_code: Text
    name: LocalizedText


class ProfileRevision(Evidence):
    id: Text
    profile_id: Text
    effective_from: Date
    effective_until: Date | None
    cr_version_id: Text | None
    default_copy_limit: UInt | None
    construction_rules_ref: Text | None

    @model_validator(mode="after")
    def interval(self) -> ProfileRevision:
        """Keep effective intervals half open and forward."""
        if (
            self.effective_until is not None
            and self.effective_from >= self.effective_until
        ):
            raise ValueError("Profile interval must be nonempty and forward")
        return self


class Member(RecordData):
    rules_name_id: Text
    choice_option: UInt
    deck_scope: Literal["main", "evolve", "all"]


class Restriction(Evidence):
    id: Text
    profile_id: Text
    announced_on: Date | None
    effective_from: Date
    effective_until: Date | None
    kind: Literal["copy_limit", "choice_group"]
    state: Literal["confirmed", "announced", "withdrawn"]
    max_copies: UInt | None
    max_selected_groups: UInt | None
    decision_id: Text | None
    members: tuple[Member, ...]

    @model_validator(mode="after")
    def conditions(self) -> Restriction:
        """Reject ambiguous limits and duplicate or absent targets."""
        if (
            self.effective_until is not None
            and self.effective_from >= self.effective_until
        ):
            raise ValueError("Restriction interval must be nonempty and forward")
        if (
            self.kind == "copy_limit"
            and not (self.max_copies is not None and self.max_selected_groups is None)
        ) or (
            self.kind == "choice_group"
            and not (self.max_copies is None and self.max_selected_groups is not None)
        ):
            raise ValueError("Restriction limit fields must match its kind exclusively")
        keys = {(member.rules_name_id, member.deck_scope) for member in self.members}
        if not self.members or len(keys) != len(self.members):
            raise ValueError("Restriction members must be nonempty and unique")
        return self


class Coverage(Evidence):
    profile_id: Text
    from_date: Date
    until_date: Date | None
    state: Literal["complete", "partial"]

    @model_validator(mode="after")
    def interval(self) -> Coverage:
        """Keep coverage intervals half open and forward."""
        if self.until_date is not None and self.from_date >= self.until_date:
            raise ValueError("Coverage interval must be nonempty and forward")
        return self


class DeckRoleOverride(RecordData):
    card_id: Text
    region: Region
    role: Literal["main", "evolve", "leader", "extra"]
    decision_id: Text


class Clause(RecordData):
    id: Text
    number: Text
    text: LocalizedText
    locator: Text


class CRVersion(Evidence):
    id: Text
    region: Region
    version: Text
    published_on: Date | None
    effective_on: Date | None
    clauses: tuple[Clause, ...]

    @model_validator(mode="after")
    def clause_language(self) -> CRVersion:
        """Keep clause numbers version-local and text region-specific."""
        expected = "ja" if self.region == "jp" else "en"
        if any(
            clause.text.lang != expected or not clause.text.text
            for clause in self.clauses
        ):
            raise ValueError(
                "CR clauses must have nonempty text in the regional language"
            )
        if len({clause.number for clause in self.clauses}) != len(self.clauses):
            raise ValueError("CR clause numbers must be unique within the version")
        return self


class Construction(RecordData):
    profiles: tuple[Profile, ...] = ()
    revisions: tuple[ProfileRevision, ...] = ()
    restrictions: tuple[Restriction, ...] = ()
    coverage: tuple[Coverage, ...] = ()
    overrides: tuple[DeckRoleOverride, ...] = ()
    cr_versions: tuple[CRVersion, ...] = ()

    def configuration(self) -> dict[str, JsonValue]:
        """Pin the entire staging plan, including exact text, locators and archive pins."""
        return {"construction": self.model_dump(mode="json")}

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Retain a locator for each observation and each CR clause."""
        records = (
            *self.profiles,
            *self.revisions,
            *self.restrictions,
            *self.coverage,
            *self.cr_versions,
        )
        uses = [record.evidence for record in records]
        uses.extend(
            SourceUse(
                source=version.evidence.source,
                usage="cr_clause",
                locator=clause.locator,
            )
            for version in self.cr_versions
            for clause in version.clauses
        )
        return uses_sorted(uses)


def load_construction(raw: bytes) -> Construction:
    """Read internal JSON staging, not an authored adoption shard or YAML contract."""
    return Construction.model_validate_json(canonical(parse(raw)))
