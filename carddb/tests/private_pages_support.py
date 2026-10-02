"""Pinned, field-specific oracles for private parser fixtures.

Text checks hash UTF-8 exactly; nulls and list lengths are separate checks.
Mapping checks allow new output fields. Local verification never consults Git.
"""

import hashlib
import json
import re
from dataclasses import asdict, dataclass
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from sve_carddb.extract import official_en as extract_en
from sve_carddb.extract import official_jp as extract_jp
from sve_carddb.sources import official_en as en
from sve_carddb.sources import official_jp as jp
from sve_carddb.sources import official_sv1 as sv1
from sve_carddb.sources import official_svwb as wb

if TYPE_CHECKING:
    from collections.abc import Mapping

INDEX_PATH = Path(__file__).parent / "fixtures" / "private-pages.json"
MODE_ENV = "SVE_PRIVATE_TESTDATA_MODE"
DIRECTORY_ENV = "SVE_PRIVATE_TESTDATA_DIR"
_HASH = re.compile(r"[0-9a-f]{64}")
_COMMIT = re.compile(r"[0-9a-f]{40}")
_CASE_ID = re.compile(r"F[0-9]{2}")
_PARSERS = frozenset(
    {
        "jp-card",
        "en-card",
        "sve-list-first",
        "sve-list-more",
        "sve-sets",
        "sv1-card",
        "wb-list",
    }
)


class PrivatePageError(ValueError):
    """A safe failure location, without private values or parser diagnostics."""


@dataclass(frozen=True)
class Check:
    path: tuple[str | int, ...]
    kind: str
    expected: str | int | bool | None


@dataclass(frozen=True)
class Case:
    id: str
    path: str
    sha256: str
    parser: str
    number: str | None
    page: int | None
    max_page: int | None
    total: int | None
    checks: tuple[Check, ...]


@dataclass(frozen=True)
class Index:
    commit: str
    cases: tuple[Case, ...]


def mode(environment: Mapping[str, str]) -> str:
    value = environment.get(MODE_ENV, "excluded")
    if value not in {"required", "excluded"}:
        raise PrivatePageError("private-page mode must be required or excluded")
    return value


def _object(value: object, fields: set[str]) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise PrivatePageError("invalid private-page index object")
    return {str(key): item for key, item in value.items()}


def _string(value: object) -> str:
    if not isinstance(value, str) or not value or any(ord(char) < 32 for char in value):
        raise PrivatePageError("invalid private-page index string")
    return value


def _integer(value: object) -> int:
    if type(value) is not int or value < 0:
        raise PrivatePageError("invalid private-page index integer")
    return value


def _pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    values: dict[str, object] = {}
    for key, value in pairs:
        if key in values:
            raise PrivatePageError("duplicate private-page index key")
        values[key] = value
    return values


def _check(value: object) -> Check:
    if not isinstance(value, dict):
        raise PrivatePageError("invalid private-page check")
    kind = _string(value.get("kind"))
    raw = _object(
        value,
        {"path", "kind"}
        | ({"expected"} if kind in {"text", "integer", "boolean", "length"} else set()),
    )
    path = raw["path"]
    if not isinstance(path, list) or not path:
        raise PrivatePageError("invalid private-page check path")
    segments = [
        _integer(segment) if type(segment) is int else _string(segment)
        for segment in path
    ]
    expected: str | int | bool | None = None
    if kind == "text":
        expected = _string(raw["expected"])
        if not _HASH.fullmatch(expected):
            raise PrivatePageError("invalid private-page text hash")
    elif kind in {"integer", "length"}:
        expected = _integer(raw["expected"])
    elif kind == "boolean":
        if type(raw["expected"]) is not bool:
            raise PrivatePageError("invalid private-page boolean")
        expected = raw["expected"]
    elif kind not in {"null", "mapping"}:
        raise PrivatePageError("unknown private-page check kind")
    return Check(tuple(segments), kind, expected)


def _case(value: object) -> Case:
    raw = _object(value, {"id", "path", "sha256", "parser", "arguments", "checks"})
    identity = _string(raw["id"])
    if not _CASE_ID.fullmatch(identity):
        raise PrivatePageError("invalid private-page case ID")
    path = _string(raw["path"])
    relative = PurePosixPath(path)
    if (
        relative.is_absolute()
        or ".." in relative.parts
        or relative.as_posix() != path
        or relative.parts[:2] != ("fixtures", "carddb")
        or relative.suffix not in {".html", ".json"}
    ):
        raise PrivatePageError("unsafe private-page fixture path")
    digest = _string(raw["sha256"])
    if not _HASH.fullmatch(digest):
        raise PrivatePageError("invalid private-page fixture hash")
    parser = _string(raw["parser"])
    if parser not in _PARSERS:
        raise PrivatePageError("unknown private-page parser")
    fields = (
        {"number"}
        if parser in {"jp-card", "en-card"}
        else {"page", "max_page", "total"}
        if parser == "sve-list-more"
        else set()
    )
    args = _object(raw["arguments"], fields)
    checks = raw["checks"]
    if not isinstance(checks, list) or not checks:
        raise PrivatePageError("private-page case has no checks")
    parsed = tuple(_check(check) for check in checks)
    if len({check.path for check in parsed}) != len(parsed):
        raise PrivatePageError("duplicate private-page check path")
    return Case(
        identity,
        path,
        digest,
        parser,
        _string(args["number"]) if "number" in args else None,
        _integer(args["page"]) if "page" in args else None,
        _integer(args["max_page"]) if "max_page" in args else None,
        _integer(args["total"]) if "total" in args else None,
        parsed,
    )


