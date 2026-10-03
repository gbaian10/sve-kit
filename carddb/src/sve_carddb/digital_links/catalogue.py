"""Complete frozen digital catalogues shared by triage and policy evaluation."""

from collections import defaultdict
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

from sve_carddb.digital_links.evidence import Name, PageRef, batch_refs, inventory
from sve_carddb.snapshot.values import array, canonical, object_value

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import ReviewContext
    from sve_carddb.translations.sources import Sources


def complete_inventory(  # ruff: ignore[complex-structure] -- provider-specific pagination and whole-page uniqueness are checked together
    sources: Sources, review: ReviewContext, game: str
) -> dict[tuple[str, str, str, str], Name]:
    """Use every frozen page; a selected-target subset cannot prove uniqueness."""
    batch_refs(sources, review.source_batches, game)
    refs = []
    counts: dict[str, set[int]] = defaultdict(set)
    offsets: dict[str, set[int]] = defaultdict(set)
    identifiers: dict[str, set[str]] = defaultdict(set)
    batches = {(b.store_id, b.batch_id) for b in review.source_batches}
    for (store, batch, version, parser), (
        lang,
        document,
        source,
    ) in sources.cache.items():
        if (store, batch) not in batches or parser != "translation-" + game + "-v1":
            continue
        data = object_value(object_value(document)["data"])
        refs.append(
            PageRef(
                store_id=store, batch_id=batch, source_version_id=version, parser=parser
            )
        )
        query = parse_qs(urlsplit(source.url).query)
        if game == "sv1" and (
            set(query) != {"format", "lang"} or query["format"] != ["json"]
        ):
            raise ValueError("Digital candidate catalogue uses filtered URL")
        if game == "svwb" and (
            set(query) != {"include_token", "lang", "offset"}
            or query["include_token"] != ["1"]
        ):
            raise ValueError("Digital candidate catalogue uses filtered URL")
        if game == "svwb":
            count = data.get("count")
            query = parse_qs(urlsplit(source.url).query)
            offset = query.get("offset", [])
            if (
                type(count) is not int
                or count < 1
                or len(offset) != 1
                or not offset[0].isascii()
                or not offset[0].isdecimal()
            ):
                raise ValueError("Digital candidate catalogue pagination is invalid")
            counts[lang].add(count)
            offsets[lang].add(int(offset[0]))
            identifiers[lang].update(object_value(data["card_details"]))
        else:
            ids = [object_value(c).get("card_id") for c in array(data.get("cards"))]
            if len({canonical(identifier) for identifier in ids}) != len(ids):
                raise ValueError("Digital catalogue repeats an ID within a page")
    if not refs:
        raise ValueError("Digital candidate catalogue is absent")
    names = inventory(sources, tuple(refs))
    if game == "svwb" and any(
        len(counts[lang]) != 1
        or len(identifiers[lang]) != next(iter(counts[lang]))
        or offsets[lang] != set(range(0, next(iter(counts[lang])), 30))
        for lang in counts
    ):
        raise ValueError("Digital candidate catalogue page closure is incomplete")
    if not any(key[3] == "ja" for key in names):
        raise ValueError("Digital candidate catalogue lacks Japanese")
    return names
