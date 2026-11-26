use super::event_cemetery::finish_checks;
use super::*;

const GOLD: &str = "nightmare-seed-treasure";
const SEARCH_RESULT: &str = "nightmare-search-follower";

fn game_with_fixtures(initial: &Value) -> Game {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let (mut material, material_author) = current_fixture::author(
            GOLD,
            "a671bda5000000000000000000000001",
            "トレジャー・トークン",
            vec![json!({"activate":{
                "zone":"ex","cost":{"pp":0_u8},
                "targets":[{"bind":"target","from":{"who":"self","zone":"field","kind":"follower","trait":"グラブル"},"n":1_u8}],
                "do":[
                    {"limit_break":{"on":"target"}},
                    {"final_limit_break":{"on":"target"}}
                ]
            }})],
        );
        material["faces"][0]["power"] = json!("-");
        material["faces"][0]["hp"] = json!("-");
        snapshot.push_str(&material.to_string());
        snapshot.push('\n');
        input["authors"].as_array_mut().unwrap().push(material_author);
        let (mut follower, mut follower_author) = current_fixture::author(
            SEARCH_RESULT,
            "a671bda5000000000000000000000002",
            "フォロワー",
            vec![json!({"fanfare":{"do":[{"draw":{"n":1_u8}}]}})],
        );
        follower["faces"][0]["cost"] = json!("5");
        follower["faces"][0]["traits"] = json!(["グラブル"]);
        follower_author["input"]["source_payload"]["faces"][0]["cost"] = json!(5_u8);
        snapshot.push_str(&follower.to_string());
        snapshot.push('\n');
        input["authors"].as_array_mut().unwrap().push(follower_author);
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-nightmare",
    )
    .unwrap()
}

const P1: PlayerId = PlayerId { seat: 1 };

fn evolve(game: &mut Game) {
    let result = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(
        result.outcome, "resolved",
        "the printed evolution is payable"
    );
    finish_checks(game);
}

fn choose(game: &mut Game, question: &DecisionView, answer: Answer) -> CardPlay {
    game.answer_internal_card(question.actor, question.token, answer)
        .unwrap()
}

fn waiting(step: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = step else {
        panic!("expected an ability decision");
    };
    question
}

fn pending(game: &mut Game, printing: &str, line: u64) -> CardPlay {
    finish_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected printed pending abilities");
    };
    let option = options
        .iter()
        .find(|option| option.card == printing && option.line == line)
        .unwrap();
    choose(game, &question, Answer::Pending(option.handle))
}

fn target(game: &mut Game, step: CardPlay, printing: &str) -> CardPlay {
    let question = waiting(step);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a formal target");
    };
    let member = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(printing))
        .unwrap();
    choose(game, &question, Answer::Targets(vec![member.handle]))
}

fn field(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
    initial
}

fn danua() -> Value {
    let mut initial = field("CP05-GBF52");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-GBF53"}]);
    initial
}

fn break_with_gold(game: &mut Game) {
    let question = waiting(
        game.activate_internal_card("gold", AbilityId(1), P1)
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("the material declares the follower it breaks");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only the chosen subject has the Granblue trait"
    );
    resolved(choose(
        game,
        &question,
        Answer::Targets(vec![domain.members[0].handle]),
    ));
    finish_checks(game);
}

#[test]
fn nio_reduces_an_enemy_again_at_final_limit_break() {
    let mut initial = field("CP05-GBF46");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"gold","card":GOLD}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP14-022"}]);
    let mut game = game_with_fixtures(&initial);
    break_with_gold(&mut game);

    for (line, expected_power, expected_max_hp) in [(2_u64, 2_u8, 3_u8), (3, 0, 1)] {
        let step = pending(&mut game, "CP05-GBF46", line);
        resolved(target(&mut game, step, "BP14-022"));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["power"], expected_power);
        assert_eq!(view["objects"]["enemy"]["max_hp"], expected_max_hp);
    }
}

#[test]
fn evolved_ferry_draws_one_at_her_evolution() {
    let mut initial = field("CP05-076");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-GBF50"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    resolved(pending(&mut game, "CP05-GBF50", 3));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
}