def load_index(path: Path = INDEX_PATH) -> Index:
    try:
        document: object = json.loads(path.read_bytes(), object_pairs_hook=_pairs)
    except OSError, json.JSONDecodeError, UnicodeDecodeError:
        raise PrivatePageError("cannot read private-page index") from None
    raw = _object(document, {"version", "commit", "cases"})
    if type(raw["version"]) is not int or raw["version"] != 1:
        raise PrivatePageError("unsupported private-page index version")
    commit = _string(raw["commit"])
    if not _COMMIT.fullmatch(commit):
        raise PrivatePageError("invalid private-page commit pin")
    cases = raw["cases"]
    if not isinstance(cases, list) or not cases:
        raise PrivatePageError("private-page index has no cases")
    parsed = tuple(_case(case) for case in cases)
    if len({case.id for case in parsed}) != len(parsed) or len(
        {case.path for case in parsed}
    ) != len(parsed):
        raise PrivatePageError("duplicate private-page case or fixture")
    return Index(commit, parsed)


def read_inputs(index: Index, directory: Path) -> dict[str, bytes]:
    try:
        root = directory.resolve(strict=True)
    except OSError:
        raise PrivatePageError("private-page data directory is unavailable") from None
    if not root.is_dir():
        raise PrivatePageError("private-page data directory is unavailable")
    bodies: dict[str, bytes] = {}
    for case in index.cases:
        try:
            path = (root / case.path).resolve(strict=True)
            if not path.is_relative_to(root) or not path.is_file():
                raise PrivatePageError(f"{case.id}: unsafe fixture")
            body = path.read_bytes()
        except OSError:
            raise PrivatePageError(f"{case.id}: fixture unavailable") from None
        if hashlib.sha256(body).hexdigest() != case.sha256:
            raise PrivatePageError(f"{case.id}: fixture hash mismatch")
        bodies[case.id] = body
    return bodies


def project(case: Case, body: bytes) -> dict[str, object]:
    try:
        if case.parser in {"jp-card", "en-card"} and case.number is not None:
            source = en if case.parser == "en-card" else jp
            extractor = extract_en if case.parser == "en-card" else extract_jp
            return {
                "page": asdict(source.parse_card(body, expected_number=case.number)),
                "record": asdict(extractor.extract_card(body, number=case.number)),
            }
        if case.parser == "sve-list-first":
            return {"page": asdict(jp.parse_list_first(body))}
        if (
            case.parser == "sve-list-more"
            and case.page is not None
            and case.max_page is not None
            and case.total is not None
        ):
            return {
                "page": asdict(
                    jp.parse_list_more(
                        body, page=case.page, max_page=case.max_page, total=case.total
                    )
                )
            }
        if case.parser == "sve-sets":
            return {"sets": [asdict(product) for product in jp.parse_sets(body)]}
        if case.parser == "sv1-card":
            return {"images": asdict(sv1.parse_card_images(body))}
        if case.parser == "wb-list":
            return {
                "page": asdict(wb.parse_list(body)),
                "image_hashes": wb.image_hashes(body),
            }
    except ValueError, LookupError, TypeError:
        # Parser errors can quote private text; only expose the safe case location.
        raise PrivatePageError(f"{case.id}: parser rejected fixture") from None
    raise PrivatePageError(f"{case.id}: invalid parser arguments")


def _matches(check: Check, value: object) -> bool:
    match check.kind:
        case "null":
            return value is None
        case "mapping":
            return isinstance(value, dict)
        case "text":
            return (
                isinstance(value, str)
                and hashlib.sha256(value.encode("utf-8")).hexdigest() == check.expected
            )
        case "integer":
            return type(value) is int and value == check.expected
        case "boolean":
            return type(value) is bool and value == check.expected
        case _:
            return (
                check.kind == "length"
                and isinstance(value, list)
                and len(value) == check.expected
            )


def check_projection(case: Case, projection: dict[str, object]) -> None:
    for check in case.checks:
        value: object = projection
        location = ".".join(str(part) for part in check.path)
        for part in check.path:
            if isinstance(part, str) and isinstance(value, dict) and part in value:
                value = value[part]
                continue
            if type(part) is int and isinstance(value, list) and part < len(value):
                value = value[part]
                continue
            raise PrivatePageError(f"{case.id}: {location}: missing field")
        if not _matches(check, value):
            raise PrivatePageError(f"{case.id}: {location}: {check.kind} mismatch")
