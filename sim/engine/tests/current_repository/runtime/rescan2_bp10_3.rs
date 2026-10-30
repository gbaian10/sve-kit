#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
}

fn mode_question(game: &mut Game) -> DecisionView {
    play(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("fanfare pending")
    };
    let CardPlay::Awaiting(modes) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("mode selection")
    };
    assert!(
        matches!(&modes.domain, InputDomain::Modes { .. }),
        "the fanfare offers its printed modes"
    );
    modes
}
fn select_mode(game: &mut Game, modes: &DecisionView, mode: ModeId) {
    let step = game
        .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![mode]))
        .unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
}
#[test]
fn bp10_051_fanfare_can_draw_or_recover_only_a_grave_spell() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = position("BP10-051");
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"spell","card":"BP01-023"}]);
        let mut game = game(&initial);
        let question = mode_question(&mut game);
        select_mode(&mut game, &question, mode);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if mode == ModeId(1) {
                json!(["first"])
            } else {
                json!(["spell"])
            }
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if mode == ModeId(1) {
                json!(["wrong", "spell"])
            } else {
                json!(["wrong"])
            }
        );
    }
}
#[test]
fn bp10_106_fanfare_creates_falcon_or_recovers_a_grave_amulet() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = position("BP10-106");
        initial["players"]["P1"]["zones"]["cemetery"] =
            json!([{"id":"wrong","card":"BP01-171"},{"id":"amulet","card":"BP01-016"}]);
        let mut game = game(&initial);
        let question = mode_question(&mut game);
        select_mode(&mut game, &question, mode);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if mode == ModeId(1) {
                json!([])
            } else {
                json!(["amulet"])
            }
        );
        assert_eq!(
            state["P1"]["field"].as_array().unwrap().len(),
            if mode == ModeId(1) { 2 } else { 1 }
        );
        if mode == ModeId(1) {
            let falcon = state["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "ホーリーファルコン")
                .unwrap()
                .as_str()
                .unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":falcon,"target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the real Falcon has entry-turn Storm"
            );
        }
    }
}
#[test]
fn official_qa_forbids_recovery_modes_without_their_required_grave_targets() {
    for card in ["BP10-051", "BP10-106"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"BP01-171"}]);
        let mut game = game(&initial);
        let question = mode_question(&mut game);
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("printed modes")
        };
        assert_eq!(
            modes.as_slice(),
            &[ModeId(1)],
            "Q1299/Q1349 exclude the recovery mode without a legal grave target"
        );
        assert!(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)])
            )
            .is_err(),
            "Q1299/Q1349 require choosing the other mode when no legal recovery target exists"
        );
        select_mode(&mut game, &question, ModeId(1));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["wrong"]));
        if card == "BP10-051" {
            assert_eq!(state["P1"]["hand"], json!(["first"]));
        } else {
            assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
        }
    }
}
