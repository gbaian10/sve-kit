#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed hand scenarios."
)]
use super::*;

fn aliases(number: &str) -> Vec<String> {
    let selected = selection_with_rules();
    let id = &selected.current[number].card_id;
    selected
        .current
        .iter()
        .filter(|(_, card)| &card.card_id == id)
        .map(|(alias, _)| alias.clone())
        .collect()
}
fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "hand-selection-batch",
    )
    .unwrap()
}
fn initial(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"},{"id":"tail1","card":"BP01-037"},{"id":"tail2","card":"BP01-037"},{"id":"tail3","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn vanguard_initial(number: &str) -> Value {
    let mut initial = initial(number, "hand");
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("cardfight_vanguard");
    initial
}
fn finish(game: &mut Game, mut play: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) | InputDomain::CostMaterials(domain) => {
                let count = domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0;
                let handles = if count == 1
                    && let Some(leader) = domain.members.iter().find(|m| m.card.is_none())
                {
                    vec![leader.handle]
                } else {
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(count).unwrap())
                        .map(|m| m.handle)
                        .collect::<Vec<_>>()
                };
                if matches!(question.domain, InputDomain::Targets(_)) {
                    Answer::Targets(handles)
                } else {
                    Answer::CostMaterials(handles)
                }
            }
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected hand migration question: {:?}", question.domain)
            }
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has a legal declaration"
    );
}
fn settle(game: &mut Game, mode: u64) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending ability");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, mode);
    }
}
fn play(game: &mut Game, mode: u64) {
    let step = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, mode);
    settle(game, mode);
}
fn activate(game: &mut Game, number: &str, line: u32) {
    let definition = selection_with_rules().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated && a.source.line == line)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", definition, PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, 1);
    settle(game, 1);
}
fn evolve(game: &mut Game) {
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    settle(game, 1);
}
#[test]
fn bp09_087_evolution_puts_a_matching_bishop_from_hand_into_the_field() {
    for number in aliases("BP09-087") {
        let mut input = initial("BP09-086", "field");
        input["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        input["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"wrong_class","card":"BP13-044"},{"id":"matching","card":"BP01-144"}]);
        let mut game = game(&input);
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "matching"]));
        assert_eq!(state["P1"]["hand"], json!(["wrong_class"]));
    }
}
#[test]
fn bp13_045_moves_a_mage_spell_to_ex_and_reduces_its_actual_play_cost() {
    for number in aliases("BP13-045") {
        let mut input = initial("BP13-044", "field");
        input["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        input["players"]["P1"]["zones"]["hand"] = json!([{"id":"wrong_trait","card":"SD03-016"},{"id":"wrong_kind","card":"BP13-044"},{"id":"spell","card":"BP01-073"}]);
        let mut game = game(&input);
        evolve(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["ex"],
            json!(["spell"])
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["wrong_trait", "wrong_kind"])
        );
        let step = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, step, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            8_u8
        );
    }
}
#[test]
fn cp03_119_inserts_the_chosen_hand_card_third_from_the_top_after_drawing() {
    for number in aliases("CP03-119") {
        let mut input = vanguard_initial(&number);
        input["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-037"}));
        let mut game = game(&input);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["deck"],
            json!(["tail1", "tail2", "old", "tail3"])
        );
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    }
}
#[test]
fn bp18_043_draws_three_before_banishing_two_hand_cards() {
    for number in aliases("BP18-043") {
        let mut input = initial(&number, "hand");
        input["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-037"}));
        let mut game = game(&input);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["old", "drawn"]));
        assert_eq!(state["P1"]["hand"], json!(["tail1", "tail2"]));
        assert_eq!(state["P1"]["deck"], json!(["tail3"]));
    }
}
#[test]
fn bp04_126_places_the_hand_card_on_the_bottom_before_drawing() {
    for number in aliases("BP04-126") {
        let mut input = initial(&number, "hand");
        input["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-037"}));
        let mut game = game(&input);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["tail1", "tail2", "tail3", "old"])
        );
    }
}
#[test]
fn bp15_124_evolution_draws_then_bottoms_one_hand_card() {
    for number in aliases("BP15-124") {
        let mut input = initial("BP15-123", "field");
        input["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        input["players"]["P1"]["zones"]["hand"] = json!([{"id":"old","card":"BP01-037"}]);
        let mut game = game(&input);
        evolve(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            state["P1"]["deck"],
            json!(["tail1", "tail2", "tail3", "old"])
        );
    }
}
#[test]
fn cp03_045_draw_and_banish_choice_uses_the_newly_updated_hand() {
    for number in aliases("CP03-045") {
        let mut input = vanguard_initial(&number);
        input["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"old","card":"BP01-037"}));
        let mut game = game(&input);
        play(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"], json!(["old"]));
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    }
}
#[test]
fn bp02_021_choice_puts_a_low_cost_follower_from_hand_into_the_field() {
    for number in aliases("BP02-021") {
        let mut input = initial(&number, "hand");
        input["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"too_costly","card":"BP01-037"}),
                json!({"id":"old","card":"BP01-144"}),
            ]);
        let mut game = game(&input);
        play(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"],
            json!(["source", "old"])
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["too_costly"])
        );
    }
}
#[test]
fn bp14_006_can_select_the_card_just_returned_from_ex_as_its_payment() {
    for number in aliases("BP14-006") {
        let mut input = initial(&number, "field");
        input["players"]["P1"]["zones"]["ex"] = json!([{"id":"returned","card":"BP01-037"}]);
        let mut game = game(&input);
        activate(&mut game, &number, 2);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["ex"], json!(["returned"]));
        assert_eq!(state["P1"]["hand"], json!([]));
        let definition = selection_with_rules().current[&number]
            .program
            .card()
            .abilities
            .iter()
            .find(|a| a.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        game.activate_internal_card("source", definition, PlayerId { seat: 1 })
            .unwrap_err();
    }
}

#[test]
fn cp03_045_second_mode_grants_storm_only_at_five_banished_cards() {
    for number in aliases("CP03-045") {
        for count in [4_u8, 5_u8] {
            let mut input = vanguard_initial(&number);
            input["players"]["P1"]["zones"]["banish"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("banished{i}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
            let mut game = game(&input);
            play(&mut game, 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                count == 5
            );
        }
    }
}
