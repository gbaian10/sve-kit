#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate the printed material scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn prepared(with_attack: bool) -> &'static SelectedEffects {
    static SELECTED: OnceLock<(SelectedEffects, SelectedEffects)> = OnceLock::new();
    let (exchange, triggered) = SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        input["authors"].as_array_mut().unwrap().retain(|author| {
            matches!(author["input"]["card_no"].as_str(), Some("CP04-036" | "CP04-106" | "CP04-016" | "BP07-024" | "BP07-T03" | "CP02-T01" | "BP03-T02"))
        });
        // Blank companions isolate the printed payment filter from unrelated subscriptions.
        for (number, name, kind) in [
            ("query-pecorine", "プリンセス・ペコリーヌ", "フォロワー"),
            ("query-wrong-kind", "ペコリーヌ", "アミュレット"),
            ("query-tree", "ナテラの大樹", "アミュレット"),
            ("query-enemy", "enemy", "フォロワー"),
            ("query-bayleon", "王たる光・ベイリオン", "フォロワー"),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"traits":[],"cost":"1","power":if kind=="フォロワー" {"1"} else {"-"},"hp":if kind=="フォロワー" {"20"} else {"-"},"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let triggered = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        // The exchange boundary rejects unsupported attack subscriptions even without a holder.
        input["authors"].as_array_mut().unwrap().retain(|author| author["input"]["card_no"] != "CP04-016");
        let exchange = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        (exchange, triggered)
    });
    if with_attack { triggered } else { exchange }
}
fn game(initial: &Value, with_attack: bool) -> Game {
    let mut game = Game::from_selected_effects(
        prepared(with_attack).clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "material-query",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "query-pecorine",
        "query-wrong-kind",
        "query-tree",
        "query-enemy",
        "query-bayleon",
    ] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}
fn finish(game: &mut Game, mut play: CardPlay, count: usize) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match question.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::Targets(_) => Answer::Targets(vec![0]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    count,
                    "only the matching unspent materials are offered"
                );
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
            }
            _ => panic!("unexpected input"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play);
}
#[test]
fn cp04_036_only_acts_a_matching_follower_and_deals_distinct_damage_to_its_leader() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-036"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fee","card":"query-pecorine"},
        {"id":"acted","card":"query-pecorine","state":{"acted":true}},
        {"id":"wrong-kind","card":"query-wrong-kind"},
        {"id":"wrong-name","card":"query-enemy"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"query-enemy"}]);
    let mut game = game(&initial, false);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["objects"]["fee"]["acted"], true);
    assert_eq!(state["objects"]["wrong-kind"]["acted"], false);
    assert_eq!(state["objects"]["wrong-name"]["acted"], false);
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}
#[test]
fn cp04_106_acts_itself_and_one_other_card_before_moving_the_target_to_its_owners_ex() {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":10_u8,"P2":10_u8});
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"CP04-106"}, {"id":"fee","card":"query-tree"},
        {"id":"acted","card":"query-tree","state":{"acted":true}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"query-enemy"}]);
    let mut game = game(&initial, false);
    let ability = prepared(false).current["CP04-106"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let target = awaiting(
        game.activate_internal_card("source", ability, PlayerId { seat: 1 })
            .unwrap(),
    );
    let material = awaiting(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &material.domain else {
        panic!("material")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("query-tree"));
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["objects"]["fee"]["acted"], true);
    assert_eq!(state["P2"]["ex"], json!(["enemy"]));
    assert_eq!(state["P1"]["ex"], json!([]));
}
#[test]
fn cp04_016_fanfare_cannot_use_itself_as_the_other_material() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"CP04-016"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"query-tree"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"query-enemy"}]);
    let mut game = game(&initial, true);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(play, CardPlay::Entered(_)));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending trigger")
    };
    assert_eq!(options.len(), 1);
    let declaration = awaiting(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(declaration.domain, InputDomain::OptionalCost);
    let target = awaiting(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(true),
        )
        .unwrap(),
    );
    let material = awaiting(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let InputDomain::CostMaterials(domain) = &material.domain else {
        panic!("material")
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("query-tree"));
    resolved(
        game.answer_internal_card(
            material.actor,
            material.token,
            Answer::CostMaterials(vec![0]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["ex"], json!(["fee"]));
    assert_eq!(state["objects"]["enemy"]["acted"], true);
}

#[test]
fn bp07_024_and_its_corrected_reprint_combine_both_discounts() {
    for number in ["BP07-024", "BP07-P06"] {
        for bayleon in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["pp"] = json!({"current":u8::from(!bayleon),"max":1_u8});
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"tree0","card":"BP07-T03"}, {"id":"tree1","card":"BP07-T03"}
            ]);
            if bayleon {
                initial["players"]["P1"]["zones"]["field"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"bayleon","card":"query-bayleon"}));
            }
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"query-enemy"}]);
            let mut game = game(&initial, false);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play, 2);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0_u8);
            assert_eq!(state["objects"]["tree0"]["acted"], true);
            assert_eq!(state["objects"]["tree1"]["acted"], true);
            assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        }
    }
}
