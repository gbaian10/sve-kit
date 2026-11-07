#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed discard-source scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

const ACES: [&str; 6] = [
    "BP22-R23",
    "BP22-SL35",
    "ECP01-031",
    "ECP01-SL16",
    "ECP01-SP16",
    "ECP01-SSP16",
];
const BARCS: [&str; 2] = ["CP03-030", "PR-276"];

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"d0","card":"BP01-171"}, {"id":"d1","card":"BP01-171"},
        {"id":"d2","card":"BP01-171"}
    ]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-discard-ability-source",
    )
    .unwrap()
}

fn spell_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-spell-discard-source",
    )
    .unwrap()
}

fn pending(game: &mut Game, card: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed occurrence");
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay, material: &str, target: &str, accept: bool) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(vec![
                domain
                    .members
                    .iter()
                    .find(|entry| entry.card.as_deref() == Some(material))
                    .unwrap()
                    .handle,
            ]),
            InputDomain::Targets(domain) => Answer::Targets(vec![
                domain
                    .members
                    .iter()
                    .find(|entry| entry.card.as_deref() == Some(target))
                    .unwrap()
                    .handle,
            ]),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Order(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed decision: {:?}", q.domain)
            }
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_) | CardPlay::Entered(_)),
        "the printed play completes"
    );
    finish_checks(game);
}

#[test]
fn q1867_discard_at_six_max_checks_awakening_after_neouniverse_finishes() {
    for number in ACES {
        for accept in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"neo","card":"ECP01-028"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"ECP01-029"}]);
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"ace","card":number}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
            let mut board = game(&initial);
            assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"neo","evolve_card":"form","pay":{"pp":2_u8}})).unwrap().outcome, "resolved");
            let evolution = pending(&mut board, "ECP01-029", 2);
            finish(&mut board, evolution, number, "BP01-171", true);
            let before = board.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["pp"], json!({"current":4_u8,"max":7_u8}));
            assert_eq!(before["P1"]["cemetery"], json!(["ace"]));
            let return_ace = pending(&mut board, number, 1);
            finish(&mut board, return_ace, number, "BP01-171", accept);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accept { 1_u8 } else { 4_u8 }
            );
            assert_eq!(
                state["P1"]["cemetery"],
                if accept { json!([]) } else { json!(["ace"]) }
            );
            if accept {
                // CR 12.4.3 includes entry from the cemetery in Fanfare.
                let fanfare = pending(&mut board, number, 4);
                finish(&mut board, fanfare, number, "BP01-171", true);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["P2"]["cemetery"],
                    json!(["enemy"])
                );
                assert_eq!(
                    board
                        .internal_rule_action(
                            &json!({"do":"attack","attacker":"ace","target":"P2.leader"})
                        )
                        .unwrap()
                        .outcome,
                    "resolved",
                    "the returned real Ace retains Storm"
                );
            } else {
                assert!(board.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn printed_spell_optional_discard_cost_is_the_direct_uma_source() {
    for number in ACES {
        for maximum in [6_u8, 7] {
            let mut initial = position();
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"spell","card":"ECP01-036"},{"id":"ace","card":number}
            ]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-152"}]);
            let mut board = spell_game(&initial);
            let spell = board
                .play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, spell, number, "BP01-152", true);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["hand"],
                json!(["d0"]),
                "the actual seven-cost material enables the spell's printed draw"
            );
            assert_eq!(state["P1"]["pp"]["current"], maximum - 1);
            let return_ace = pending(&mut board, number, 1);
            finish(&mut board, return_ace, number, "BP01-152", true);
            if maximum == 7 {
                let fanfare = pending(&mut board, number, 4);
                finish(&mut board, fanfare, number, "BP01-152", true);
                assert_eq!(
                    board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    3_u8
                );
                assert_eq!(
                    board.projection(View::Referee).unwrap()["P2"]["cemetery"],
                    json!(["enemy"])
                );
            } else {
                assert_eq!(
                    board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                    5_u8
                );
                assert!(board.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn royal_paladin_fanfare_cost_offers_each_barc_printing_its_optional_ex_move() {
    for number in BARCS {
        for accept in [false, true] {
            let mut initial = position();
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"helper","card":"CP03-036"},{"id":"barc","card":number}
            ]);
            let mut board = game(&initial);
            let entry = board
                .play_internal_card("helper", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut board, entry, number, "BP01-171", true);
            let fanfare = pending(&mut board, "CP03-036", 1);
            finish(&mut board, fanfare, number, "BP01-171", true);
            assert_eq!(
                board.projection(View::Referee).unwrap()["P1"]["hand"],
                json!(["d0"])
            );
            let transfer = pending(&mut board, number, 1);
            finish(&mut board, transfer, number, "BP01-171", accept);
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["ex"],
                if accept { json!(["barc"]) } else { json!([]) }
            );
            assert_eq!(
                state["P1"]["cemetery"],
                if accept { json!([]) } else { json!(["barc"]) }
            );
            assert!(board.internal_pending_choice().unwrap().is_none());
        }
    }
}

#[test]
fn barc_keeps_ward_and_its_printed_lastword_after_ordinary_play() {
    for number in BARCS {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"barc","card":number},{"id":"themis","card":"BP01-131"}
        ]);
        let mut board = game(&initial);
        let entry = board
            .play_internal_card("barc", PlayerId { seat: 1 })
            .unwrap();
        let CardPlay::Awaiting(q) = entry else {
            panic!("the printed Ward asks its entry posture");
        };
        assert!(matches!(q.domain, InputDomain::WardEntry { .. }));
        let posture = board
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(true))
            .unwrap();
        finish(&mut board, posture, number, "BP01-171", true);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["barc"]["acted"],
            true
        );
        let spell = board
            .play_internal_card("themis", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, spell, number, "BP01-171", true);
        let lastword = pending(&mut board, number, 3);
        finish(&mut board, lastword, number, "BP01-171", true);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["d0"])
        );
        assert!(board.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn ace_keeps_fanfare_meal_and_storm_on_every_ordinary_printing() {
    for number in ACES {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot-0","card":"CSD01-032"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
        let mut board = game(&initial);
        let entry = board
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut board, entry, number, "BP01-171", true);
        let fanfare = pending(&mut board, number, 4);
        finish(&mut board, fanfare, number, "BP01-171", true);
        meal_race::feed(&mut board, number, 0, false);
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 2_u8);
        assert_eq!(view["P1"]["race"], json!(["carrot-0"]));
        assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
    }
}
