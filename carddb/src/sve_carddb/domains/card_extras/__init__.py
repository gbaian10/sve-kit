"""Official Q&A, errata fragments, related links and adopted reskin validation."""

from sve_carddb.domains.card_extras.archive import FrozenCardExtras, parse_card_page
from sve_carddb.domains.card_extras.importer import (
    CardExtrasRestriction,
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.domains.card_extras.models import (
    CardPage,
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
    QABlock,
    QAEntry,
    QAPage,
    RelatedLink,
)
from sve_carddb.domains.card_extras.plan import ExtrasPlan, plan_card_extras
from sve_carddb.domains.card_extras.reskin import applicable_reskin_regions

__all__ = [
    "CardExtrasRestriction",
    "CardPage",
    "ErrataChange",
    "ErrataPage",
    "ErrataPrinting",
    "ExtrasPlan",
    "FrozenCardExtras",
    "QABlock",
    "QAEntry",
    "QAPage",
    "RelatedLink",
    "applicable_reskin_regions",
    "parse_card_page",
    "plan_card_extras",
    "populate_card_extras",
    "require_card_extras_ready",
]
