# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Neutral format validator for sve-exam/2 question files (contract v2.1, CONTRACT.md).

Checks structure and references only; it never decides whether an expected result is correct.

Usage: uv run validate.py <cards.jsonl> <file-or-dir>...
"""

# A CLI checklist: printing is its output, YAML data is untyped, and each check is
# one flat branch, so splitting them up would only scatter the list.
# ruff: file-ignore[print, any-type, complex-structure, too-many-branches, too-many-locals, too-many-statements]

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

import yaml

SCHEMAS = {"sve-exam/2.1"}
KINDS = {"card", "rule", "flow"}
STATUSES = {"draft", "verified", "disputed"}
# opus writes the holdout positions kept outside the repo.
AUTHORS = {"fable", "astra", "opus"}
PLAYERS = {"P1", "P2"}
CONSTRUCTIONS = {"class", "title", "crossover"}
PHASES = {"start", "main", "end"}
HISTORIES = {"none", "explicit"}
ZONES = {
    "deck",
    "evolve_deck",
    "hand",
    "field",
    "ex",
    "cemetery",
    "banish",
    "resolution",
    "evolution",
    "race",
    "drive",
    "trigger",
    "equipment",
}
TOKEN_ALLOWED = {  # 9.1.4.1-3
    "フォロワー": {"ex", "field", "resolution"},
    "アミュレット": {"ex", "field", "resolution"},
    "クレスト": {"ex"},
    "スペル": {"ex", "resolution"},
}
AT_VALUES = {"main", "check-timing", "quick", "resolve", "start", "end", "pregame"}
DOS = {
    "play": {
        "card",
        "targets",
        "costs",
        "optional_costs",
        "x",
        "options",
        "distribute",
    },
    "activate": {
        "ability",
        "targets",
        "costs",
        "optional_costs",
        "x",
        "options",
        "distribute",
        "pay",
    },
    "evolve": {"source", "ability", "evolve_card", "costs", "pay", "face"},
    "attack": {"attacker", "target"},
    "choose-pending": {
        "pending",
        "targets",
        "costs",
        "optional_costs",
        "x",
        "options",
        "distribute",
    },
    "resolve-choice": {
        "choice",
        "select",
        "order",
        "distribute",
        "continue",
        "keyword",
        "position",
        "options",
    },
    "pass": set(),
    "end-phase": set(),
    # v2.1
    "end-discard": {"select"},
    "guard-act": {"select"},
    "place-acted": {"object", "acted"},
    "order-replacements": {"order"},
    "choose-first": {"first"},
    "mulligan": {"redo", "order"},
    "choose-start-amulet": {"object"},
}
OF_TARGETS = {"play", "activate", "evolve", "choose-pending", "attack"}
OF_ALLOWED = {"resolve-choice", "place-acted", "order-replacements"}
NEW_ID = re.compile(r"^new-\d+$")
OUTCOMES = {
    "resolved",
    "paused",
    "cannot-play",
    "cannot-activate",
    "cannot-evolve",
    "cannot-attack",
    "pending-cancelled",
    "game-end",
}
VIEWS = {"omniscient", "P1", "P2"}
EVENT_KINDS = {
    "プレイ",
    "解決",
    "費用成立",
    "移動",
    "場に出す",
    "引く",
    "捨てる",
    "ダメージ",
    "破壊",
    "消滅",
    "消去",
    "アクト",
    "スタンド",
    "カウンター",
    "体力増加",
    "回復",
    "進化",
    "攻撃",
    "待機",
    "待機取消",
    "取代",
    "敗北",
    "公開",
    "抑制",  # v2.1
}
TRIGGER_ICONS = {"critical", "draw", "stand", "heal", "none"}  # 14.4.5.1.3.1-4
AT_RE = re.compile(r"^after-decision-(\d+)$")


class Report:
    """Errors collected for one question file."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.errors: list[str] = []

    def err(self, where: str, msg: str) -> None:
        """Record one error at a location inside the file."""
        self.errors.append(f"{where}: {msg}")


def load_cards(path: Path) -> dict[str, dict[str, Any]]:
    """Load carddb's cards.jsonl, keyed by card number."""
    cards: dict[str, dict[str, Any]] = {}
    with path.open(encoding="utf-8") as f:
        for line in f:
            record = json.loads(line)
            cards[record["number"]] = record
    return cards


def token_type(card: dict[str, Any]) -> str | None:
    """Return the token's card type (a key of TOKEN_ALLOWED), or None if not a token."""
    card_type = card["faces"][0]["card_type"]
    if "トークン" not in card_type:
        return None
    return next((t for t in TOKEN_ALLOWED if t in card_type), None)


