#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer};

pub(super) const PLAYER: PlayerId = PlayerId { seat: 1 };

pub(super) fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"acted":true,"power":0_u8,"hp":30_u8,"max_hp":30_u8}}
    ]);
    initial
}

pub(super) fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-ability-combinations",
    )
    .unwrap()
}

pub(super) fn decision_answer(domain: &InputDomain, modes: &[ModeId]) -> Answer {
    match domain {
        InputDomain::Pending(options) => Answer::Pending(
            options
                .iter()
                .min_by_key(|option| option.line)
                .unwrap()
                .handle,
        ),
        InputDomain::Targets(d) => Answer::Targets(
            d.members
                .iter()
                .take(
                    d.cardinality
                        .maximum
                        .map_or(d.members.len(), |n| usize::try_from(n.0).unwrap()),
                )
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::CostMaterials(d) => Answer::CostMaterials(
            d.members
                .iter()
                .take(usize::try_from(d.cardinality.minimum.0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
            d.cards
                .iter()
                .take(usize::try_from(d.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                .map(|m| m.handle)
                .collect(),
        )),
        InputDomain::SearchPartition(d) => Answer::SearchPartition(
            d.cards
                .iter()
                .take(usize::try_from(d.cardinality.maximum.unwrap_or(Nat(1)).0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Distribution(d) => {
            let count = u64::try_from(d.members.len()).unwrap();
            Answer::Distribution(
                d.members
                    .iter()
                    .enumerate()
                    .map(|(i, m)| DamageShare {
                        handle: m.handle,
                        amount: Nat(d
                            .total
                            .0
                            .checked_div(count)
                            .unwrap()
                            .checked_add(if i == 0 {
                                d.total.0.checked_rem(count).unwrap()
                            } else {
                                0
                            })
                            .unwrap()),
                    })
                    .collect(),
            )
        }
        InputDomain::Modes { .. } => Answer::Modes(modes.to_vec()),
        InputDomain::OptionalCost => Answer::OptionalCost(true),
        InputDomain::Integer(d) => Answer::Integer(d.maximum.unwrap_or(d.minimum)),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::Order(cards) => Answer::Order(cards.iter().map(|m| m.handle).collect()),
        InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
        InputDomain::Quick { .. } => Answer::Quick(None),
        _ => panic!("unexpected decision"),
    }
}

pub(super) fn finish(game: &mut Game, mut step: Option<CardPlay>, modes: &[ModeId]) {
    for _ in 0_u8..100_u8 {
        if let Some(CardPlay::Awaiting(question)) = step.take() {
            let answer = decision_answer(&question.domain, modes);
            step = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
            continue;
        }
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
            continue;
        }
        if let Some(q) = game.internal_pending_choice().unwrap() {
            let InputDomain::Pending(options) = &q.domain else {
                panic!("pending ability");
            };
            step = Some(
                game.answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Pending(
                        options
                            .iter()
                            .min_by_key(|option| option.line)
                            .unwrap()
                            .handle,
                    ),
                )
                .unwrap(),
            );
            continue;
        }
        if let Some(q) = game.internal_quick_choice().unwrap() {
            step = Some(
                game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                    .unwrap(),
            );
            continue;
        }
        return;
    }
    panic!("scenario did not settle");
}

pub(super) fn play(game: &mut Game, modes: &[ModeId]) {
    let step = game.play_internal_card("source", PLAYER).unwrap();
    finish(game, Some(step), modes);
}

pub(super) fn evolve(game: &mut Game, pp: u8) {
    evolve_with_modes(game, pp, &[ModeId(1)]);
}

pub(super) fn evolve_with_modes(game: &mut Game, pp: u8, modes: &[ModeId]) {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}}),
    )
    .unwrap();
    finish(game, None, modes);
}

pub(super) fn evolve_position(base: &str, form: &str) -> Value {
    let mut initial = position(base, "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

pub(super) fn activate_card(game: &mut Game, card: &str, line: u32, modes: &[ModeId]) {
    let ability = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line && a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PLAYER)
        .unwrap();
    finish(game, Some(step), modes);
}

pub(super) fn view(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}
