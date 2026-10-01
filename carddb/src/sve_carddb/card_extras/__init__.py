"""Official Q&A, errata fragments, related links and adopted reskin validation."""

from sve_carddb.card_extras.archive import FrozenCardExtras, parse_card_page
from sve_carddb.card_extras.importer import (
    populate_card_extras,
    require_card_extras_ready,
)
from sve_carddb.card_extras.models import (
    CardPage,
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
    QAEntry,
    RelatedLink,
)
from sve_carddb.card_extras.plan import ExtrasPlan, plan_card_extras
from sve_carddb.card_extras.reskin import applicable_reskin_regions

__all__ = [
    "CardPage",
    "ErrataChange",
    "ErrataPage",
    "ErrataPrinting",
    "ExtrasPlan",
    "FrozenCardExtras",
    "QAEntry",
    "RelatedLink",
    "applicable_reskin_regions",
    "parse_card_page",
    "plan_card_extras",
    "populate_card_extras",
    "require_card_extras_ready",
]
