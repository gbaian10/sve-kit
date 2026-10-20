"""Target role declarations, not complete selectors or binding admission."""

from typing import Annotated, Literal

from pydantic import BeforeValidator, Field

from sve_carddb.contracts.author_references import RoleName
from sve_carddb.core.json import integer
from sve_carddb.core.models import RecordData


class TargetRoleOne(RecordData):
    # Literal alone accepts true as 1 even in Pydantic's strict mode.
    if_nonempty: Annotated[Literal[1], BeforeValidator(integer)]


type TargetRoleValue = TargetRoleOne | Literal["remaining"]
type TargetRoles = Annotated[dict[RoleName, TargetRoleValue], Field(min_length=1)]
