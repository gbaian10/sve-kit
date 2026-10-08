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
        pytest.param(
            "https://example.com:8443/x",
            "https://example.com:8443/x",
            id="keep other port",
        ),
        pytest.param(f"{JP}/errata/#top", f"{JP}/errata/", id="drop fragment"),
        pytest.param(f"{JP}", f"{JP}/", id="empty path becomes slash"),
        pytest.param(
            "https://en.shadowverse-evolve.com/cards/?view=text&cardno=BP01-001EN&expansion=BP01",
            "https://en.shadowverse-evolve.com/cards/?cardno=BP01-001EN&expansion=BP01&view=text",
            id="sort query parameters",
        ),
        pytest.param(
            f"{JP}/cardlist/?cardno=BP03-LDⓈ01",
            f"{JP}/cardlist/?cardno=BP03-LD%E2%93%8801",
            id="encode Ⓢ in query",
        ),
        pytest.param(
            f"{JP}/wordpress/wp-content/images/cardlist/BP03/BP03-LDⓈ01EN.png",
            f"{JP}/wordpress/wp-content/images/cardlist/BP03/BP03-LD%E2%93%8801EN.png",
            id="encode Ⓢ in path",
        ),
        pytest.param(
            f"{JP}/wordpress/wp-content/images/cardlist/ETD01/etd01-002 .png",
            f"{JP}/wordpress/wp-content/images/cardlist/ETD01/etd01-002%20.png",
            id="encode space in path",
        ),
        pytest.param(f"{JP}/a%2fb", f"{JP}/a%2Fb", id="uppercase existing escape"),
        pytest.param(f"{JP}/?q=", f"{JP}/?q=", id="keep blank query value"),
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