def merged_setup(
    scenarios: dict[str, dict[str, Any]], name: str, seen: tuple[str, ...] = ()
) -> dict[str, Any]:
    """Resolve a scenario's setup and card_facts through its inherit chain."""
    scenario = scenarios[name]
    parent = scenario.get("inherit")
    if parent is None:
        return {
            "setup": scenario.get("setup") or {},
            "card_facts": scenario.get("card_facts") or {},
        }
    if parent in seen or parent == name:
        raise ValueError(f"inherit cycle via {parent}")
    if parent not in scenarios:
        raise ValueError(f"inherit target {parent!r} not found")
    base = merged_setup(scenarios, parent, (*seen, name))
    return {
        "setup": deep_merge(base["setup"], scenario.get("setup") or {}),
        "card_facts": deep_merge(base["card_facts"], scenario.get("card_facts") or {}),
    }


def deep_merge(base: Any, over: Any) -> Any:
    """Merge mappings recursively; any other value in `over` replaces `base`."""
    if isinstance(base, dict) and isinstance(over, dict):
        out = dict(base)
        for key, value in over.items():
            out[key] = deep_merge(base.get(key), value) if key in base else value
        return out
    return over


def collect_objects(
    setup: dict[str, Any], rep: Report, where: str
) -> dict[str, tuple[str, str, str]]:
    """Map object id -> (player, zone, card number)."""
    objects: dict[str, tuple[str, str, str]] = {}
    players = setup.get("players") or {}
    if set(players) != PLAYERS:
        rep.err(
            where, f"setup.players must be exactly P1 and P2, got {sorted(players)}"
        )
    for player, raw in players.items():
        pdata = raw or {}
        if pdata.get("construction") not in CONSTRUCTIONS:
            rep.err(
                where, f"{player}.construction must be one of {sorted(CONSTRUCTIONS)}"
            )
        zones = dict(pdata.get("zones") or {})
        for key, zone in (
            ("deck_list", "deck"),
            ("evolve_deck_list", "evolve_deck"),
        ):  # contract 9.13
            if key in pdata:
                zones[zone] = [*zones.get(zone, []), *pdata[key]]
        for zone, entries in zones.items():
            if zone not in ZONES:
                rep.err(where, f"{player}.zones.{zone} is not a contract zone")
                continue
            for entry in entries or []:
                if "filler" in entry:
                    if zone not in {"deck", "hand"}:
                        rep.err(
                            where,
                            f"filler not allowed in {player}.{zone} (contract 2.3)",
                        )
                    continue
                oid, card = entry.get("id"), entry.get("card")
                if not oid or not card:
                    rep.err(where, f"{player}.{zone} entry needs id and card: {entry}")
                    continue
                if oid in objects:
                    rep.err(where, f"duplicate object id {oid}")
                objects[oid] = (player, zone, str(card))
    return objects


def walk_ids(value: Any) -> list[str]:
    """Every string that looks like an object id inside a structure."""
    found: list[str] = []
    if isinstance(value, str):
        found.append(value)
    elif isinstance(value, dict):
        for v in value.values():
            found.extend(walk_ids(v))
    elif isinstance(value, list):
        for v in value:
            found.extend(walk_ids(v))
    return found


def check_ability(
    ability: Any, objects: dict[str, Any], rep: Report, where: str
) -> None:
    """Check an ability reference: a known source and a line (or rule)."""
    if not isinstance(ability, dict):
        rep.err(where, f"ability must be a mapping, got {ability!r}")
        return
    if ability.get("source") not in objects and not NEW_ID.match(
        str(ability.get("source"))
    ):
        rep.err(
            where, f"ability.source {ability.get('source')!r} is not an object in setup"
        )
    if "line" not in ability and "rule" not in ability:
        rep.err(where, "ability needs line (or rule)")


