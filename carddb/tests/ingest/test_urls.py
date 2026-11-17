import pytest

from sve_carddb.ingest.urls import canonicalize

JP = "https://shadowverse-evolve.com"


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        pytest.param(
            f"{JP}/cardlist/?cardno=BP01-001",
            f"{JP}/cardlist/?cardno=BP01-001",
            id="already canonical",
        ),
        pytest.param(
            "HTTPS://Shadowverse-Evolve.COM/cardlist/",
            f"{JP}/cardlist/",
            id="lowercase scheme and host",
        ),
        pytest.param(f"{JP}:443/rules/", f"{JP}/rules/", id="drop default port"),
    ],
)
def test_canonicalize(url: str, expected: str) -> None:
    assert canonicalize(url) == expected


@pytest.mark.parametrize(
    "url",
    [
        f"{JP}/cardlist/?cardno=BP03-LDⓈ01",
        f"{JP}/wordpress/wp-content/images/cardlist/ETD01/etd01-002 .png",
    ],
)
def test_canonicalize_is_idempotent(url: str) -> None:
    once = canonicalize(url)
    assert canonicalize(once) == once


@pytest.mark.parametrize(
    "url", ["ftp://example.com/x", "/relative/path", "https:///no-host"]
)
def test_canonicalize_rejects_non_http_urls(url: str) -> None:
    with pytest.raises(ValueError, match="URL"):
        canonicalize(url)
