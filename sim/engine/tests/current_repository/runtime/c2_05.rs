use super::*;
use sve_engine::game::Step;
use sve_engine::ir::{
    AbilityOrigin, Action, CostSpec, Expr, Int, KeywordId, Resource, ResourceOperation,
};

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

fn position(active: &str) -> Value {
    let mut initial = setup(active, active, [20, 20]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial
}

fn play_into_field(initial: &Value, card: &str) -> Game {
    let mut initial = initial.clone();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"played","card":card}]);
    let mut game = game(&initial);
    let result = game.play_internal_card("played", actor("P1")).unwrap();
    assert!(
        matches!(result, CardPlay::Entered(_)),
        "the printed follower enters before combat"
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "these cards have no fanfare choice"
    );
    game
}

fn attack(game: &mut Game, attacker: &str, target: &str) -> Step {
    let step = game
        .internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        assert!(
            matches!(
                game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                    .unwrap(),
                CardPlay::Continued(_)
            ),
            "passing the quick window continues combat"
        );
    }
    step
}

fn blank_token_uses_its_printed_combat_stats(card: &str, token_survives: bool) {
    let mut initial = position("P1");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"token","card":card,"state":{"acted":true}}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP06-048"}]);
    let mut game = game(&initial);
    let before_combat = game.projection(View::Referee).unwrap();
    assert_eq!(
        before_combat["objects"]["token"]["card"],
        json!(card),
        "the target keeps the selected token identity"
    );
    assert_eq!(
        attack(&mut game, "attacker", "token").outcome,
        "resolved",
        "the follower can attack the rested vanilla token"
    );
    let after_combat = game.projection(View::Referee).unwrap();
    assert_eq!(
        after_combat["P2"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("token")),
        token_survives,
        "the printed token stats decide whether it survives"
    );
    assert!(
        !after_combat["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("attacker")),
        "the 1/1 attacker is destroyed in combat"
    );
}

#[test]
fn bp17_t11_blank_knight_fights_as_its_printed_vanilla_body() {
    blank_token_uses_its_printed_combat_stats("BP17-T11", false);
}

#[test]
fn bp17_t12_blank_steelclad_knight_fights_as_its_printed_vanilla_body() {
    blank_token_uses_its_printed_combat_stats("BP17-T12", true);
}

fn game_with_engine_rules(initial: &Value) -> Game {
    let selected = selection_with_rules().clone();
    Game::from_selected_effects(selected, initial, &Value::Null, &Value::Null, "c2-05").unwrap()
}

fn assert_evolve_pp_cost(card: &str) {
    let selected = selection().clone();
    let abilities = &selected.current[card].program.card().abilities;
    assert!(
        abilities.iter().any(|ability| {
            ability
                .node
                .play
                .as_ref()
                .is_some_and(|play| match &play.costs {
                    CostSpec::Ordered { steps } => steps.iter().any(|step| {
                        matches!(
                            &step.instruction.action,
                            Action::Resource {
                                resource: Resource::PpCurrent,
                                operation: ResourceOperation::Pay,
                                amount: Expr::Literal(Int(3)),
                                ..
                            }
                        )
                    }),
                    CostSpec::Alternative { .. } => false,
                })
        }),
        "{card} keeps its printed evolve PP cost of 3"
    );
}

#[test]
fn cp03_015_storm_can_attack_the_opponent_leader_on_the_turn_it_enters() {
    let mut game = game_with_engine_rules(&{
        let mut initial = position("P1");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"played","card":"CP03-015"}]);
        initial
    });
    assert!(matches!(
        game.play_internal_card("played", actor("P1")).unwrap(),
        CardPlay::Entered(_)
    ));
    game.resume_internal_card().unwrap();

    let result = attack(&mut game, "played", "P2.leader");
    assert_eq!(
        result.outcome, "resolved",
        "storm permits the leader attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        json!(19_u64),
        "the 1/1 follower deals its printed attack"
    );
}

#[test]
fn csd01_t01_remains_selected_without_a_card_ability() {
    let selected = selection().clone();
    assert!(
        selected.current.contains_key("CSD01-T01"),
        "the EP card remains in the current selection"
    );
    assert_eq!(
        selected.current["CSD01-T01"].program.card().abilities.len(),
        0,
        "the common EP rule reminder does not become a card ability"
    );
}

#[test]
fn dsd01b_005_keeps_aura_and_storm_and_can_attack_the_opponent_leader() {
    let selected = selection().clone();
    assert_eq!(
        selected.current["DSD01b-005"]
            .program
            .card()
            .abilities
            .len(),
        2,
        "aura and storm both remain printed abilities"
    );
    let abilities = &selected.current["DSD01b-005"].program.card().abilities;
    for keyword in [KeywordId::Aura, KeywordId::Storm] {
        assert!(
            abilities.iter().any(|ability| matches!(
                &ability.node.source.origin,
                AbilityOrigin::Keyword { keyword: found, .. } if *found == keyword
            )),
            "the lowered card retains its {keyword:?} keyword"
        );
    }
    let mut initial = position("P1");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"forte","card":"DSD01b-005"}]);
    let mut game = game(&initial);

    assert_eq!(
        attack(&mut game, "forte", "P2.leader").outcome,
        "resolved",
        "storm permits the evolved follower to attack the leader"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        json!(13_u64),
        "the 7/3 evolved follower deals its printed attack"
    );
}

#[test]
fn pr005_storm_attacks_the_opponent_leader_immediately_after_play() {
    assert_evolve_pp_cost("PR-005");
    let mut game = play_into_field(&position("P1"), "PR-005");

    assert_eq!(
        attack(&mut game, "played", "P2.leader").outcome,
        "resolved",
        "storm permits the leader attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        json!(19_u64),
        "the 1/1 follower deals its printed attack"
    );
}

#[test]
fn sd07_006_storm_attacks_the_opponent_leader_immediately_after_play() {
    assert_evolve_pp_cost("SD07-006");
    let mut game = play_into_field(&position("P1"), "SD07-006");

    assert_eq!(
        attack(&mut game, "played", "P2.leader").outcome,
        "resolved",
        "storm permits the leader attack"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        json!(17_u64),
        "the 3/5 follower deals its printed attack"
    );
}
