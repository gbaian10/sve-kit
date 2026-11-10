"""Current authored decisions retain their identity, evidence strength and hint text."""

from pathlib import Path

from sve_carddb.core.json import object_value
from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.domains.rulings.reader import load

ROOT = Path(__file__).resolve().parents[4]


def test_hishi_references_the_inferred_decision_with_project_interpretation_hints() -> (
    None
):
    path = ROOT / "authored/rules/author-effects/ECP01/ECP01-037.yaml"
    author = object_value(
        JSON_VALUE.validate_python(parse_yaml(path.read_bytes()), strict=True)
    )
    card = object_value(object_value(author["cards"])["ECP01-037"])
    assert object_value(card["meta"])["rulings"] == ["R-0017"]
    decision = {item.ruling.id: item.ruling for item in load(ROOT)}["R-0017"]
    assert decision.strength == "inferred"
    assert any(item.kind == "project" for item in decision.evidence)
    assert decision.hint["zh-TW"].startswith("本專案解讀")
    assert decision.hint["ja"].startswith("本プロジェクトの解釈")
    assert decision.hint["en"].startswith("Project interpretation")
