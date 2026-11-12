#![expect(
    clippy::panic,
    reason = "Unexpected real-card evolution stages fail the scenario."
)]

use super::*;
use sve_engine::game::player::{
    Command, EvolutionAnswer, EvolutionQuestion, Input, MainActionAnswer, MainIntentKind, Question,
    QuestionShape, SelectionQuestion, SubmissionKind, Viewer,
};

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player evolution question");
    };
    *question
}

fn assert_selection(game: &Game, selection: &SelectionQuestion) {
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(
        selection.groups.len(),
        1,
        "An evolution payment has one effective selection group"
    );
    assert_eq!(
        selection.groups[0].cardinality, selection.cardinality,
        "The effective group bounds match the complete selection"
    );
    assert!(
        selection.members.iter().all(|member| {
            member.object.as_ref().is_some_and(|link| {
                observation
                    .players
                    .iter()
                    .flat_map(|player| &player.zones)
                    .flat_map(|zone| &zone.cards)
                    .any(|card| &card.card.handle == link)
            })
        }),
        "Evolution cost candidates must link to the choosers Observation"
    );
    assert!(
        matches!(game.input(Viewer::P2), Input::Waiting(_)),
        "Only the actor receives evolution cost candidates"
    );
}

pub(super) fn evolve(game: &mut Game, super_evolution: bool) -> SubmissionKind {
    evolve_with_ep(game, super_evolution, false)
}

fn evolve_with_ep(game: &mut Game, super_evolution: bool, use_ep: bool) -> SubmissionKind {
    game.advance_player_input();
    let main = question(game);
    let QuestionShape::ActionWindow(actions) = main.shape else {
        panic!("main actions");
    };
    let intent = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Evolve { .. }))
        .unwrap();
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: main.token,
                intent: intent.id.clone(),
                answer: MainActionAnswer::None
            }
        )
        .kind,
        SubmissionKind::Applied,
        "The main intent begins the native evolution declaration"
    );
    for _ in 0_u8..5 {
        let current = question(game);
        let QuestionShape::Evolution(stage) = current.shape else {
            panic!("evolution declaration stage");
        };
        let (answer, finish) = match stage {
            EvolutionQuestion::Form(forms) => (EvolutionAnswer::Form(forms[0].id), false),
            EvolutionQuestion::Materials(domain) => {
                assert_selection(game, &domain);
                (
                    EvolutionAnswer::Materials(
                        domain
                            .members
                            .iter()
                            .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                            .map(|member| member.handle)
                            .collect(),
                    ),
                    false,
                )
            }
            EvolutionQuestion::Discard(domain) => {
                assert_selection(game, &domain);
                (
                    EvolutionAnswer::Discard(
                        domain
                            .members
                            .iter()
                            .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                            .map(|member| member.handle)
                            .collect(),
                    ),
                    false,
                )
            }
            EvolutionQuestion::Payment(payment) => (
                EvolutionAnswer::Payment {
                    pp: payment.total_pp.checked_sub(i64::from(use_ep)).unwrap(),
                    ep: use_ep,
                    super_evolution,
                },
                true,
            ),
        };
        let result = game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: current.token,
                answer,
            },
        );
        if finish || result.kind != SubmissionKind::Applied {
            return result.kind;
        }
    }
    panic!("evolution declaration must finish");
}

#[test]
fn player_evolution_real_life_cost_is_native_and_unpayable_cost_reverts() {
    for life in [2_u8, 20] {
        let mut initial = evolution_payment::position("BP01-105", "BP01-106");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = evolution_payment::game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, false),
            if life == 2 {
                SubmissionKind::Reverted
            } else {
                SubmissionKind::Applied
            },
            "Life is paid by the original intrinsic rule procedure"
        );
        let state = game.projection(View::Referee).unwrap();
        if life == 2 {
            assert_eq!(
                state, before,
                "Failed life payment reveals and spends nothing"
            );
        } else {
            assert_eq!(
                state["P1"]["leader"]["life"], 17_i64,
                "The printed three life is paid once"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 10_i64,
                "This evolution has no PP cost"
            );
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "The selected real form is used"
            );
        }
    }
}

#[test]
fn player_evolution_real_cemetery_materials_are_selected_before_native_payment() {
    for available in [1_u8, 2] {
        let mut initial = evolution_payment::position("BP04-094", "BP04-095");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..available)
                .map(|index| json!({"id":format!("fee{index}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = evolution_payment::game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, false),
            if available == 1 {
                SubmissionKind::Reverted
            } else {
                SubmissionKind::Applied
            },
            "The native material count remains two"
        );
        let state = game.projection(View::Referee).unwrap();
        if available == 1 {
            assert_eq!(
                state, before,
                "An impossible material declaration spends nothing"
            );
        } else {
            assert_eq!(
                state["P1"]["banish"],
                json!(["fee0", "fee1"]),
                "Both selected cemetery cards pay the real cost"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!([]),
                "The material movement is committed once"
            );
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "The form is committed after payment"
            );
        }
    }
}

#[test]
fn player_evolution_real_hand_discard_uses_own_hand_and_preserves_other_zones() {
    for zone in ["hand", "cemetery"] {
        let mut initial = evolution_payment::position("BP05-098", "BP05-099");
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"fee","card":"BP01-171"}]);
        let mut game = evolution_payment::game(&initial);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            evolve(&mut game, false),
            if zone == "hand" {
                SubmissionKind::Applied
            } else {
                SubmissionKind::Reverted
            },
            "Only an own-hand card can pay the printed discard"
        );
        let state = game.projection(View::Referee).unwrap();
        if zone == "cemetery" {
            assert_eq!(
                state, before,
                "Another zone cannot substitute for a discard"
            );
        } else {
            assert_eq!(
                state["P1"]["hand"],
                json!([]),
                "The selected hand card is discarded once"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["fee"]),
                "The original discard movement is retained"
            );
            assert_eq!(
                state["objects"]["source"]["evolved_with"], "form",
                "The real source evolves after discarding"
            );
        }
    }
}

#[test]
fn player_evolution_real_pp_cost_can_use_ep_and_sep_without_expanding_face_payments() {
    for super_evolution in [false, true] {
        let mut initial = evolution_payment::position("BP09-004", "BP09-005");
        initial["players"]["P1"]["pp"] = json!({"current":0_i64,"max":10_i64});
        initial["players"]["P1"]["ep"] = json!(1_i64);
        initial["players"]["P1"]["sep"] = json!(1_i64);
        let mut game = evolution_payment::game(&initial);
        assert_eq!(
            evolve_with_ep(&mut game, super_evolution, true),
            SubmissionKind::Applied,
            "One EP replaces the printed PP, with optional SEP at turn seven"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"], 0_i64,
            "No unavailable PP is spent"
        );
        assert_eq!(state["P1"]["ep"], 0_i64, "Exactly one EP replaces one PP");
        assert_eq!(
            state["P1"]["sep"],
            i64::from(!super_evolution),
            "SEP is spent only for super evolution"
        );
        assert_eq!(
            state["objects"]["source"]["name"], "深緑の純心・ポーラ",
            "The displayed face capability selects the actual real destination"
        );
        assert_eq!(
            state["objects"]["source"]["super_evolved"], super_evolution,
            "Normal and super facets follow the original primitive"
        );
    }
}