#[test]
fn danua_uses_her_one_pp_printed_evolution_cost() {
    let mut game = game(&danua());
    evolve(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    assert_eq!(view["objects"]["source"]["power"], 2_u8);
    assert_eq!(view["objects"]["source"]["max_hp"], 2_u8);
}

#[test]
fn evolved_danua_searches_by_both_granblue_type_and_printed_cost() {
    let mut initial = danua();
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"low","card":"CP05-080"}, {"id":"found","card":SEARCH_RESULT}, {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game_with_fixtures(&initial);
    evolve(&mut game);
    let question = waiting(pending(&mut game, "CP05-GBF53", 1));
    let InputDomain::Search(domain) = &question.domain else {
        panic!("expected the private deck search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, SEARCH_RESULT);
    resolved(choose(
        &mut game,
        &question,
        Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["found"])
    );
}

fn pay_dark(game: &mut Game, step: CardPlay, printing: &str) -> CardPlay {
    let question = waiting(step);
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("expected private dark hand materials");
    };
    assert_eq!(
        domain.members.len(),
        1,
        "only an eligible dark card can pay"
    );
    assert_eq!(
        domain.members[0].card.as_deref(),
        Some(printing),
        "the paying card is the dark hand card"
    );
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the other player cannot see hand materials"
    );
    choose(
        game,
        &question,
        Answer::CostMaterials(vec![domain.members[0].handle]),
    )
}

#[test]
fn stone_break_pays_for_its_second_mode_in_the_opponents_quick_window() {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"spell","card":"CP05-P68"}, {"id":"fuel","card":"CP05-074"}, {"id":"wrong","card":"CP05-080"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("expected Quick when the opponent ends their turn");
    };
    let option = options
        .iter()
        .find(|option| option.card == "CP05-P68")
        .unwrap();
    let modes = waiting(choose(
        &mut game,
        &question,
        Answer::Quick(Some(option.handle)),
    ));
    assert!(matches!(modes.domain, InputDomain::Modes { .. }));
    let mut step = choose(&mut game, &modes, Answer::Modes(vec![ModeId(2)]));
    for _ in 0_u8..2 {
        let decision = waiting(step);
        step = match &decision.domain {
            InputDomain::OptionalCost => choose(&mut game, &decision, Answer::OptionalCost(true)),
            InputDomain::Targets(domain) => choose(
                &mut game,
                &decision,
                Answer::Targets(vec![domain.members[0].handle]),
            ),
            _ => panic!("second mode requires the optional fee and a formal target"),
        };
    }
    resolved(pay_dark(&mut game, step, "CP05-074"));
    finish_checks(&mut game);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 9_u8);
    assert_eq!(view["P1"]["hand"], json!(["wrong"]));
    assert_eq!(view["P2"]["leader"]["life"], 19_u8);
    assert_eq!(view["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn evolved_fediel_discards_two_to_draw_two_and_observes_only_the_dark_card() {
    let mut initial = field("CP05-074");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-GBF48"}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"dark","card":"CP05-074"},{"id":"other","card":"CP05-080"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-171"},{"id":"second","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game);
    let fee = waiting(pending(&mut game, "CP05-GBF48", 2));
    assert!(matches!(fee.domain, InputDomain::OptionalCost));
    let materials = waiting(choose(&mut game, &fee, Answer::OptionalCost(true)));
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected two hand materials for evolution");
    };
    assert_eq!(domain.members.len(), 2);
    resolved(choose(
        &mut game,
        &materials,
        Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect()),
    ));
    resolved(pending(&mut game, "CP05-GBF48", 1));
    finish_checks(&mut game);
    assert!(game.internal_pending_choice().unwrap().is_none());
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(view["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn super_evolved_fediel_returns_different_names_then_discards_one_card() {
    let mut initial = field("CP05-074");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-GBF48"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"same","card":"CP05-GBF47"},
        {"id":"repeat-first","card":"BP01-171"},
        {"id":"repeat-second","card":"BP01-171"},
        {"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}}),
        )
        .unwrap()
        .outcome,
        "resolved"
    );

    let targets = waiting(pending(&mut game, "CP05-GBF48", 3));
    let InputDomain::Targets(domain) = &targets.domain else {
        panic!("the printed ability declares its cemetery targets");
    };
    assert_eq!(domain.members.len(), 3);
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    assert!(
        domain
            .members
            .iter()
            .all(|member| member.card.as_deref() != Some("CP05-GBF47"))
    );
    let repeated: Vec<_> = domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some("BP01-171"))
        .map(|member| member.handle)
        .collect();
    assert_eq!(repeated.len(), 2);
    let first = repeated[0];
    let other = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap()
        .handle;
    game.answer_internal_card(targets.actor, targets.token, Answer::Targets(repeated))
        .unwrap_err();
    let discard = choose(&mut game, &targets, Answer::Targets(vec![first, other]));
    resolved(target(&mut game, discard, "BP01-037"));
    let view = game.projection(View::Referee).unwrap();
    let hand = view["P1"]["hand"].as_array().unwrap();
    assert_eq!(hand.len(), 1);
    assert_eq!(
        view["objects"][hand[0].as_str().unwrap()]["card"],
        "BP01-171"
    );
}

#[test]
fn evolved_lady_grey_gains_attack_when_the_dark_amulet_discards_a_card() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-084","state":{"evolved":true,"evolved_with":"form"}},
        {"id":"amulet","card":"CP05-079"}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"CP05-P65"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"dark","card":"CP05-074"},{"id":"other","card":"CP05-080"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = game(&initial);
    let ability = selection_with_rules().current["CP05-079"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 2)
        .unwrap()
        .node
        .id;
    let activation = game.activate_internal_card("amulet", ability, P1).unwrap();
    let look = waiting(pay_dark(&mut game, activation, "CP05-074"));
    assert!(matches!(look.domain, InputDomain::Integer(_)));
    resolved(choose(&mut game, &look, Answer::Integer(Nat(0))));
    resolved(pending(&mut game, "CP05-P65", 2));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["dark"]));
    assert_eq!(view["objects"]["source"]["power"], 2_u8);
}
