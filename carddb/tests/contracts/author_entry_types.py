"""Python correspondence for complete author containers and source evidence."""

from pydantic import ConfigDict
from typing_extensions import TypeForm

from sve_carddb.contracts import author_entries as entries
from sve_carddb.contracts import author_evidence as evidence
from sve_carddb.contracts import four_layer

from .author_adapters import adapters, type_names

CONFIG = ConfigDict(regex_engine="python-re")

ENTRY_TYPES: dict[str, tuple[TypeForm[object], str]] = {
    "A.ability": (entries.Ability, "Ability"),
    "A.ability.activate": (entries.ActivateAbility, "ActivateAbility"),
    "A.ability.attack-time": (entries.AttackTimeAbility, "AttackTimeAbility"),
    "A.ability.auto": (entries.AutoAbility, "AutoAbility"),
    "A.ability.drive-time": (entries.DriveTimeAbility, "DriveTimeAbility"),
    "A.ability.evolve": (entries.EvolveAbility, "EvolveAbility"),
    "A.ability.evolve-time": (entries.EvolveTimeAbility, "EvolveTimeAbility"),
    "A.ability.fanfare": (entries.FanfareAbility, "FanfareAbility"),
    "A.ability.feed": (entries.FeedAbility, "FeedAbility"),
    "A.ability.generated": (entries.GeneratedAbility, "GeneratedAbility"),
    "A.ability.keywords": (entries.KeywordsAbility, "KeywordsAbility"),
    "A.ability.lastword": (entries.LastwordAbility, "LastwordAbility"),
    "A.ability.play": (entries.PlayAbility, "PlayAbility"),
    "A.ability.race-time": (entries.RaceTimeAbility, "RaceTimeAbility"),
    "A.ability.ride": (entries.RideAbility, "RideAbility"),
    "A.ability.spell": (entries.SpellAbility, "SpellAbility"),
    "A.ability.static": (entries.StaticAbility, "StaticAbility"),
    "A.ability.super-evolve-time": (
        entries.SuperEvolveTimeAbility,
        "SuperEvolveTimeAbility",
    ),
    "A.card": (entries.AuthorCard, "AuthorCard"),
    "A.face": (entries.AuthorFace, "AuthorFace"),
    "A.generated-by": (entries.GeneratedBy, "GeneratedBy"),
    "A.inline-ability": (entries.InlineAbility, "InlineAbility"),
    "Actor": (evidence.Actor, "Actor"),
    "BodyDependencies": (evidence.BodyDependencies, "BodyDependencies"),
    "BodyReference": (evidence.BodyReference, "BodyReference"),
    "BodyRekey": (evidence.BodyRekey, "BodyRekey"),
    "BodyReview": (evidence.BodyReview, "BodyReview"),
    "Companion": (entries.Companion, "Companion"),
    "CrOwner": (four_layer.CrOwner, "CrOwner"),
    "DeckRule": (entries.DeckRule, "DeckRule"),
    "Domain": (evidence.Domain, "Domain"),
    "FaceRevisionOwner": (four_layer.FaceRevisionOwner, "FaceRevisionOwner"),
    "FamilyOwner": (four_layer.FamilyOwner, "FamilyOwner"),
    "ImportBinding": (evidence.ImportBinding, "ImportBinding"),
    "ImportUse": (evidence.ImportUse, "ImportUse"),
    "ImportedDependency": (evidence.ImportedDependency, "ImportedDependency"),
    "InlineDependencies": (evidence.InlineDependencies, "InlineDependencies"),
    "InterfaceDependency": (evidence.InterfaceDependency, "InterfaceDependency"),
    "KeywordOwner": (four_layer.KeywordOwner, "KeywordOwner"),
    "LocalBinding": (evidence.LocalBinding, "LocalBinding"),
    "MacroDependencies": (evidence.MacroDependencies, "MacroDependencies"),
    "MacroMechanical": (evidence.MacroMechanical, "MacroMechanical"),
    "MacroReference": (entries.MacroReference, "MacroReference"),
    "Meta": (evidence.Meta, "Meta"),
    "OccurrenceKey": (four_layer.OccurrenceKey, "OccurrenceKey"),
    "OriginalEvidence": (evidence.OriginalEvidence, "OriginalEvidence"),
    "Ports": (evidence.Ports, "Ports"),
    "PrintingFaceOwner": (four_layer.PrintingFaceOwner, "PrintingFaceOwner"),
    "ProductOwner": (four_layer.ProductOwner, "ProductOwner"),
    "QaOwner": (four_layer.QaOwner, "QaOwner"),
    "RecipePin": (evidence.RecipePin, "RecipePin"),
    "Registration": (evidence.Registration, "Registration"),
    "ResolvedVariant": (evidence.ResolvedVariant, "ResolvedVariant"),
    "ReviewedSource": (evidence.ReviewedSource, "ReviewedSource"),
    "RulingDependency": (evidence.RulingDependency, "RulingDependency"),
    "SourceUse": (evidence.SourceUse, "SourceUse"),
    "Span": (four_layer.Span, "Span"),
    "StructuralPath": (evidence.StructuralPath, "StructuralPath"),
    "UseDependencies": (evidence.UseDependencies, "UseDependencies"),
    "UseRekey": (evidence.UseRekey, "UseRekey"),
    "UseReview": (evidence.UseReview, "UseReview"),
    "VocabularyOwner": (four_layer.VocabularyOwner, "VocabularyOwner"),
    "WrittenBy": (evidence.WrittenBy, "WrittenBy"),
    "A.file": (entries.AuthorEffectSet, "AuthorEffectSet"),
    "A.body-shard": (entries.EffectBodyShard, "EffectBodyShard"),
    "authored": (entries.AuthorEffectSet, "AuthorEffectSet"),
}


ENTRIES = adapters(ENTRY_TYPES)
ENTRY_NAMES = type_names(
    {key: value for key, value in ENTRY_TYPES.items() if key != "authored"}
)
