"""Run existing independent synthetic contract oracles against retained v1 calculations.

The assertions and examples stay in their original test modules. Only imported
calculation functions and error types change; producer Git code is never loaded.
"""

import sys
from types import FunctionType, ModuleType

import pytest
from _pytest.mark.structures import ParameterSet

from sve_carddb.template_semantics.v1 import candidates as inventory
from sve_carddb.template_semantics.v1 import (
    html,
    identity,
    official_en,
    official_jp,
    presence,
    validation,
)
from sve_carddb.template_semantics.v1.parameters import (
    analysis,
    candidate_matching,
    numeric_rules,
    provenance,
    references,
    rule_candidates,
    spans,
    verification,
)

from . import (
    test_effect_presence,
    test_extract,
    test_extract_en,
    test_template_parameters,
    test_template_rule_candidates,
    test_validate,
)

MODULES = {
    "sve_carddb.html": html,
    "sve_carddb.fetch.validate": validation,
    "sve_carddb.extract.official_jp": official_jp,
    "sve_carddb.extract.official_en": official_en,
    "sve_carddb.registry.inputs": identity,
    "sve_carddb.text_observations.presence": presence,
    "sve_carddb.template_parameters.inventory": inventory,
    "sve_carddb.template_parameters.analysis": analysis,
    "sve_carddb.template_parameters.candidate_matching": candidate_matching,
    "sve_carddb.template_parameters.numeric_rules": numeric_rules,
    "sve_carddb.template_parameters.provenance": provenance,
    "sve_carddb.template_parameters.references": references,
    "sve_carddb.template_parameters.rule_candidates": rule_candidates,
    "sve_carddb.template_parameters.spans": spans,
    "sve_carddb.template_parameters.verification": verification,
}


_SCOPES: dict[str, dict[str, object]] = {}


def _replacement(value: object) -> object:  # ruff: ignore[too-many-return-statements] -- test imports and nested parameter sets retain their original shape
    if isinstance(value, ModuleType):
        return MODULES.get(value.__name__, value)
    if isinstance(value, (FunctionType, type)):
        if value.__module__ in _SCOPES:
            return _SCOPES[value.__module__].get(value.__name__, value)
        module = MODULES.get(value.__module__)
        if module is not None and hasattr(module, value.__name__):
            return getattr(module, value.__name__)
    if isinstance(value, ParameterSet):
        return value._replace(values=tuple(_replacement(v) for v in value.values))
    if isinstance(value, tuple):
        return tuple(_replacement(v) for v in value)
    if isinstance(value, list):
        return [_replacement(v) for v in value]
    return value


def _collect(source: ModuleType, prefix: str) -> None:
    scope = {name: _replacement(value) for name, value in vars(source).items()}
    for name, value in vars(source).items():
        for original, fixed in MODULES.items():
            module = sys.modules.get(original)
            if (
                module is not None
                and not isinstance(value, ModuleType)
                and getattr(module, name, None) is value
                and hasattr(fixed, name)
            ):
                scope[name] = getattr(fixed, name)
    _SCOPES[source.__name__] = scope
    # Local helpers use the same fixed imports as their surrounding oracle.
    for name, value in vars(source).items():
        if isinstance(value, FunctionType) and value.__module__ == source.__name__:
            cloned = FunctionType(
                value.__code__,
                scope,
                value.__name__,
                value.__defaults__,
                value.__closure__,
            )
            cloned.__kwdefaults__ = value.__kwdefaults__
            cloned.__dict__.update(value.__dict__)
            cloned.__module__ = __name__
            cloned.__dict__["pytestmark"] = [
                getattr(pytest.mark, mark.name)(
                    *(_replacement(arg) for arg in mark.args), **mark.kwargs
                )
                for mark in getattr(value, "pytestmark", [])
            ]
            scope[name] = cloned
            if (
                name.startswith("test_")
                and name
                != "test_rule_specific_conditions_do_not_hash_unrelated_family_syntax"
            ):
                # That oracle intentionally patches the mutable module by string path;
                # fixed syntax is instead tested directly by the pinned hash guards.
                globals()["test_frozen_" + prefix + "_" + name[5:]] = cloned


_collect(test_extract, "jp")
_collect(test_extract_en, "en")
_collect(test_validate, "validation")
_collect(test_template_parameters, "parameters")
_collect(test_template_rule_candidates, "rules")
# Direct detector/model oracles do not call the mutable FrozenTexts integration.
for _name, _function in vars(test_effect_presence).items():
    if (
        _name.startswith("test_")
        and "detect_presence" in _function.__code__.co_names
        and "card_from_raw" not in _function.__code__.co_names
    ):
        _scope = {
            name: _replacement(value)
            for name, value in vars(test_effect_presence).items()
        }
        _cloned = FunctionType(
            _function.__code__,
            _scope,
            _function.__name__,
            _function.__defaults__,
            _function.__closure__,
        )
        _cloned.__dict__.update(_function.__dict__)
        _cloned.__module__ = __name__
        globals()["test_frozen_presence_" + _name[5:]] = _cloned
