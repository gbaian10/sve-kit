"""YAML emission for synthetic fixture edits."""

from ruamel.yaml import YAML


def yaml_emitter() -> YAML:
    yaml = YAML(typ="safe", pure=True)
    yaml.version = (1, 2)
    return yaml
