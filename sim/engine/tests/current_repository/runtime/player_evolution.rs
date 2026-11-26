
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

fn offered_evolution_lines(game: &mut Game) -> Vec<u64> {
    let before = game.projection(View::Referee).unwrap();
    game.advance_player_input();
    let QuestionShape::ActionWindow(actions) = question(game).shape else {
        panic!("main actions");
    };
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "inspecting the menu does not reveal the form or pay a cost"
    );
    actions
        .intents
        .into_iter()
        .filter_map(|intent| {
            if let MainIntentKind::Evolve { line, .. } = intent.kind {
                Some(line)
            } else {
                None
            }
        })
        .collect()
}

fn assert_selection(game: &Game, selection: &SelectionQuestion, expected_groups: usize) {
    let observation = game.observe(Viewer::P1).unwrap();
    assert_eq!(
        selection.groups.len(),
        expected_groups,
        "The evolution payment has the printed number of selection groups"
    );
    if let [group] = selection.groups.as_slice() {
        assert_eq!(
            group.cardinality, selection.cardinality,
            "The effective group bounds match the complete selection"
        );
    } else {
        assert!(
            selection
                .groups
                .iter()
                .all(|group| group.cardinality.maximum == Some(group.cardinality.minimum)),
            "Each grouped evolution material has a fixed quota"
        );
        assert_eq!(
            selection
                .groups
                .iter()
                .map(|group| group.cardinality.minimum.0)
                .sum::<u64>(),
            selection.cardinality.minimum.0,
            "The effective group minima match the complete selection"
        );
        assert_eq!(
            selection.cardinality.maximum,
            Some(selection.cardinality.minimum),
            "The complete grouped selection also has a fixed quota"
        );
    }
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
    evolve_with_ep(game, super_evolution, false, 1)
}

pub(super) fn evolve_with_material_groups(
    game: &mut Game,
    expected_groups: usize,
) -> SubmissionKind {
    evolve_with_ep(game, false, false, expected_groups)
}

fn evolve_with_ep(
    game: &mut Game,
    super_evolution: bool,
    use_ep: bool,
    expected_groups: usize,
) -> SubmissionKind {
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
                assert_selection(game, &domain, expected_groups);
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
                assert_selection(game, &domain, 1);
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
fn player_evolution_real_life_cost_is_native_and_unpayable_cost_is_hidden() {
    for life in [2_u8, 20] {
        let mut initial = evolution_payment::position("BP01-105", "BP01-106");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = evolution_payment::game(&initial);
        if life == 2 {
            assert_eq!(
                offered_evolution_lines(&mut game),
                Vec::<u64>::new(),
                "two life cannot pay three"
            );
            let before = game.projection(View::Referee).unwrap();
            let outcome = game.internal_rule_action(&json!({
                "do":"evolve", "source":"source", "evolve_card":"form", "pay":{"pp":0_u8}
            }));
            assert_eq!(outcome.unwrap().outcome, "cannot-evolve");
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        assert_eq!(evolve(&mut game, false), SubmissionKind::Applied);
        let state = game.projection(View::Referee).unwrap();
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
        if available == 1 {
            assert_eq!(
                offered_evolution_lines(&mut game),
                Vec::<u64>::new(),
                "one cemetery card cannot pay for two"
            );
            continue;
        }
        assert_eq!(evolve(&mut game, false), SubmissionKind::Applied);
        let state = game.projection(View::Referee).unwrap();
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

#[test]
fn player_evolution_real_hand_discard_uses_own_hand_and_preserves_other_zones() {
    for zone in ["hand", "cemetery"] {
        let mut initial = evolution_payment::position("BP05-098", "BP05-099");
        initial["players"]["P1"]["zones"][zone] = json!([{"id":"fee","card":"BP01-171"}]);
        let mut game = evolution_payment::game(&initial);
        if zone == "cemetery" {
            assert_eq!(
                offered_evolution_lines(&mut game),
                Vec::<u64>::new(),
                "a cemetery card cannot pay a discard"
            );
            continue;
        }
        assert_eq!(evolve(&mut game, false), SubmissionKind::Applied);
        let state = game.projection(View::Referee).unwrap();
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

#[test]
fn player_evolution_cosmos_fang_requires_another_own_follower() {
    let mut initial = evolution_payment::position("BP03-002", "BP03-003");
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["ep"] = json!(1_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        Vec::<u64>::new(),
        "an opposing follower cannot pay the return cost"
    );

    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    let mut game = evolution_payment::game(&initial);
    assert_eq!(offered_evolution_lines(&mut game), [1]);
    assert_eq!(
        evolve_with_ep(&mut game, false, true, 1),
        SubmissionKind::Applied
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["ally"]));
    assert_eq!(after["P2"]["field"], json!(["enemy"]));
}

#[test]
fn player_evolution_tsukasa_offers_only_payable_printed_lines() {
    let mut initial = evolution_payment::position("CP02-057", "CP02-058");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["ex"] = json!([
        {"id":"item1","card":"CP02-T01"},{"id":"item2","card":"CP02-T01"}
    ]);
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        Vec::<u64>::new(),
        "one PP and two items cannot pay either evolution cost"
    );

    initial["players"]["P1"]["zones"]["ex"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"item3","card":"CP02-T01"}));
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        [2]
    );

    initial["players"]["P1"]["pp"]["current"] = json!(2_u8);
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        [1, 2]
    );
}

