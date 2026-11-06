#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-sol-e-top1",
    )
    .unwrap()
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"opp-draw","card":"BP01-171"}]);
    initial
}

fn finish(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = match &q.domain {
            InputDomain::Targets(d) => Answer::Targets(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::CostMaterials(d) => Answer::CostMaterials(
                d.members
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Search(d) => Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(
                        usize::try_from(d.cardinality.maximum.unwrap_or(d.cardinality.minimum).0)
                            .unwrap(),
                    )
                    .map(|m| m.handle)
                    .collect(),
            )),
            InputDomain::Order(d) => Answer::Order(d.iter().map(|m| m.handle).collect()),
            InputDomain::Modes { .. } => Answer::Modes(vec![mode]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::TokenCapacity { .. } => Answer::TokenCapacity(0),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement => panic!("Unexpected card decision"),
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step, ModeId(1));
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Expected pending ability")
    };
    let chosen = options.iter().find(|o| o.card == card).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap()
}

fn pass_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    for _ in 0_u8..4_u8 {
        if let Some(q) = game.internal_quick_choice().unwrap() {
            game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
        } else {
            return;
        }
    }
    panic!("Quick window must finish")
}

#[test]
fn cp04_060_ub_evolution_searches_a_different_dragons_nest_with_a_discount() {
    let mut initial = position("CP04-060");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-059"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-060"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"same-name","card":"CP04-059"},{"id":"found","card":"CP04-063"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let step = trigger(&mut game, "CP04-060");
    let CardPlay::Awaiting(q) = &step else {
        panic!("Expected a deck search")
    };
    let InputDomain::Search(domain) = &q.domain else {
        panic!("Expected search candidates")
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "CP04-063");
    finish(&mut game, step, ModeId(1));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["found"])
    );
    play(&mut game, "found");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8
    );
}

#[test]
fn bp16_099_acts_an_amulet_on_entry_to_prevent_the_enemys_next_turn_attack() {
    let mut initial = position("BP16-099");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-041"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let step = trigger(&mut game, "BP16-099");
    finish(&mut game, step, ModeId(1));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["amulet"]["acted"],
        true
    );
    pass_turn(&mut game);
    let opposing_turn = game.projection(View::Referee).unwrap();
    assert_eq!(opposing_turn["turn"]["active"], "P2");
    assert_eq!(opposing_turn["objects"]["enemy"]["acted"], false);
    for target in ["P1.leader", "source"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":target}))
                .unwrap()
                .outcome,
            "cannot-attack"
        );
    }
}
