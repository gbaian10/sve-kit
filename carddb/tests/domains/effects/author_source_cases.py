"""Synthetic resolved contracts are not results of production source admission."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.contracts.four_layer import (
    Frame,
    Projection,
    Scope,
    SemanticVariant,
    hash_payload,
)
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    SourceContext,
    check_source_compatibility,
)
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import DOMAIN, DOMAIN_CODE

from ..translations.test_four_layer_classification import classifier, source

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceBinding


@dataclass(frozen=True)
class SyntheticOffset:
    candidate: SourceCandidate
    frame: Frame
    binding: SourceBinding
    canonical_source: str

    def compatible(self) -> SourceContext:
        return check_source_compatibility(
            self.frame,
            self.binding,
            self.canonical_source,
            {**self.candidate.classifier.domains, DOMAIN_CODE: DOMAIN},
            classifier=self.candidate.classifier,
        )

    def use(self) -> SourceUse:
        return SourceUse(
            binding_id=self.binding.id,
            occurrence=self.binding.occurrence_key(),
            ability_scope="ability_0",
        )

    def declaration(self) -> ParamDecl:
        slot = next(
            s for s in self.frame.leaf_schema.slots if s.role == "arithmetic_offset"
        )
        return ParamDecl(
            type="Int", source=ParamSource(slot=slot.name, component="value")
        )


def synthetic_offset(*, named: bool = False) -> SyntheticOffset:
    count = "自分の場の『仮名』の数+5" if named else "自分のデッキの枚数-5"
    raw = f"それにXダメージ。Xは「{count}」である。"
    terms = (Term("term:name.synthetic", "card_name", "仮名"),) if named else ()
    engine = classifier(terms)
    candidate = SourceCandidate(raw, source(raw), engine)
    derived = derive_source(raw, candidate.descriptor, engine)
    frame, binding = derived.bind(0)
    canonical_source = derived.field.parts[0].canonical_source
    frame = frame.model_copy(
        update={
            "semantic_variant": SemanticVariant(
                state="resolved", key="synthetic.offset.v1", scope=None
            ),
            "projection": Projection(
                projection_kind="ability_body",
                discriminator="synthetic.offset.v1",
                scopes=(Scope(id="ability_0", parent=None, kind="ability"),),
                imports=(),
                exports=(),
            ),
        }
    )
    checksum = hash_payload(frame.payload(canonical_source))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    binding = binding.model_copy(update={"frame_id": frame.id})
    binding = binding.model_copy(
        update={"id": "bind:" + hash_payload(binding.payload())}
    )
    return SyntheticOffset(candidate, frame, binding, canonical_source)
