"""Author reference atoms, not complete action-position types or scope admission."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.contracts.author_leaves import Phase
from sve_carddb.core.models import RecordData

BindingName = Annotated[
    str,
    Field(
        pattern=r"^(?!(?:this|it|item|affected|found|entering|attack_target|host|cost1|self|opp|turn|non_turn|current|opponents|each|other_players)$(?![\s\S]))[A-Za-z][A-Za-z0-9_]*$(?![\s\S])"
    ),
]
RoleName = Annotated[str, Field(pattern=r"^[A-Za-z][A-Za-z0-9_]*$(?![\s\S])")]
ReservedRefName = Literal[
    "this",
    "it",
    "item",
    "affected",
    "found",
    "entering",
    "attack_target",
    "host",
    "cost1",
]
ReservedPlayerName = Literal["self", "opp", "turn", "non_turn", "current", "item"]
PlayerSetName = Literal["opponents", "each", "other_players"]
Edge = Literal["begin", "end"]


class EventMoveReceipt(RecordData):
    event: Literal["move_receipt"]


class EntryPaymentRef(RecordData):
    entry_payment: Literal["additional"]


type FollowSource = BindingName | EventMoveReceipt
type ReceiptRef = FollowSource | EntryPaymentRef


class FollowRef(RecordData):
    follow: FollowSource


class RoleSelection(RecordData):
    of: BindingName
    name: RoleName


class RoleRef(RecordData):
    role: RoleSelection


class EventObjectRef(RecordData):
    event: Literal["holder"]


type Ref = ReservedRefName | BindingName | FollowRef | RoleRef | EventObjectRef


class OwnerRef(RecordData):
    owner_of: Ref


class MasterRef(RecordData):
    master_of: Ref


type PlayerRef = ReservedPlayerName | BindingName | OwnerRef | MasterRef
type PlayerSet = PlayerSetName


class SingleLeader(RecordData):
    leader: Literal["self", "opp"]


class MultipleLeaders(RecordData):
    leaders: Literal["opponents", "all"]


class LeaderOf(RecordData):
    leader_of: Ref


type LeaderRecipient = SingleLeader | MultipleLeaders | LeaderOf


class PhaseEdge(RecordData):
    who: PlayerRef
    phase: Phase
    edge: Edge


class TurnEdge(RecordData):
    who: PlayerRef
    edge: Edge


class PhaseInterval(RecordData):
    who: PlayerRef
    phase: Phase


class UntilNextPhase(RecordData):
    next_phase: PhaseEdge


class UntilNextTurn(RecordData):
    next_turn: TurnEdge


class UntilDuringNextPhase(RecordData):
    during_next_phase: PhaseInterval


class TurnInterval(RecordData):
    who: PlayerRef


class UntilDuringNextTurn(RecordData):
    during_next_turn: TurnInterval


type Until = (
    Literal["turn", "next_opp_turn", "this_and_next_opp_turn", "game"]
    | UntilNextPhase
    | UntilNextTurn
    | UntilDuringNextPhase
    | UntilDuringNextTurn
)
