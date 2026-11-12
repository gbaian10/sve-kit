#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

pub(super) fn position(number: &str) -> Value {
    let mut value = draw_position();
    value["turn"]["elapsed_turns"] = json!({"P1":7_i32,"P2":6_i32});
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    value["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw1","card":"BP01-037"},
        {"id":"draw2","card":"BP01-037"},
        {"id":"draw3","card":"BP01-037"}
    ]);
    value["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    value
}

pub(super) fn finish(game: &mut Game, mut play: CardPlay) {
    for _ in 0_i32..32_i32 {
        let CardPlay::Awaiting(q) = play else {
            return;
        };
        let answer = match &q.domain {
            InputDomain::ResourceEntryPayment { materials, .. } => Answer::ResourceEntryPayment {
                materials: materials
                    .members
                    .iter()
                    .take(usize::try_from(materials.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
                use_ep: false,
            },
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(d) => {
                Answer::Targets(d.members.iter().take(1).map(|m| m.handle).collect())
            }
            InputDomain::Search(d) => Answer::Search(d.groups.as_ref().map_or_else(
                || SearchAnswer::Ungrouped(d.cards.iter().take(1).map(|m| m.handle).collect()),
                |groups| {
                    SearchAnswer::Grouped(
                        groups
                            .iter()
                            .map(|g| g.handles.iter().take(1).copied().collect())
                            .collect(),
                    )
                },
            )),
            InputDomain::Integer(d) => Answer::Integer(d.maximum.unwrap_or(d.minimum)),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
            InputDomain::Quick { .. } => Answer::Quick(None),
            InputDomain::OptionalCost => Answer::OptionalCost(false),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            _ => panic!("unexpected decision: {:?}", q.domain),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    panic!("unbounded decision loop");
}

pub(super) fn settle(game: &mut Game) {
    for _ in 0_i32..8_i32 {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(q) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending");
        };
        let handle = options[0].handle;
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap();
        finish(game, play);
    }
    panic!("unbounded pending loop");
}

pub(super) fn enter(game: &mut Game) {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, play);
    settle(game);
}