def check_decisions(
    scenario: dict[str, Any], objects: dict[str, Any], rep: Report, where: str
) -> dict[int, str]:
    """Check each decision and return its `do` by decision number."""
    kinds: dict[int, str] = {}
    for index, decision in enumerate(scenario.get("decisions") or [], start=1):
        dwhere = f"{where} decision {index}"
        if decision.get("n") != index:
            rep.err(
                dwhere,
                f"n must be {index} (consecutive from 1), got {decision.get('n')}",
            )
        if decision.get("by") not in PLAYERS:
            rep.err(dwhere, "by must be P1 or P2")
        if decision.get("at") not in AT_VALUES:
            rep.err(dwhere, f"at must be one of {sorted(AT_VALUES)}")
        do = decision.get("do")
        kinds[index] = str(do)
        if do not in DOS:
            rep.err(dwhere, f"unknown do {do!r}")
            continue
        extra = set(decision) - {"n", "by", "at", "do", "of"} - DOS[do]
        if extra:
            rep.err(dwhere, f"{do} does not take {sorted(extra)}")
        if decision.get("at") == "resolve" or ("of" in decision and do in OF_ALLOWED):
            of = decision.get("of") or {}
            target = of.get("decision")
            if (
                not isinstance(target, int)
                or target >= index
                or kinds.get(target) not in OF_TARGETS
            ):
                rep.err(
                    dwhere,
                    f"of.decision must point to an earlier {sorted(OF_TARGETS)} decision",
                )
            if not isinstance(of.get("occurrence"), int) or of["occurrence"] < 1:
                rep.err(dwhere, "of.occurrence must be an integer >= 1")
        elif "of" in decision:
            rep.err(
                dwhere,
                f"of is only allowed with at: resolve or for {sorted(OF_ALLOWED)}",
            )
        costs = decision.get("costs")
        if costs is not None and costs != "decline" and not isinstance(costs, dict):
            rep.err(dwhere, "costs must be a mapping or 'decline'")
        if costs == "decline" and do not in {"activate", "choose-pending"}:
            rep.err(dwhere, "costs: decline is only for activate / choose-pending")
        options = decision.get("options")
        if options is not None and (
            not isinstance(options, list)
            or not all(isinstance(o, int) for o in options)
        ):
            rep.err(dwhere, "options must be a list of integers")
        elif options and len(set(options)) != len(options):
            # contract 9.12: an illegal player decision is allowed only as a negative example
            outcomes = {
                e.get("outcome")
                for e in scenario.get("expected") or []
                if e.get("at") == f"after-decision-{index}"
            }
            if not any(str(o).startswith("cannot-") for o in outcomes):
                rep.err(
                    dwhere,
                    f"duplicate options need a cannot-* outcome at after-decision-{index} (illegal-decision example)",
                )
        pay = decision.get("pay")
        if pay is not None and (
            not isinstance(pay, dict)
            or pay.get("ep", 0) not in {0, 1}
            or pay.get("sep", 0) not in {0, 1}
        ):
            rep.err(dwhere, "pay needs {pp, ep: 0|1, sep: 0|1}")
        if "ability" in decision:
            check_ability(decision["ability"], objects, rep, dwhere)
        for key in ("card", "attacker", "source", "evolve_card", "object"):
            if (
                key in decision
                and decision[key] not in objects
                and not NEW_ID.match(str(decision[key]))
            ):
                rep.err(dwhere, f"{key} {decision[key]!r} is not an object in setup")
        target = decision.get("target")
        if (
            target is not None
            and target not in objects
            and target not in {"P1.leader", "P2.leader"}
            and not NEW_ID.match(str(target))
        ):
            rep.err(dwhere, f"target {target!r} is not an object or leader")
    return kinds


def check_expected(
    scenario: dict[str, Any],
    kinds: dict[int, str],
    tokens: dict[str, str],
    rep: Report,
    where: str,
) -> None:
    """Check each expected checkpoint against the decisions and the contract."""
    for index, exp in enumerate(scenario.get("expected") or [], start=1):
        ewhere = f"{where} expected {index}"
        match = AT_RE.match(str(exp.get("at")))
        if not match or int(match.group(1)) not in kinds:
            rep.err(
                ewhere,
                f"at must be after-decision-N for an existing decision, got {exp.get('at')!r}",
            )
        if exp.get("view") not in VIEWS:
            rep.err(ewhere, f"view must be one of {sorted(VIEWS)}")
        outcome = exp.get("outcome")
        if outcome not in OUTCOMES:
            rep.err(ewhere, f"outcome must be one of {sorted(OUTCOMES)}")
        if outcome == "paused" and not exp.get("awaiting"):
            rep.err(ewhere, "paused requires awaiting")
        if "awaiting" in exp and exp["awaiting"] is None:
            rep.err(ewhere, "awaiting: null was removed in contract v2")
        for choice in (exp.get("awaiting") or {}).get("choices") or []:
            if not isinstance(choice, dict) or choice.get("do") not in DOS:
                rep.err(
                    ewhere,
                    f"awaiting choice must be a decision shape with a known do: {choice!r}",
                )
        for key in ("events", "forbidden_events"):
            for event in exp.get(key) or []:
                if event.get("kind") not in EVENT_KINDS:
                    rep.err(
                        ewhere,
                        f"{key} kind {event.get('kind')!r} is not in the contract vocabulary",
                    )
                if any("filler" in str(v) for v in walk_ids(event)):
                    rep.err(ewhere, f"filler must not appear in {key}")
        if exp.get("events") == []:
            rep.err(
                ewhere,
                "events: [] asserts nothing under subsequence matching; use forbidden_events",
            )
        for path, value in (exp.get("assert") or {}).items():
            player, _, zone = str(path).partition(".")
            if player in PLAYERS and zone in ZONES and isinstance(value, list):
                for item in value:
                    if (
                        isinstance(item, str)
                        and item in tokens
                        and zone not in TOKEN_ALLOWED[tokens[item]]
                    ):
                        rep.err(
                            ewhere,
                            f"token {item} asserted in {zone} (9.1.4.4: erased immediately)",
                        )


