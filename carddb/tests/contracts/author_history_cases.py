"""Explicit correspondence for event and history node contracts."""

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_queries as author

CONFIG = ConfigDict(regex_engine="python-re")
HISTORY_NAMES = {
    "A.expr.orig-cost-of": "ExprOrigCostOf",
    "A.expr.event-number": "ExprEventNumber",
    "A.expr.dice": "ExprDice",
    "A.expr.x-paid": "ExprXPaid",
    "A.lki-of": "LkiOf",
    "A.paid-receipt": "PaidReceipt",
    "A.cond.paid": "CondPaid",
    "A.expr.player": "PlayerExpr",
    "A.cond.player-cmp-payload": "PlayerCmpPayload",
    "A.event-subject-filter": "EventSubjectFilter",
    "A.event-object-sel": "EventObjectSel",
    "A.event-recipient-atom": "EventRecipientAtom",
    "A.event-recipient-union": "EventRecipientUnion",
    "A.event-recipient": "EventRecipient",
    "A.chosen-payload": "ChosenPayload",
    "A.cond.chosen": "CondChosen",
    "A.during": "During",
    "A.event-by": "EventBy",
    "A.move-origin": "MoveOrigin",
    "A.nat-eq": "NatEq",
    "A.nat-le": "NatLe",
    "A.nat-ge": "NatGe",
    "A.nat-threshold": "NatThreshold",
    "A.die-eq": "DieEq",
    "A.die-le": "DieLe",
    "A.die-ge": "DieGe",
    "A.die-threshold": "DieThreshold",
    "A.since-window": "SinceWindow",
    "A.history-window": "HistoryWindow",
    "A.event.enter-field-payload": "EventEnterFieldPayload",
    "A.event.enter-field": "EventEnterField",
    "A.history.enter-field-where": "EnterFieldHistoryWhere",
    "A.history.enter-field": "HistoryEnterField",
    "A.history.enter-field-condition": "HistoryEnterFieldCondition",
    "A.event.left-field-payload": "EventLeftFieldPayload",
    "A.event.left-field": "EventLeftField",
    "A.history.left-field-where": "LeftFieldHistoryWhere",
    "A.history.left-field": "HistoryLeftField",
    "A.history.left-field-condition": "HistoryLeftFieldCondition",
    "A.event.placed-ex-payload": "EventPlacedExPayload",
    "A.event.placed-ex": "EventPlacedEx",
    "A.history.placed-ex-where": "PlacedExHistoryWhere",
    "A.history.placed-ex": "HistoryPlacedEx",
    "A.history.placed-ex-condition": "HistoryPlacedExCondition",
    "A.event.discarded-payload": "EventDiscardedPayload",
    "A.event.discarded": "EventDiscarded",
    "A.history.discarded-where": "DiscardedHistoryWhere",
    "A.history.discarded": "HistoryDiscarded",
    "A.history.discarded-condition": "HistoryDiscardedCondition",
    "A.event.banished-payload": "EventBanishedPayload",
    "A.event.banished": "EventBanished",
    "A.history.banished-where": "BanishedHistoryWhere",
    "A.history.banished": "HistoryBanished",
    "A.history.banished-condition": "HistoryBanishedCondition",
    "A.event.destroyed-payload": "EventDestroyedPayload",
    "A.event.destroyed": "EventDestroyed",
    "A.history.destroyed-where": "DestroyedHistoryWhere",
    "A.history.destroyed": "HistoryDestroyed",
    "A.history.destroyed-condition": "HistoryDestroyedCondition",
    "A.event.damaged-payload": "EventDamagedPayload",
    "A.event.damaged": "EventDamaged",
    "A.history.damaged-where": "DamagedHistoryWhere",
    "A.history.damaged": "HistoryDamaged",
    "A.history.damaged-condition": "HistoryDamagedCondition",
    "A.history.damaged-sum": "HistoryDamagedSum",
    "A.event.dealt-damage-payload": "EventDealtDamagePayload",
    "A.event.dealt-damage": "EventDealtDamage",
    "A.history.dealt-damage-where": "DealtDamageHistoryWhere",
    "A.history.dealt-damage": "HistoryDealtDamage",
    "A.history.dealt-damage-condition": "HistoryDealtDamageCondition",
    "A.history.dealt-damage-sum": "HistoryDealtDamageSum",
    "A.event.attacked-payload": "EventAttackedPayload",
    "A.event.attacked": "EventAttacked",
    "A.history.attacked-where": "AttackedHistoryWhere",
    "A.history.attacked": "HistoryAttacked",
    "A.history.attacked-condition": "HistoryAttackedCondition",
    "A.event.attack-target-payload": "EventAttackTargetPayload",
    "A.event.attack-target": "EventAttackTarget",
    "A.history.attack-target-where": "AttackTargetHistoryWhere",
    "A.history.attack-target": "HistoryAttackTarget",
    "A.history.attack-target-condition": "HistoryAttackTargetCondition",
    "A.event.evolved-payload": "EventEvolvedPayload",
    "A.event.evolved": "EventEvolved",
    "A.history.evolved-where": "EvolvedHistoryWhere",
    "A.history.evolved": "HistoryEvolved",
    "A.history.evolved-condition": "HistoryEvolvedCondition",
    "A.event.super-evolved-payload": "EventSuperEvolvedPayload",
    "A.event.super-evolved": "EventSuperEvolved",
    "A.history.super-evolved-where": "SuperEvolvedHistoryWhere",
    "A.history.super-evolved": "HistorySuperEvolved",
    "A.history.super-evolved-condition": "HistorySuperEvolvedCondition",
    "A.event.phase-payload": "EventPhasePayload",
    "A.event.phase": "EventPhase",
    "A.history.phase-where": "PhaseHistoryWhere",
    "A.history.phase": "HistoryPhase",
    "A.history.phase-condition": "HistoryPhaseCondition",
    "A.event.played-payload": "EventPlayedPayload",
    "A.event.played": "EventPlayed",
    "A.history.played-where": "PlayedHistoryWhere",
    "A.history.played": "HistoryPlayed",
    "A.history.played-condition": "HistoryPlayedCondition",
    "A.event.ability-played-payload": "EventAbilityPlayedPayload",
    "A.event.ability-played": "EventAbilityPlayed",
    "A.history.ability-played-where": "AbilityPlayedHistoryWhere",
    "A.history.ability-played": "HistoryAbilityPlayed",
    "A.history.ability-played-condition": "HistoryAbilityPlayedCondition",
    "A.event.life-changed-payload": "EventLifeChangedPayload",
    "A.event.life-changed": "EventLifeChanged",
    "A.history.life-changed-where": "LifeChangedHistoryWhere",
    "A.history.life-changed": "HistoryLifeChanged",
    "A.history.life-changed-condition": "HistoryLifeChangedCondition",
    "A.history.life-sum-where": "LifeSumWhere",
    "A.history.life-changed-sum": "HistoryLifeChangedSum",
    "A.event.stat-changed-payload": "EventStatChangedPayload",
    "A.event.stat-changed": "EventStatChanged",
    "A.history.stat-changed-where": "StatChangedHistoryWhere",
    "A.history.stat-changed": "HistoryStatChanged",
    "A.history.stat-changed-condition": "HistoryStatChangedCondition",
    "A.event.counter-changed-payload": "EventCounterChangedPayload",
    "A.event.counter-changed": "EventCounterChanged",
    "A.history.counter-changed-where": "CounterChangedHistoryWhere",
    "A.history.counter-changed": "HistoryCounterChanged",
    "A.history.counter-changed-condition": "HistoryCounterChangedCondition",
    "A.event.acted-payload": "EventActedPayload",
    "A.event.acted": "EventActed",
    "A.history.acted-where": "ActedHistoryWhere",
    "A.history.acted": "HistoryActed",
    "A.history.acted-condition": "HistoryActedCondition",
    "A.event.stood-payload": "EventStoodPayload",
    "A.event.stood": "EventStood",
    "A.history.stood-where": "StoodHistoryWhere",
    "A.history.stood": "HistoryStood",
    "A.history.stood-condition": "HistoryStoodCondition",
    "A.event.raced-payload": "EventRacedPayload",
    "A.event.raced": "EventRaced",
    "A.history.raced-where": "RacedHistoryWhere",
    "A.history.raced": "HistoryRaced",
    "A.history.raced-condition": "HistoryRacedCondition",
    "A.event.drive-gained-payload": "EventDriveGainedPayload",
    "A.event.drive-gained": "EventDriveGained",
    "A.history.drive-gained-where": "DriveGainedHistoryWhere",
    "A.history.drive-gained": "HistoryDriveGained",
    "A.history.drive-gained-condition": "HistoryDriveGainedCondition",
    "A.event.ub-activated-payload": "EventUbActivatedPayload",
    "A.event.ub-activated": "EventUbActivated",
    "A.history.ub-activated-where": "UbActivatedHistoryWhere",
    "A.history.ub-activated": "HistoryUbActivated",
    "A.history.ub-activated-condition": "HistoryUbActivatedCondition",
    "A.event.fused-payload": "EventFusedPayload",
    "A.event.fused": "EventFused",
    "A.history.fused-where": "FusedHistoryWhere",
    "A.history.fused": "HistoryFused",
    "A.history.fused-condition": "HistoryFusedCondition",
    "A.event.targeted-payload": "EventTargetedPayload",
    "A.event.targeted": "EventTargeted",
    "A.history.targeted-where": "TargetedHistoryWhere",
    "A.history.targeted": "HistoryTargeted",
    "A.history.targeted-condition": "HistoryTargetedCondition",
    "A.event.dice-rolled-payload": "EventDiceRolledPayload",
    "A.event.dice-rolled": "EventDiceRolled",
    "A.history.dice-rolled-where": "DiceRolledHistoryWhere",
    "A.history.dice-rolled": "HistoryDiceRolled",
    "A.history.dice-rolled-condition": "HistoryDiceRolledCondition",
    "A.event.drive-checked-payload": "EventDriveCheckedPayload",
    "A.event.drive-checked": "EventDriveChecked",
    "A.history.drive-checked-where": "DriveCheckedHistoryWhere",
    "A.history.drive-checked": "HistoryDriveChecked",
    "A.history.drive-checked-condition": "HistoryDriveCheckedCondition",
    "A.event.drive-trigger-payload": "EventDriveTriggerPayload",
    "A.event.drive-trigger": "EventDriveTrigger",
    "A.history.drive-trigger": "HistoryDriveTrigger",
    "A.history.drive-trigger-condition": "HistoryDriveTriggerCondition",
    "A.event.moved-payload": "EventMovedPayload",
    "A.event.moved": "EventMoved",
    "A.history.moved-where": "MovedHistoryWhere",
    "A.history.moved": "HistoryMoved",
    "A.history.moved-condition": "HistoryMovedCondition",
    "A.event.drew-payload": "EventDrewPayload",
    "A.event.drew": "EventDrew",
    "A.history.drew-where": "DrewHistoryWhere",
    "A.history.drew": "HistoryDrew",
    "A.history.drew-condition": "HistoryDrewCondition",
    "A.event.equipped-payload": "EventEquippedPayload",
    "A.event.equipped": "EventEquipped",
    "A.history.equipped-where": "EquippedHistoryWhere",
    "A.history.equipped": "HistoryEquipped",
    "A.history.equipped-condition": "HistoryEquippedCondition",
    "A.event.fused-as-material-payload": "EventFusedAsMaterialPayload",
    "A.event.fused-as-material": "EventFusedAsMaterial",
    "A.history.fused-as-material-where": "FusedAsMaterialHistoryWhere",
    "A.history.fused-as-material": "HistoryFusedAsMaterial",
    "A.history.fused-as-material-condition": "HistoryFusedAsMaterialCondition",
    "A.history.payment-where": "PaymentHistoryWhere",
    "A.history.payment": "HistoryPayment",
    "A.history.payment-condition": "HistoryPaymentCondition",
    "A.history.payment-sum": "HistoryPaymentSum",
    "A.event.counter-reached-payload": "CounterReachedPayload",
    "A.event.counter-reached": "EventCounterReached",
    "A.event.state": "EventState",
    "A.event-atom": "EventAtom",
    "A.event.any-of": "EventAnyOf",
    "A.event": "EventPattern",
    "A.history-query": "HistoryQuery",
    "A.history-condition-query": "HistoryConditionQuery",
    "A.history-sum-query": "HistorySumQuery",
    "A.cond.history": "CondHistory",
    "A.expr.history-count": "ExprHistoryCount",
    "A.expr.history-sum": "ExprHistorySum",
    "A.capture.power-of": "CapturedPowerOf",
    "A.capture.hp-of": "CapturedHpOf",
    "A.capture.cost-of": "CapturedCostOf",
    "A.capture.orig-cost-of": "CapturedOrigCostOf",
    "A.state-numeric-read": "StateNumericRead",
    "A.capture.timed": "TimedCapture",
    "A.capture.selection": "SelectionCapture",
    "A.expr.capture": "CaptureRead",
}
HISTORY: dict[str, TypeAdapter[object]] = {
    "A.expr.orig-cost-of": TypeAdapter(author.ExprOrigCostOf),
    "A.expr.event-number": TypeAdapter(author.ExprEventNumber),
    "A.expr.dice": TypeAdapter(author.ExprDice),
    "A.expr.x-paid": TypeAdapter(author.ExprXPaid),
    "A.lki-of": TypeAdapter(author.LkiOf),
    "A.paid-receipt": TypeAdapter(author.PaidReceipt),
    "A.cond.paid": TypeAdapter(author.CondPaid),
    "A.expr.player": TypeAdapter(author.PlayerExpr),
    "A.cond.player-cmp-payload": TypeAdapter(author.PlayerCmpPayload),
    "A.event-subject-filter": TypeAdapter(author.EventSubjectFilter, config=CONFIG),
    "A.event-object-sel": TypeAdapter(author.EventObjectSel, config=CONFIG),
    "A.event-recipient-atom": TypeAdapter(author.EventRecipientAtom, config=CONFIG),
    "A.event-recipient-union": TypeAdapter(author.EventRecipientUnion, config=CONFIG),
    "A.event-recipient": TypeAdapter(author.EventRecipient, config=CONFIG),
    "A.chosen-payload": TypeAdapter(author.ChosenPayload, config=CONFIG),
    "A.cond.chosen": TypeAdapter(author.CondChosen),
    "A.during": TypeAdapter(author.During),
    "A.event-by": TypeAdapter(author.EventBy),
    "A.move-origin": TypeAdapter(author.MoveOrigin),
    "A.nat-eq": TypeAdapter(author.NatEq),
    "A.nat-le": TypeAdapter(author.NatLe),
    "A.nat-ge": TypeAdapter(author.NatGe),
    "A.nat-threshold": TypeAdapter(author.NatThreshold, config=CONFIG),
    "A.die-eq": TypeAdapter(author.DieEq),
    "A.die-le": TypeAdapter(author.DieLe),
    "A.die-ge": TypeAdapter(author.DieGe),
    "A.die-threshold": TypeAdapter(author.DieThreshold, config=CONFIG),
    "A.since-window": TypeAdapter(author.SinceWindow),
    "A.history-window": TypeAdapter(author.HistoryWindow, config=CONFIG),
    "A.event.enter-field-payload": TypeAdapter(
        author.EventEnterFieldPayload, config=CONFIG
    ),
    "A.event.enter-field": TypeAdapter(author.EventEnterField),
    "A.history.enter-field-where": TypeAdapter(
        author.EnterFieldHistoryWhere, config=CONFIG
    ),
    "A.history.enter-field": TypeAdapter(author.HistoryEnterField, config=CONFIG),
    "A.history.enter-field-condition": TypeAdapter(
        author.HistoryEnterFieldCondition, config=CONFIG
    ),
    "A.event.left-field-payload": TypeAdapter(
        author.EventLeftFieldPayload, config=CONFIG
    ),
    "A.event.left-field": TypeAdapter(author.EventLeftField),
    "A.history.left-field-where": TypeAdapter(
        author.LeftFieldHistoryWhere, config=CONFIG
    ),
    "A.history.left-field": TypeAdapter(author.HistoryLeftField, config=CONFIG),
    "A.history.left-field-condition": TypeAdapter(
        author.HistoryLeftFieldCondition, config=CONFIG
    ),
    "A.event.placed-ex-payload": TypeAdapter(
        author.EventPlacedExPayload, config=CONFIG
    ),
    "A.event.placed-ex": TypeAdapter(author.EventPlacedEx),
    "A.history.placed-ex-where": TypeAdapter(
        author.PlacedExHistoryWhere, config=CONFIG
    ),
    "A.history.placed-ex": TypeAdapter(author.HistoryPlacedEx, config=CONFIG),
    "A.history.placed-ex-condition": TypeAdapter(
        author.HistoryPlacedExCondition, config=CONFIG
    ),
    "A.event.discarded-payload": TypeAdapter(
        author.EventDiscardedPayload, config=CONFIG
    ),
    "A.event.discarded": TypeAdapter(author.EventDiscarded),
    "A.history.discarded-where": TypeAdapter(
        author.DiscardedHistoryWhere, config=CONFIG
    ),
    "A.history.discarded": TypeAdapter(author.HistoryDiscarded, config=CONFIG),
    "A.history.discarded-condition": TypeAdapter(
        author.HistoryDiscardedCondition, config=CONFIG
    ),
    "A.event.banished-payload": TypeAdapter(author.EventBanishedPayload, config=CONFIG),
    "A.event.banished": TypeAdapter(author.EventBanished),
    "A.history.banished-where": TypeAdapter(author.BanishedHistoryWhere, config=CONFIG),
    "A.history.banished": TypeAdapter(author.HistoryBanished, config=CONFIG),
    "A.history.banished-condition": TypeAdapter(
        author.HistoryBanishedCondition, config=CONFIG
    ),
    "A.event.destroyed-payload": TypeAdapter(
        author.EventDestroyedPayload, config=CONFIG
    ),
    "A.event.destroyed": TypeAdapter(author.EventDestroyed),
    "A.history.destroyed-where": TypeAdapter(
        author.DestroyedHistoryWhere, config=CONFIG
    ),
    "A.history.destroyed": TypeAdapter(author.HistoryDestroyed, config=CONFIG),
    "A.history.destroyed-condition": TypeAdapter(
        author.HistoryDestroyedCondition, config=CONFIG
    ),
    "A.event.damaged-payload": TypeAdapter(author.EventDamagedPayload, config=CONFIG),
    "A.event.damaged": TypeAdapter(author.EventDamaged),
    "A.history.damaged-where": TypeAdapter(author.DamagedHistoryWhere, config=CONFIG),
    "A.history.damaged": TypeAdapter(author.HistoryDamaged, config=CONFIG),
    "A.history.damaged-condition": TypeAdapter(
        author.HistoryDamagedCondition, config=CONFIG
    ),
    "A.history.damaged-sum": TypeAdapter(author.HistoryDamagedSum, config=CONFIG),
    "A.event.dealt-damage-payload": TypeAdapter(
        author.EventDealtDamagePayload, config=CONFIG
    ),
    "A.event.dealt-damage": TypeAdapter(author.EventDealtDamage),
    "A.history.dealt-damage-where": TypeAdapter(
        author.DealtDamageHistoryWhere, config=CONFIG
    ),
    "A.history.dealt-damage": TypeAdapter(author.HistoryDealtDamage, config=CONFIG),
    "A.history.dealt-damage-condition": TypeAdapter(
        author.HistoryDealtDamageCondition, config=CONFIG
    ),
    "A.history.dealt-damage-sum": TypeAdapter(
        author.HistoryDealtDamageSum, config=CONFIG
    ),
    "A.event.attacked-payload": TypeAdapter(author.EventAttackedPayload, config=CONFIG),
    "A.event.attacked": TypeAdapter(author.EventAttacked),
    "A.history.attacked-where": TypeAdapter(author.AttackedHistoryWhere, config=CONFIG),
    "A.history.attacked": TypeAdapter(author.HistoryAttacked, config=CONFIG),
    "A.history.attacked-condition": TypeAdapter(
        author.HistoryAttackedCondition, config=CONFIG
    ),
    "A.event.attack-target-payload": TypeAdapter(
        author.EventAttackTargetPayload, config=CONFIG
    ),
    "A.event.attack-target": TypeAdapter(author.EventAttackTarget),
    "A.history.attack-target-where": TypeAdapter(
        author.AttackTargetHistoryWhere, config=CONFIG
    ),
    "A.history.attack-target": TypeAdapter(author.HistoryAttackTarget, config=CONFIG),
    "A.history.attack-target-condition": TypeAdapter(
        author.HistoryAttackTargetCondition, config=CONFIG
    ),
    "A.event.evolved-payload": TypeAdapter(author.EventEvolvedPayload, config=CONFIG),
    "A.event.evolved": TypeAdapter(author.EventEvolved),
    "A.history.evolved-where": TypeAdapter(author.EvolvedHistoryWhere, config=CONFIG),
    "A.history.evolved": TypeAdapter(author.HistoryEvolved, config=CONFIG),
    "A.history.evolved-condition": TypeAdapter(
        author.HistoryEvolvedCondition, config=CONFIG
    ),
    "A.event.super-evolved-payload": TypeAdapter(
        author.EventSuperEvolvedPayload, config=CONFIG
    ),
    "A.event.super-evolved": TypeAdapter(author.EventSuperEvolved),
    "A.history.super-evolved-where": TypeAdapter(
        author.SuperEvolvedHistoryWhere, config=CONFIG
    ),
    "A.history.super-evolved": TypeAdapter(author.HistorySuperEvolved, config=CONFIG),
    "A.history.super-evolved-condition": TypeAdapter(
        author.HistorySuperEvolvedCondition, config=CONFIG
    ),
    "A.event.phase-payload": TypeAdapter(author.EventPhasePayload, config=CONFIG),
    "A.event.phase": TypeAdapter(author.EventPhase),
    "A.history.phase-where": TypeAdapter(author.PhaseHistoryWhere, config=CONFIG),
    "A.history.phase": TypeAdapter(author.HistoryPhase, config=CONFIG),
    "A.history.phase-condition": TypeAdapter(
        author.HistoryPhaseCondition, config=CONFIG
    ),
    "A.event.played-payload": TypeAdapter(author.EventPlayedPayload, config=CONFIG),
    "A.event.played": TypeAdapter(author.EventPlayed),
    "A.history.played-where": TypeAdapter(author.PlayedHistoryWhere, config=CONFIG),
    "A.history.played": TypeAdapter(author.HistoryPlayed, config=CONFIG),
    "A.history.played-condition": TypeAdapter(
        author.HistoryPlayedCondition, config=CONFIG
    ),
    "A.event.ability-played-payload": TypeAdapter(
        author.EventAbilityPlayedPayload, config=CONFIG
    ),
    "A.event.ability-played": TypeAdapter(author.EventAbilityPlayed),
    "A.history.ability-played-where": TypeAdapter(
        author.AbilityPlayedHistoryWhere, config=CONFIG
    ),
    "A.history.ability-played": TypeAdapter(author.HistoryAbilityPlayed, config=CONFIG),
    "A.history.ability-played-condition": TypeAdapter(
        author.HistoryAbilityPlayedCondition, config=CONFIG
    ),
    "A.event.life-changed-payload": TypeAdapter(
        author.EventLifeChangedPayload, config=CONFIG
    ),
    "A.event.life-changed": TypeAdapter(author.EventLifeChanged),
    "A.history.life-changed-where": TypeAdapter(
        author.LifeChangedHistoryWhere, config=CONFIG
    ),
    "A.history.life-changed": TypeAdapter(author.HistoryLifeChanged, config=CONFIG),
    "A.history.life-changed-condition": TypeAdapter(
        author.HistoryLifeChangedCondition, config=CONFIG
    ),
    "A.history.life-sum-where": TypeAdapter(author.LifeSumWhere, config=CONFIG),
    "A.history.life-changed-sum": TypeAdapter(
        author.HistoryLifeChangedSum, config=CONFIG
    ),
    "A.event.stat-changed-payload": TypeAdapter(
        author.EventStatChangedPayload, config=CONFIG
    ),
    "A.event.stat-changed": TypeAdapter(author.EventStatChanged),
    "A.history.stat-changed-where": TypeAdapter(
        author.StatChangedHistoryWhere, config=CONFIG
    ),
    "A.history.stat-changed": TypeAdapter(author.HistoryStatChanged, config=CONFIG),
    "A.history.stat-changed-condition": TypeAdapter(
        author.HistoryStatChangedCondition, config=CONFIG
    ),
    "A.event.counter-changed-payload": TypeAdapter(
        author.EventCounterChangedPayload, config=CONFIG
    ),
    "A.event.counter-changed": TypeAdapter(author.EventCounterChanged),
    "A.history.counter-changed-where": TypeAdapter(
        author.CounterChangedHistoryWhere, config=CONFIG
    ),
    "A.history.counter-changed": TypeAdapter(
        author.HistoryCounterChanged, config=CONFIG
    ),
    "A.history.counter-changed-condition": TypeAdapter(
        author.HistoryCounterChangedCondition, config=CONFIG
    ),
    "A.event.acted-payload": TypeAdapter(author.EventActedPayload, config=CONFIG),
    "A.event.acted": TypeAdapter(author.EventActed),
    "A.history.acted-where": TypeAdapter(author.ActedHistoryWhere, config=CONFIG),
    "A.history.acted": TypeAdapter(author.HistoryActed, config=CONFIG),
    "A.history.acted-condition": TypeAdapter(
        author.HistoryActedCondition, config=CONFIG
    ),
    "A.event.stood-payload": TypeAdapter(author.EventStoodPayload, config=CONFIG),
    "A.event.stood": TypeAdapter(author.EventStood),
    "A.history.stood-where": TypeAdapter(author.StoodHistoryWhere, config=CONFIG),
    "A.history.stood": TypeAdapter(author.HistoryStood, config=CONFIG),
    "A.history.stood-condition": TypeAdapter(
        author.HistoryStoodCondition, config=CONFIG
    ),
    "A.event.raced-payload": TypeAdapter(author.EventRacedPayload, config=CONFIG),
    "A.event.raced": TypeAdapter(author.EventRaced),
    "A.history.raced-where": TypeAdapter(author.RacedHistoryWhere, config=CONFIG),
    "A.history.raced": TypeAdapter(author.HistoryRaced, config=CONFIG),
    "A.history.raced-condition": TypeAdapter(
        author.HistoryRacedCondition, config=CONFIG
    ),
    "A.event.drive-gained-payload": TypeAdapter(
        author.EventDriveGainedPayload, config=CONFIG
    ),
    "A.event.drive-gained": TypeAdapter(author.EventDriveGained),
    "A.history.drive-gained-where": TypeAdapter(
        author.DriveGainedHistoryWhere, config=CONFIG
    ),
    "A.history.drive-gained": TypeAdapter(author.HistoryDriveGained, config=CONFIG),
    "A.history.drive-gained-condition": TypeAdapter(
        author.HistoryDriveGainedCondition, config=CONFIG
    ),
    "A.event.ub-activated-payload": TypeAdapter(
        author.EventUbActivatedPayload, config=CONFIG
    ),
    "A.event.ub-activated": TypeAdapter(author.EventUbActivated),
    "A.history.ub-activated-where": TypeAdapter(
        author.UbActivatedHistoryWhere, config=CONFIG
    ),
    "A.history.ub-activated": TypeAdapter(author.HistoryUbActivated, config=CONFIG),
    "A.history.ub-activated-condition": TypeAdapter(
        author.HistoryUbActivatedCondition, config=CONFIG
    ),
    "A.event.fused-payload": TypeAdapter(author.EventFusedPayload, config=CONFIG),
    "A.event.fused": TypeAdapter(author.EventFused),
    "A.history.fused-where": TypeAdapter(author.FusedHistoryWhere, config=CONFIG),
    "A.history.fused": TypeAdapter(author.HistoryFused, config=CONFIG),
    "A.history.fused-condition": TypeAdapter(
        author.HistoryFusedCondition, config=CONFIG
    ),
    "A.event.targeted-payload": TypeAdapter(author.EventTargetedPayload, config=CONFIG),
    "A.event.targeted": TypeAdapter(author.EventTargeted),
    "A.history.targeted-where": TypeAdapter(author.TargetedHistoryWhere, config=CONFIG),
    "A.history.targeted": TypeAdapter(author.HistoryTargeted, config=CONFIG),
    "A.history.targeted-condition": TypeAdapter(
        author.HistoryTargetedCondition, config=CONFIG
    ),
    "A.event.dice-rolled-payload": TypeAdapter(
        author.EventDiceRolledPayload, config=CONFIG
    ),
    "A.event.dice-rolled": TypeAdapter(author.EventDiceRolled),
    "A.history.dice-rolled-where": TypeAdapter(
        author.DiceRolledHistoryWhere, config=CONFIG
    ),
    "A.history.dice-rolled": TypeAdapter(author.HistoryDiceRolled, config=CONFIG),
    "A.history.dice-rolled-condition": TypeAdapter(
        author.HistoryDiceRolledCondition, config=CONFIG
    ),
    "A.event.drive-checked-payload": TypeAdapter(
        author.EventDriveCheckedPayload, config=CONFIG
    ),
    "A.event.drive-checked": TypeAdapter(author.EventDriveChecked),
    "A.history.drive-checked-where": TypeAdapter(
        author.DriveCheckedHistoryWhere, config=CONFIG
    ),
    "A.history.drive-checked": TypeAdapter(author.HistoryDriveChecked, config=CONFIG),
    "A.history.drive-checked-condition": TypeAdapter(
        author.HistoryDriveCheckedCondition, config=CONFIG
    ),
    "A.event.drive-trigger-payload": TypeAdapter(
        author.EventDriveTriggerPayload, config=CONFIG
    ),
    "A.event.drive-trigger": TypeAdapter(author.EventDriveTrigger),
    "A.history.drive-trigger": TypeAdapter(author.HistoryDriveTrigger, config=CONFIG),
    "A.history.drive-trigger-condition": TypeAdapter(
        author.HistoryDriveTriggerCondition, config=CONFIG
    ),
    "A.event.moved-payload": TypeAdapter(author.EventMovedPayload, config=CONFIG),
    "A.event.moved": TypeAdapter(author.EventMoved),
    "A.history.moved-where": TypeAdapter(author.MovedHistoryWhere, config=CONFIG),
    "A.history.moved": TypeAdapter(author.HistoryMoved, config=CONFIG),
    "A.history.moved-condition": TypeAdapter(
        author.HistoryMovedCondition, config=CONFIG
    ),
    "A.event.drew-payload": TypeAdapter(author.EventDrewPayload, config=CONFIG),
    "A.event.drew": TypeAdapter(author.EventDrew),
    "A.history.drew-where": TypeAdapter(author.DrewHistoryWhere, config=CONFIG),
    "A.history.drew": TypeAdapter(author.HistoryDrew, config=CONFIG),
    "A.history.drew-condition": TypeAdapter(author.HistoryDrewCondition, config=CONFIG),
    "A.event.equipped-payload": TypeAdapter(author.EventEquippedPayload, config=CONFIG),
    "A.event.equipped": TypeAdapter(author.EventEquipped),
    "A.history.equipped-where": TypeAdapter(author.EquippedHistoryWhere, config=CONFIG),
    "A.history.equipped": TypeAdapter(author.HistoryEquipped, config=CONFIG),
    "A.history.equipped-condition": TypeAdapter(
        author.HistoryEquippedCondition, config=CONFIG
    ),
    "A.event.fused-as-material-payload": TypeAdapter(
        author.EventFusedAsMaterialPayload, config=CONFIG
    ),
    "A.event.fused-as-material": TypeAdapter(author.EventFusedAsMaterial),
    "A.history.fused-as-material-where": TypeAdapter(
        author.FusedAsMaterialHistoryWhere, config=CONFIG
    ),
    "A.history.fused-as-material": TypeAdapter(
        author.HistoryFusedAsMaterial, config=CONFIG
    ),
    "A.history.fused-as-material-condition": TypeAdapter(
        author.HistoryFusedAsMaterialCondition, config=CONFIG
    ),
    "A.history.payment-where": TypeAdapter(author.PaymentHistoryWhere),
    "A.history.payment": TypeAdapter(author.HistoryPayment, config=CONFIG),
    "A.history.payment-condition": TypeAdapter(
        author.HistoryPaymentCondition, config=CONFIG
    ),
    "A.history.payment-sum": TypeAdapter(author.HistoryPaymentSum, config=CONFIG),
    "A.event.counter-reached-payload": TypeAdapter(author.CounterReachedPayload),
    "A.event.counter-reached": TypeAdapter(author.EventCounterReached),
    "A.event.state": TypeAdapter(author.EventState),
    "A.event-atom": TypeAdapter(author.EventAtom, config=CONFIG),
    "A.event.any-of": TypeAdapter(author.EventAnyOf),
    "A.event": TypeAdapter(author.EventPattern, config=CONFIG),
    "A.history-query": TypeAdapter(author.HistoryQuery, config=CONFIG),
    "A.history-condition-query": TypeAdapter(
        author.HistoryConditionQuery, config=CONFIG
    ),
    "A.history-sum-query": TypeAdapter(author.HistorySumQuery, config=CONFIG),
    "A.cond.history": TypeAdapter(author.CondHistory),
    "A.expr.history-count": TypeAdapter(author.ExprHistoryCount),
    "A.expr.history-sum": TypeAdapter(author.ExprHistorySum),
    "A.capture.power-of": TypeAdapter(author.CapturedPowerOf),
    "A.capture.hp-of": TypeAdapter(author.CapturedHpOf),
    "A.capture.cost-of": TypeAdapter(author.CapturedCostOf),
    "A.capture.orig-cost-of": TypeAdapter(author.CapturedOrigCostOf),
    "A.state-numeric-read": TypeAdapter(author.StateNumericRead, config=CONFIG),
    "A.capture.timed": TypeAdapter(author.TimedCapture),
    "A.capture.selection": TypeAdapter(author.SelectionCapture),
    "A.expr.capture": TypeAdapter(author.CaptureRead, config=CONFIG),
}
