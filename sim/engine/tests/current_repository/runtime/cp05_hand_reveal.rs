
use super::*;

const FIRE: &str = "reveal-test-fire";
const DARK: &str = "reveal-test-dark";

fn reveal_game(initial: &Value) -> Game {
    static SELECTION: OnceLock<SelectedEffects> = OnceLock::new();
    let selected = SELECTION.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (printing, attribute) in [(FIRE, "火属性"), (DARK, "闇属性")] {
            snapshot.push_str(&json!({
                "number": printing,
                "faces": [{
                    "name": printing, "card_class": "ドラゴン", "card_type": "フォロワー",
                    "traits": [attribute], "cost": "1", "power": "1", "hp": "1",
                    "text": null, "sections": [],
                }],
            }).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{printing}.yaml"),
                json!({"format":1_u8,"kind":"fixture_cards","cards":{printing:{"status":"complete"}}}).to_string(),
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    });
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "cp05-hand-reveal",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for printing in [FIRE, DARK] {
        game.load_internal_card(printing, &blank).unwrap();
    }
    game
}

fn start_pending(game: &mut Game, card: &str, line: u64) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger pending");
    };
    let option = options
        .iter()
        .find(|option| option.card == card && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

fn select_target(game: &mut Game, play: CardPlay, printing: &str) -> CardPlay {
    let CardPlay::Awaiting(question) = play else {
        panic!("formal target required");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected formal targets");
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(printing))
        .unwrap()
        .handle;
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![handle]),
    )
    .unwrap()
}

#[test]
fn cp05_068_attack_reveals_exactly_three_fire_cards_and_each_copy_observes_itself() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-P52"}, {"id":"ally","card":FIRE}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"first","card":"CP05-068"}, {"id":"second","card":"CP05-P52"},
        {"id":"third","card":FIRE}, {"id":"wrong","card":DARK}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = reveal_game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let CardPlay::Awaiting(payment) = start_pending(&mut game, "CP05-P52", 3) else {
        panic!("head payment declaration");
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let target = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap();
    let CardPlay::Awaiting(materials) = select_target(&mut game, target, "BP01-037") else {
        panic!("private materials required");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected hand material choice");
    };
    assert_eq!(domain.members.len(), 3);
    assert!(game.internal_decision(View::P2).is_none());
    let handles = domain
        .members
        .iter()
        .map(|member| member.handle)
        .collect::<Vec<_>>();
    game.answer_internal_card(
        materials.actor,
        materials.token,
        Answer::CostMaterials(handles[..2].to_vec()),
    )
    .unwrap_err();
    assert!(game.projection(View::P2).unwrap()["objects"]["first"].is_null());
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(handles),
        )
        .unwrap(),
    );

    game.resume_internal_card().unwrap();
    let attack_result = game.projection(View::Referee).unwrap();
    assert_eq!(attack_result["P2"]["leader"]["life"], 14_u8);
    assert_eq!(attack_result["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(
        attack_result["P1"]["hand"],
        json!(["first", "second", "third", "wrong"])
    );
    for id in ["first", "second", "third", "wrong"] {
        assert!(game.projection(View::P2).unwrap()["objects"][id].is_null());
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("both revealed copies are pending");
    };
    assert_eq!(options.len(), 2);
    for printing in ["CP05-068", "CP05-P52"] {
        assert!(
            options
                .iter()
                .any(|option| option.card == printing && option.line == 1)
        );
        let reveal = start_pending(&mut game, printing, 1);
        resolved(select_target(&mut game, reveal, FIRE));
        game.resume_internal_card().unwrap();
    }
    let after = game.projection(View::Referee).unwrap();
    let keywords = after["objects"]["ally"]["keywords"].as_array().unwrap();
    assert!(keywords.contains(&json!("突進")));
    assert!(keywords.contains(&json!("指定攻撃")));
}

#[test]
fn cp05_068_observes_its_owners_whole_hand_reveal_on_the_opponents_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"detective","card":"BP06-118"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"own_ally","card":FIRE}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"opponents_card","card":"CP05-068"}]);
    let mut game = reveal_game(&initial);
    assert!(matches!(
        game.play_internal_card("detective", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(start_pending(&mut game, "BP06-118", 1));
    game.resume_internal_card().unwrap();
    let reveal = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(reveal.actor, PlayerId { seat: 2 });
    let InputDomain::Pending(options) = &reveal.domain else {
        panic!("the revealed printing must have a pending ability");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "CP05-068");
    assert!(game.projection(View::P1).unwrap()["objects"]["opponents_card"].is_null());
    let target = start_pending(&mut game, "CP05-068", 1);
    resolved(select_target(&mut game, target, FIRE));
    game.resume_internal_card().unwrap();
    let view = game.projection(View::Referee).unwrap();
    let keywords = view["objects"]["own_ally"]["keywords"].as_array().unwrap();
    assert!(keywords.contains(&json!("突進")));
    assert!(keywords.contains(&json!("指定攻撃")));
}

#[test]
fn cp05_068_does_not_observe_revelation_in_the_deck() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP19-113"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"revealed","card":"CP05-068"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":FIRE}]);
    let mut game = reveal_game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    resolved(start_pending(&mut game, "BP19-113", 1));
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    let public = game.projection(View::P2).unwrap();
    assert!(
        public["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|card| card["id"] == "revealed")
    );
}

#[test]
fn cp05_068_keeps_its_pending_ability_when_random_hand_revelation_moves_it() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP11-112"}, {"id":"ally","card":FIRE}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"revealed","card":"CP05-P52"}, {"id":"other","card":DARK}
    ]);
    let mut game = reveal_game(&initial);
    let printed = selection_with_rules().current["BP11-112"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == 3)
        .unwrap()
        .node
        .id;
    resolved(
        game.activate_internal_card("source", printed, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert!(
        game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("revealed"))
    );
    let target = start_pending(&mut game, "CP05-P52", 1);
    resolved(select_target(&mut game, target, FIRE));
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert!(
        after["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}

#[test]
fn cp05_057s_variable_reveal_also_triggers_cp05_068_in_hand() {
    let mut initial = draw_position();
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP05-056"}, {"id":"ally","card":FIRE}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-057"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"revealed","card":"CP05-P52"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    let mut game = reveal_game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}
        }))
        .unwrap()
        .outcome,
        "resolved"
    );
    let CardPlay::Awaiting(payment) = start_pending(&mut game, "CP05-057", 2) else {
        panic!("the evolved card offers a reveal cost");
    };
    assert_eq!(payment.domain, InputDomain::OptionalCost);
    let enemy = game
        .answer_internal_card(payment.actor, payment.token, Answer::OptionalCost(true))
        .unwrap();
    let CardPlay::Awaiting(materials) = select_target(&mut game, enemy, "BP04-117") else {
        panic!("the printed reveal must ask for private materials");
    };
    let InputDomain::CostMaterials(domain) = &materials.domain else {
        panic!("expected variable Fire materials");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("CP05-P52"));
    resolved(
        game.answer_internal_card(
            materials.actor,
            materials.token,
            Answer::CostMaterials(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );

    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19_u8
    );
    let grant = start_pending(&mut game, "CP05-P52", 1);
    resolved(select_target(&mut game, grant, FIRE));
    game.resume_internal_card().unwrap();
    let result = game.projection(View::Referee).unwrap();
    assert!(
        result["objects"]["ally"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("指定攻撃"))
    );
}
