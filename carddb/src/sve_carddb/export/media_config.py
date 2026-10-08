"""Fixed offline endpoint configuration for the 2.0 candidate producer."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.export.project.source import Record


def offline_configuration(config: Record) -> Record:
    """Expose URL templates without claiming a service health check or DB adoption."""
    return config | {
        "digital_endpoints": [
            {
                "game": "sv1",
                "card_url_template": "https://shadowverse-portal.com/card/{official_id}?lang={provider_lang}",
                "language_map": {"ja": "ja", "en": "en", "zh-Hant": "zh-tw"},
                "status": "unknown",
                "refresh_policy": "frozen",
            },
            {
                "game": "svwb",
                "card_url_template": "https://shadowverse-wb.com/{provider_lang}/deck/cardslist/card/?card_id={official_id}",
                "language_map": {"ja": "ja", "en": "en", "zh-Hant": "cht"},
                "status": "unknown",
                "refresh_policy": "on_sve_release",
            },
        ]
    }