def check_file(path: Path, cards: dict[str, dict[str, Any]]) -> Report:
    """Validate one question file."""
    rep = Report(path)
    try:
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as exc:
        rep.err("file", f"YAML error: {exc}")
        return rep
    if doc.get("schema") not in SCHEMAS:
        rep.err("file", f"schema must be one of {sorted(SCHEMAS)}")
    for key in (
        "id",
        "kind",
        "must_pass",
        "cards",
        "why",
        "discriminates",
        "refs",
        "evidence",
        "status",
        "author",
        "scenarios",
    ):
        if doc.get(key) in (None, "", []):
            rep.err("file", f"missing {key}")
    if doc.get("kind") not in KINDS:
        rep.err("file", f"kind must be one of {sorted(KINDS)}")
    if doc.get("status") not in STATUSES:
        rep.err("file", f"status must be one of {sorted(STATUSES)}")
    # merged questions list both authors
    authors = doc.get("author")
    authors = authors if isinstance(authors, list) else [authors]
    if not authors or any(a not in AUTHORS for a in authors):
        rep.err("file", f"author must be one of {sorted(AUTHORS)} (or a list of them)")
    listed = {str(c) for c in doc.get("cards") or []}
    for number in listed - set(cards):
        rep.err("file", f"cards lists unknown card number {number}")

    scenarios = {s.get("name"): s for s in doc.get("scenarios") or []}
    if len(scenarios) != len(doc.get("scenarios") or []):
        rep.err("file", "scenario names must be unique")
    icons: dict[str, Any] = {}
    used: set[str] = set()
    for name, scenario in scenarios.items():
        where = f"scenario {name!r}"
        try:
            merged = merged_setup(scenarios, name)
        except ValueError as exc:
            rep.err(where, str(exc))
            continue
        setup = merged["setup"]
        # contract 9.13: pregame scenarios start before turn state exists
        if not setup.get("pregame"):
            if setup.get("history") not in HISTORIES:
                rep.err(where, f"history must be one of {sorted(HISTORIES)}")
            turn = setup.get("turn") or {}
            if (
                turn.get("phase") not in PHASES
                or turn.get("active") not in PLAYERS
                or turn.get("first_player") not in PLAYERS
            ):
                rep.err(where, "turn needs active, first_player, phase")
            if set(turn.get("elapsed_turns") or {}) != PLAYERS:
                rep.err(where, "turn.elapsed_turns needs P1 and P2 (3.3.1)")
        objects = collect_objects(setup, rep, where)
        for oid, (_, _, number) in objects.items():
            used.add(number)
            if number not in cards:
                rep.err(where, f"object {oid} uses unknown card {number}")
        tokens = {
            oid: t
            for oid, (_, _, n) in objects.items()
            if n in cards and (t := token_type(cards[n]))
        }
        for number, facts in (merged["card_facts"] or {}).items():
            used.add(str(number))
            icon = (facts or {}).get("trigger_icon")
            if icon is not None and icon not in TRIGGER_ICONS:
                rep.err(
                    where,
                    f"card_facts {number}.trigger_icon must be one of {sorted(TRIGGER_ICONS)}",
                )
            # `source` is free text; only the facts themselves must agree.
            key = {k: v for k, v in (facts or {}).items() if k != "source"}
            previous = icons.setdefault(str(number), key)
            if previous != key:
                rep.err(
                    where,
                    f"card_facts for {number} differ between scenarios (one card number, one set of facts)",
                )
        kinds = check_decisions(scenario, objects, rep, where)
        check_expected(scenario, kinds, tokens, rep, where)
    for number in used - listed:
        rep.err("file", f"card {number} is used but missing from cards")
    return rep


def main(argv: list[str]) -> int:
    """Validate every question file given; exit 1 if any fails."""
    cards_arg, *targets = argv[1:] or [""]
    if not targets:
        print(__doc__)
        return 2
    cards = load_cards(Path(cards_arg))
    files: list[Path] = []
    for arg in targets:
        p = Path(arg)
        files.extend(sorted(p.rglob("*.yaml")) if p.is_dir() else [p])
    failed = 0
    for path in files:
        rep = check_file(path, cards)
        if rep.errors:
            failed += 1
            print(f"✗ {path} ({len(rep.errors)})")
            for e in rep.errors:
                print(f"    {e}")
        else:
            print(f"✓ {path}")
    print(f"\n{len(files) - failed}/{len(files)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
