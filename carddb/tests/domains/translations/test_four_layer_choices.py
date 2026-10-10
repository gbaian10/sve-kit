"""Replacement choice counts require a unique reachable introduction and explicit grammar."""

import pytest

from sve_carddb.contracts.template_parameters import Range
from sve_carddb.domains.translations.four_layer_choices import choice_alternative
from sve_carddb.domains.translations.parameters.models import Hint


@pytest.mark.parametrize(
    ("suffix", "expected"),
    [
        ("【ネクロチャージ７】代わりに２つまで。", True),
        ("【ネクロチャージ７】代わりに３つまで。", False),
        ("【NC７】代わりに２つまで。", False),
        ("【ネクロチャージ７】代わりに２つ。", False),
        ("{起動}【ネクロチャージ７】代わりに２つまで。", False),
        ("下記から１つチョイスする。【ネクロチャージ７】代わりに２つまで。", False),
    ],
)
def test_post_option_replacement_count_is_bound_to_the_original_choice(
    suffix: str, expected: bool
) -> None:
    raw = "下記から１つチョイスする。【１】仮。【２】別。" + suffix
    start = raw.rindex("代わりに") + len("代わりに")
    span = Range(start=start, end=start + 1)
    hint = Hint(
        name="count",
        occurrence=span,
        source_segments=(span,),
        transformation="fullwidth_to_ascii",
        semantic_role="numeric",
        numeric_rule=None,
        type="uint",
        reference_kind=None,
        value=int(raw[start]),
        target=None,
        issues=(),
    )
    assert choice_alternative(raw, hint) is expected
