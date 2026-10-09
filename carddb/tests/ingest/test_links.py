import pytest

from sve_carddb.ingest.archive.manifest import Kind, Link, Manifest, ManifestError

CARD = "https://shadowverse-evolve.com/cardlist/?cardno=BP08-003"
IMG = "https://shadowverse-evolve.com/wordpress/wp-content/images/cardlist/BP08"
FRONT = Link(f"{IMG}/bp08-003_front.png", Kind.IMAGE, 0, "/images/bp08-003_front.png")
BACK = Link(f"{IMG}/bp08-003_back.png", Kind.IMAGE, 1, "/images/bp08-003_back.png")


def replace(manifest: Manifest, links: list[Link], sha: str = "v1") -> None:
    with manifest.transaction():
        manifest.links.replace(CARD, sha, links)


def test_replace_sets_current_links_in_order(manifest: Manifest) -> None:
    replace(manifest, [FRONT, BACK])
    assert manifest.links.current(CARD) == [FRONT, BACK]
    assert manifest.links.history(CARD) == [("added", FRONT), ("added", BACK)]


def test_replace_logs_removed_and_added_links(manifest: Manifest) -> None:
    replace(manifest, [FRONT, BACK])
    new_back = Link(
        f"{IMG}/bp08-003_ura.png", Kind.IMAGE, 1, "/images/bp08-003_ura.png"
    )
    replace(manifest, [FRONT, new_back], sha="v2")
    assert manifest.links.current(CARD) == [FRONT, new_back]
    assert manifest.links.history(CARD)[2:] == [("removed", BACK), ("added", new_back)]


def test_reordering_is_logged_as_remove_and_add(manifest: Manifest) -> None:
    replace(manifest, [FRONT, BACK])
    swapped = [
        Link(BACK.to_url, Kind.IMAGE, 0, BACK.original),
        Link(FRONT.to_url, Kind.IMAGE, 1, FRONT.original),
    ]
    replace(manifest, swapped, sha="v2")
    assert manifest.links.current(CARD) == swapped
    events = [event for event, _ in manifest.links.history(CARD)[2:]]
    assert events == ["removed", "removed", "added", "added"]


def test_replace_with_nothing_removes_all(manifest: Manifest) -> None:
    replace(manifest, [FRONT])
    replace(manifest, [], sha="v2")
    assert manifest.links.current(CARD) == []
    assert manifest.links.history(CARD)[-1] == ("removed", FRONT)


def test_replace_rejects_duplicates(manifest: Manifest) -> None:
    with pytest.raises(ManifestError, match="duplicate"):
        replace(manifest, [FRONT, FRONT])
    assert manifest.links.current(CARD) == []
