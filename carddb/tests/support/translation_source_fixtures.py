"""Trusted source adapter for the shared fixture's sealed synthetic JSON envelopes."""

from typing import TYPE_CHECKING, override

from sve_carddb.core.json import parse
from sve_carddb.domains.translations.sources import Sources

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.core.provenance import BuildContext, Source


class EnvelopeSources(Sources):
    @override
    def stage(self, build: BuildContext) -> EnvelopeSources:
        return EnvelopeSources(
            self.stores,
            self.repository,
            build,
            self.identities.current_registry,
            batches=self.batches,
            corrections=self.corrections,
        )

    @override
    def projection(
        self, batch_id: str, version: str, parser: str
    ) -> tuple[str, JsonValue, Source]:
        providers = {"translation-jp-v1": "jp", "translation-en-v1": "en"}
        provider = providers.get(parser)
        if provider is None:
            raise ValueError("Unsupported synthetic translation parser")
        source, raw, descriptor = self.batch(batch_id).read(
            version, parser_version=parser
        )
        if (descriptor.provider, descriptor.kind) != (provider, "card"):
            raise ValueError("Synthetic translation provider/kind mismatch")
        assert raw.startswith(b"<html>")
        assert raw.endswith(b"</html>")
        return ("ja" if provider == "jp" else "en"), parse(raw[6:-7]), source
