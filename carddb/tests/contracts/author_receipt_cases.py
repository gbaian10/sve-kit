"""Receipt read examples and decoder correspondence, independent of the schema."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_queries as queries
from sve_carddb.contracts import author_receipts as author

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
RECEIPTS: dict[str, TypeAdapter[object]] = {
    "A.expr.objects-of": TypeAdapter(author.ExprObjectsOf),
    "A.snapshot-read": TypeAdapter(author.SnapshotRead, config=CONFIG),
    "A.same-as": TypeAdapter(author.SameAs),
    "A.material-filter": TypeAdapter(queries.MaterialFilter, config=CONFIG),
    "A.expr.count-of-filtered": TypeAdapter(queries.CountOfFiltered),
    "A.cond.did-filtered": TypeAdapter(queries.DidFiltered, config=CONFIG),
}
SNAPSHOT: dict[str, JsonValue] = {"objects_of": "material", "view": "before"}
COUNTED: dict[str, JsonValue] = {
    "receipt": "revealed",
    "where": {"orig_cost_eq": 2},
    "view": "before",
}
DID: dict[str, JsonValue] = {
    "result": {"entry_payment": "additional"},
    "where": {"name": "ブラスター・ダーク", "zone": "field"},
}
RECEIPT_CASES: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.expr.objects-of", SNAPSHOT),
    ("A.snapshot-read", SNAPSHOT),
    ("A.same-as", {"same_as": SNAPSHOT}),
    ("A.material-filter", {"orig_cost_eq": 2}),
    ("A.expr.count-of-filtered", COUNTED),
    ("A.cond.did-filtered", DID),
]

RECEIPT_NAMES = {
    "A.expr.objects-of": "ExprObjectsOf",
    "A.material-filter": "MaterialFilter",
    "A.expr.count-of-filtered": "CountOfFiltered",
    "A.cond.did-filtered": "DidFiltered",
}
