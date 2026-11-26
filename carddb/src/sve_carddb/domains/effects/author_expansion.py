"""Load the schema resources for author expansion."""

from typing import TYPE_CHECKING

from sve_carddb.domains.effects.author_inputs import AuthorSchemas

if TYPE_CHECKING:
    from pathlib import Path


class AuthorExpander:
    def __init__(self, schema_directory: Path) -> None:
        self.schemas = AuthorSchemas(schema_directory)
