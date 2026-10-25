"""Expected fields transcribed from public parsed cards and adopted vocabulary.

Revision IDs are pinned production recipe results; expected payloads never read
the source builder. Official wording is represented only by its exact hash.
"""

from pydantic import JsonValue

FACTS = {
    "BP01-009": {
        "face": "f:8708790ca2995a0f9f9e5ffd1af52668",
        "revision": "rev:v1:c9b82c4e4e8df5c8ab2b4975c3058c7e0ffdd179e76429d4e47f4c557cac7304",
        "name": "120bb37dd3c163532f2e800c89f6b0bb01f8e71dd29ebdad759914bd07b15922",
        "effect": "a8887965f53a99dde02284d7deb217df4a20c000490c5fdc42330865aee829a3",
        "rules_name": "rn:v1:aab93bd9cce247eebd8d9d181087fad209ec33905c2bb4361a805940f651d543",
        "class": "elf",
    },
    "BP01-034": {
        "face": "f:f6426d02f6b05581af08c1b337f37632",
        "revision": "rev:v1:d3cc57e730c847bbcbf97443d513232a97a26aaaad9b2eeda520df24f20d675d",
        "name": "8dc5aec1537bf03bb6c4ae4d158b4e8d363e1d598d409ee083677e152f9f47eb",
        "effect": "86914ad53ab5adaa76c687ce4caf0685686cb5061c8c9e9d6f1ba5ced789656f",
        "rules_name": "rn:v1:99c1ae3239c1a9e301690abea6c5ec2e63ac81f65c37638a787cdf051b74d4e7",
        "class": "royal",
    },
    "BP01-092": {
        "face": "f:c94dd56ed9a45bbbbceb258790bc138b",
        "revision": "rev:v1:fe5518306017baa04f6086d060d5e2885880d45ae2a4a153f7f20839104a98ce",
        "name": "34e0765360ec010c8c26595d3f115b5b7b144066bb2b72c05be8c66ddb9c631c",
        "effect": "8cd3570fc03b55f778eb909ac8e496ed6e48afa8e039529e7b85de7bbd1a2695",
        "rules_name": "rn:v1:19103f4cea1dd087983f0d189a6b867ada7d8a02c0a26663826eddea8b91c2d4",
        "class": "dragon",
    },
}


def golden(number: str) -> dict[str, JsonValue]:
    fact = FACTS[number]
    stat = 3 if number == "BP01-009" else 1
    traits = {
        "BP01-009": ["elf", "princess"],
        "BP01-034": ["maid", "officer"],
        "BP01-092": ["wyrmkin"],
    }[number]
    return {
        "recipe": "effect-source-v1",
        "normalizer": "four-layer-jp-v10",
        "region": "jp",
        "faces": [
            {
                "face_id": fact["face"],
                "revision_id": fact["revision"],
                "texts": [
                    {
                        "field": "name",
                        "ordinal": None,
                        "state": "present",
                        "source_hash": fact["name"],
                    },
                    {
                        "field": "effect",
                        "ordinal": None,
                        "state": "present",
                        "source_hash": fact["effect"],
                    },
                ],
                "attributes": {
                    "class_code": {"state": "known", "value": fact["class"]},
                    "type_code": {"state": "known", "value": "follower"},
                    "cost": {"state": "known", "value": stat},
                    "attack": {"state": "known", "value": stat},
                    "defense": {"state": "known", "value": stat},
                    "traits": {"state": "known", "value": list[JsonValue](traits)},
                    "titles": {"state": "known", "value": []},
                    "special_kinds": {"state": "known", "value": []},
                },
                "rules_names": [
                    {"rules_name_id": fact["rules_name"], "role": "primary"}
                ],
            }
        ],
        "token_links": [],
    }