#[test]
fn player_evolution_blazing_core_needs_both_named_cemetery_materials() {
    let mut initial = evolution_payment::position("CP03-066", "CP03-067");
    initial["players"]["P1"]["pp"]["current"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"tail1","card":"CP03-080"},{"id":"tail2","card":"CP03-080"}
    ]);
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        Vec::<u64>::new(),
        "two identical materials cannot fill different name groups"
    );

    initial["players"]["P1"]["zones"]["cemetery"][1] = json!({"id":"claw","card":"CP03-077"});
    assert_eq!(
        offered_evolution_lines(&mut evolution_payment::game(&initial)),
        [2]
    );
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
            evolve_with_ep(&mut game, super_evolution, true, 1),
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

#[test]
fn player_evolution_withdrawn_at_the_super_payment_returns_to_the_action_window() {
    let mut initial = evolution_payment::position("BP09-004", "BP09-005");
    initial["players"]["P1"]["sep"] = json!(1_i64);
    let mut game = evolution_payment::game(&initial);
    game.advance_player_input();
    let window = question(&game);
    let before = game.projection(View::Referee).unwrap();
    let QuestionShape::ActionWindow(actions) = &window.shape else {
        panic!("main actions");
    };
    let intent = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Evolve { .. }))
        .unwrap();
    let declared = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: window.token,
            intent: intent.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
    assert_eq!(declared.kind, SubmissionKind::Applied);
    let payment = loop {
        let current = question(&game);
        assert!(
            current.withdrawable,
            "every evolution stage can be withdrawn"
        );
        let QuestionShape::Evolution(stage) = &current.shape else {
            panic!("evolution declaration stage");
        };
        let EvolutionQuestion::Form(forms) = stage else {
            break current;
        };
        let answered = game.submit_player(
            Viewer::P1,
            Command::Evolution {
                token: current.token,
                answer: EvolutionAnswer::Form(forms[0].id),
            },
        );
        assert_eq!(answered.kind, SubmissionKind::Applied);
    };
    assert!(matches!(
        payment.shape,
        QuestionShape::Evolution(EvolutionQuestion::Payment(quote)) if quote.super_allowed
    ));
    let withdrawn = game.submit_player(
        Viewer::P1,
        Command::Withdraw {
            token: payment.token,
        },
    );
    assert_eq!(withdrawn.kind, SubmissionKind::Reverted);
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let again = question(&game);
    assert!(matches!(again.shape, QuestionShape::ActionWindow(_)));
    assert_ne!(again.token, window.token);
}
