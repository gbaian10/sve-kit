#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-sum",
    )
    .unwrap()
}

fn ability(card: &str, index: usize) -> AbilityId {
    selection_with_rules().current[card]
        .program
        .card()
        .abilities[index]
        .node
        .id
}

fn train(card: &str, crew: &Value) -> Game {
    let mut initial = draw_position();
    let mut field = vec![json!({"id":"train","card":card})];
    field.extend(crew.as_array().unwrap().iter().cloned());
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    rules_game(&initial)
}

fn handles(question: &DecisionView, game: &Game, ids: &[&str]) -> Vec<u64> {
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected crew materials, got {:?}", question.domain);
    };
    let state = game.projection(View::Referee).unwrap();
    ids.iter()
        .map(|id| {
            let card = state["objects"][id]["card"].as_str().unwrap();
            domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(card))
                .unwrap_or_else(|| panic!("{id} must be offered"))
                .handle
        })
        .collect()
}

#[test]
fn bp11_p30_pilots_only_with_crew_whose_original_costs_total_five_or_more() {
    for printing in ["BP11-P30", "BP11-T02", "EBD03-T05"] {
        let mut game = train(
            printing,
            &json!([
                {"id":"one","card":"BP01-171"},
                {"id":"two","card":"BP17-077"},
                {"id":"three","card":"BP07-076"}
            ]),
        );
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("train", ability(printing, 0), PLAYER)
            .unwrap()
        else {
            panic!("{printing} asks for its crew");
        };
        let short = handles(&question, &game, &["one", "two"]);
        let before = format!("{game:?}");
        game.answer_internal_card(question.actor, question.token, Answer::CostMaterials(short))
            .unwrap_err();
        assert_eq!(format!("{game:?}"), before, "1 + 2 is below the printed 5");
        let crew = handles(&question, &game, &["two", "three"]);
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::CostMaterials(crew))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        for id in ["two", "three"] {
            assert_eq!(state["objects"][id]["acted"], true, "{id} crews the train");
        }
        assert_eq!(state["objects"]["one"]["acted"], false);
        assert_eq!(state["objects"]["train"]["power"], 4_u8);
        assert_eq!(state["objects"]["train"]["hp"], 5_u8);
        assert!(
            state["objects"]["train"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "the piloted train has its printed Rush"
        );
    }
}

#[test]
fn bp11_p30_cannot_be_piloted_when_every_follower_together_costs_less_than_five() {
    let mut game = train(
        "BP11-P30",
        &json!([{"id":"one","card":"BP01-171"},{"id":"three","card":"BP07-076"}]),
    );
    let before = format!("{game:?}");
    game.activate_internal_card("train", ability("BP11-P30", 0), PLAYER)
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "an impossible crew leaves the game unchanged"
    );
}

#[test]
fn bp11_069_creates_a_magic_train_only_when_none_is_on_the_field() {
    for printing in ["BP11-069", "BP11-SL13", "EBD03-005"] {
        for existing in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
            initial["players"]["P1"]["zones"]["field"] = if existing {
                json!([{"id":"old","card":"BP11-P30"}])
            } else {
                json!([])
            };
            let mut game = rules_game(&initial);
            assert!(
                matches!(
                    game.play_internal_card("source", PLAYER).unwrap(),
                    CardPlay::Entered(_)
                ),
                "{printing} enters before its fanfare"
            );
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("expected the fanfare of {printing}");
            };
            resolved(
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            let trains = state["P1"]["field"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == json!("魔導列車"))
                .count();
            assert_eq!(trains, 1, "{printing} with an existing train: {existing}");
        }
    }
}

#[test]
fn player_question_real_train_cost_floor_uses_frozen_printed_costs() {
    use sve_engine::game::player::{
        Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
    };
    let mut game = train(
        "BP11-P30",
        &json!([
            {"id":"one","card":"BP01-171","state":{"cost":9_i64}},
            {"id":"two","card":"BP17-077","state":{"cost":0_i64}},
            {"id":"three","card":"BP07-076","state":{"cost":0_i64}}
        ]),
    );
    game.activate_internal_card("train", ability("BP11-P30", 0), PLAYER)
        .unwrap();
    let Input::Question(public) = game.input(Viewer::P1) else {
        panic!("player crew question");
    };
    let QuestionShape::Decision(DecisionShape::CostMaterials(materials)) = &public.shape else {
        panic!("player crew materials");
    };
    assert_eq!(materials.minimum_original_cost, Some(5));
    assert_eq!(materials.maximum_original_cost, None);
    assert!(!materials.distinct_original_costs);
    assert!(
        materials
            .members
            .iter()
            .all(|member| member.object.is_some())
    );
    assert_eq!(
        materials
            .members
            .iter()
            .map(|member| member.original_cost.unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let selected = materials
        .members
        .iter()
        .filter(|member| member.original_cost.unwrap() >= 2)
        .map(|member| member.handle)
        .collect();
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: public.token,
                answer: Answer::CostMaterials(selected)
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["one"]["acted"], false);
    assert_eq!(state["objects"]["two"]["acted"], true);
    assert_eq!(state["objects"]["three"]["acted"], true);
    assert_eq!(state["objects"]["train"]["power"], 4_u8);
}

#[test]
fn policy_real_train_selects_complete_crew_from_public_question() {
    use sve_engine::game::player::{
        Command, DecisionShape, Input, QuestionShape, SubmissionKind, Viewer,
    };
    use sve_engine::ir::Posture;
    use sve_engine::policy;

    let mut game = train(
        "BP11-P30",
        &json!([
            {"id":"one","card":"BP01-171","state":{"cost":9_i64}},
            {"id":"two","card":"BP17-077","state":{"cost":0_i64}},
            {"id":"three","card":"BP07-076","state":{"cost":0_i64}}
        ]),
    );
    game.activate_internal_card("train", ability("BP11-P30", 0), PLAYER)
        .unwrap();
    let Input::Question(public) = game.input(Viewer::P1) else {
        panic!("player crew question");
    };
    let QuestionShape::Decision(DecisionShape::CostMaterials(materials)) = &public.shape else {
        panic!("player crew materials");
    };
    let selected = policy::select(materials).unwrap();
    assert_eq!(selected.roles.len(), 0);
    assert_eq!(selected.targets.len(), 2);
    assert_eq!(
        materials
            .members
            .iter()
            .filter(|member| selected.targets.contains(&member.handle))
            .map(|member| member.original_cost)
            .collect::<Vec<_>>(),
        vec![Some(2), Some(3)]
    );
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: public.token,
                answer: Answer::CostMaterials(selected.targets),
            },
        )
        .kind,
        SubmissionKind::Applied,
    );
    let observation = game.observe(Viewer::P1).unwrap();
    let acted = observation.players[0]
        .zones
        .iter()
        .flat_map(|zone| &zone.cards)
        .filter(|card| {
            card.card
                .properties
                .state
                .as_ref()
                .is_some_and(|state| state.posture == Posture::Acted)
        })
        .count();
    assert_eq!(acted, 2);
}
