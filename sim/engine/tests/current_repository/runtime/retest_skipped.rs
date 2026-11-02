#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        // Minimal companions isolate class, trait and name filters from unrelated card text.
        for (number, name, class, kind, traits) in [
            ("retest-royal", "royal ally", "ロイヤル", "フォロワー", vec![]),
            ("retest-royal-amulet", "royal amulet", "ロイヤル", "アミュレット", vec![]),
            ("retest-neutral", "neutral ally", "ニュートラル", "フォロワー", vec![]),
            ("retest-machine", "machine card", "ニュートラル", "アミュレット", vec!["機械"]),
            ("retest-mono", "真紅の抗戦者・モノ", "ナイトメア", "フォロワー", vec![]),
            ("retest-tough", "durable enemy", "ニュートラル", "フォロワー", vec![]),
            ("retest-killer", "strong enemy", "ニュートラル", "フォロワー", vec![]),
            ("retest-idol", "idol card", "ニュートラル", "フォロワー", vec!["デレマス"]),
        ] {
            let follower = kind == "フォロワー";
            snapshot.push_str(
                &json!({"number":number,"faces":[{"name":name,"card_class":class,"card_type":kind,
                    "cost":"1","power":if number == "retest-killer" {"5"} else if follower {"1"} else {"-"},
                    "hp":match number { "retest-tough" => "9", "retest-killer" => "5", _ if follower => "1", _ => "-" },
                    "traits":traits,"text":null,"sections":[]}]})
                .to_string(),
            );
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        // Same-name partners isolate evolution from the partners' own printed text.
        for (card, number, kind, cost) in [
            ("BP16-024", "retest-base-BP16-024", "フォロワー", "1"),
            ("BP19-022", "retest-form-BP19-022", "フォロワー・エボルヴ", "-"),
        ] {
            let record = snapshot
                .lines()
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .find(|row| row["number"] == card)
                .unwrap();
            let face = &record["faces"][0];
            snapshot.push_str(
                &json!({"number":number,"faces":[{"name":face["name"],"card_class":face["card_class"],
                    "card_type":kind,"cost":cost,"power":"6","hp":"6",
                    "traits":face["traits"],"text":"","sections":[]}]})
                .to_string(),
            );
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

const COMPANIONS: [&str; 9] = [
    "retest-idol",
    "retest-form-BP19-022",
    "retest-royal",
    "retest-royal-amulet",
    "retest-neutral",
    "retest-machine",
    "retest-mono",
    "retest-tough",
    "retest-killer",
];

fn new_game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "retest-skipped",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in COMPANIONS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"retest-tough"}]);
    initial
}

#[derive(Clone, Copy)]
struct Policy {
    mode: u64,
    targets: usize,
}

const ACCEPT: Policy = Policy {
    mode: 1,
    targets: 1,
};

fn answer(question: &DecisionView, policy: Policy) -> Answer {
    match &question.domain {
        InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(policy.mode)]),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        InputDomain::OptionalCost => Answer::OptionalCost(true),
        InputDomain::Targets(domain) => Answer::Targets(
            domain
                .members
                .iter()
                .take(policy.targets)
                .map(|m| m.handle)
                .collect(),
        ),
        InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
            domain
                .cards
                .first()
                .map(|m| vec![m.handle])
                .unwrap_or_default(),
        )),
        InputDomain::Order(cards) => Answer::Order(cards.iter().rev().map(|m| m.handle).collect()),
        InputDomain::CostMaterials(domain) => Answer::CostMaterials(
            domain
                .members
                .iter()
                .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                .map(|m| m.handle)
                .collect(),
        ),
        other => panic!("unexpected printed question: {other:?}"),
    }
}

fn finish(game: &mut Game, mut step: CardPlay, policy: Policy) {
    while let CardPlay::Awaiting(question) = step {
        let reply = answer(&question, policy);
        step = game
            .answer_internal_card(question.actor, question.token, reply)
            .unwrap();
    }
}

fn settle(game: &mut Game, policy: Policy) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, policy);
    }
}

fn play(game: &mut Game, id: &str, policy: Policy) {
    let step = game.play_internal_card(id, PLAYER).unwrap();
    finish(game, step, policy);
    settle(game, policy);
}

fn activated(card: &str) -> AbilityId {
    fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id
}

fn activate(game: &mut Game, id: &str, card: &str, policy: Policy) {
    let step = game
        .activate_internal_card(id, activated(card), PLAYER)
        .unwrap();
    finish(game, step, policy);
    settle(game, policy);
}

/// Evolves a synthetic same-name base into the printed form, optionally as a super evolution.
fn evolved(initial: &mut Value, form: &str, super_evolution: bool, policy: Policy) -> Game {
    let base = "retest-base-BP16-024".to_owned();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .map(|field| field.insert(0, json!({"id":"source","card":base})))
        .unwrap_or_else(|| {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
        });
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    if super_evolution {
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["sep"] = json!(1_u8);
    }
    let mut game = new_game(initial);
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base.clone():tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(&base).unwrap();
    game.load_internal_card(&base, &program).unwrap();
    let pay = if super_evolution {
        json!({"pp":1_u8,"sep":1_u8})
    } else {
        json!({"pp":1_u8})
    };
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":pay})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the synthetic base evolution must resolve"
    );
    settle(&mut game, policy);
    game
}

fn state(game: &Game) -> Value {
    game.projection(View::Referee).unwrap()
}

fn keywords(view: &Value, id: &str) -> Vec<Value> {
    view["objects"][id]["keywords"].as_array().unwrap().clone()
}

fn field_tokens(view: &Value) -> Vec<String> {
    view["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .filter(|id| id != "source")
        .map(|id| {
            view["objects"][&id]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

#[test]
fn bp16_024_evolution_makes_two_soldiers_and_super_evolution_arms_two_tokens() {
    for card in ["BP16-024", "BP16-P06", "SD07-009"] {
        let mut made = field_tokens(&state(&evolved(&mut position(card), card, false, ACCEPT)));
        made.sort();
        assert_eq!(made, ["シールドガーディアン", "スティールナイト"]);

        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"knight","card":"BP17-T12"},
            {"id":"printed","card":"retest-royal"}
        ]);
        let game = evolved(
            &mut initial,
            card,
            true,
            Policy {
                targets: 2,
                ..ACCEPT
            },
        );
        let view = state(&game);
        let armed: Vec<&Value> = view["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|id| keywords(&view, id.as_str().unwrap()).contains(&json!("疾走")))
            .collect();
        assert_eq!(armed.len(), 2, "up to two soldier tokens are chosen");
        for id in armed {
            let object = &view["objects"][id.as_str().unwrap()];
            assert!(object["card"].as_str().unwrap().contains("-T"));
            assert!(object["power"].as_u64().unwrap() >= 2);
        }
        assert_eq!(
            view["objects"]["printed"]["power"], 1_u8,
            "only soldier tokens qualify"
        );
        assert!(!keywords(&view, "printed").contains(&json!("疾走")));
    }
}

fn soldiers() -> Value {
    json!([
        {"id":"steel","card":"BP17-T12"},
        {"id":"shield","card":"BP17-T13"},
        {"id":"knight","card":"BP17-T11"}
    ])
}

#[test]
fn bp16_033_makes_a_knight_and_arms_a_soldier_with_three_soldiers() {
    for card in ["BP16-033", "PR-425"] {
        let mut fanfare = new_game(&position(card));
        play(&mut fanfare, "source", ACCEPT);
        assert_eq!(field_tokens(&state(&fanfare)), ["スティールナイト"]);

        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        let mut field = vec![json!({"id":"source","card":card})];
        field.extend(soldiers().as_array().unwrap().iter().cloned());
        initial["players"]["P1"]["zones"]["field"] = Value::Array(field);
        let mut game = new_game(&initial);
        activate(&mut game, "source", card, ACCEPT);
        let view = state(&game);
        assert_eq!(view["objects"]["steel"]["power"], 4_u8);
        assert!(keywords(&view, "steel").contains(&json!("疾走")));
        game.activate_internal_card("source", activated(card), PLAYER)
            .unwrap_err();
    }
}

fn finish_attack(game: &mut Game) {
    loop {
        settle(game, ACCEPT);
        let question = game
            .internal_decision(View::Referee)
            .filter(|question| matches!(question.domain, InputDomain::Quick { .. }))
            .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
        let Some(question) = question else { break };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, ACCEPT);
    }
}

#[test]
fn bp17_019_attack_buff_counts_five_royal_cards_and_fanfare_picks_a_token() {
    for card in ["BP17-019", "BP17-SL05", "BP17-U03"] {
        let mut fanfare = new_game(&position(card));
        play(&mut fanfare, "source", Policy { mode: 2, ..ACCEPT });
        assert_eq!(field_tokens(&state(&fanfare)).len(), 1);

        for royal in [false, true] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            let last = if royal {
                "retest-royal-amulet"
            } else {
                "retest-neutral"
            };
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":card},
                {"id":"r1","card":"retest-royal"},
                {"id":"r2","card":"retest-royal"},
                {"id":"r3","card":"retest-royal"},
                {"id":"last","card":last}
            ]);
            let mut game = new_game(&initial);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            finish_attack(&mut game);
            assert_eq!(
                state(&game)["P2"]["leader"]["life"],
                if royal { 17_u8 } else { 19_u8 },
                "a Royal amulet counts as a Royal card"
            );
        }
    }
}

#[test]
fn bp19_022_fanfare_evolves_itself_when_five_royal_cards_are_present() {
    for card in ["BP19-022", "BP19-SL08", "PR-558"] {
        for royal in [false, true] {
            let mut initial = position(card);
            let other = if royal {
                "retest-royal"
            } else {
                "retest-neutral"
            };
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"royal","card":"retest-royal"},{"id":"other","card":other}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"retest-form-BP19-022"}]);
            let mut game = new_game(&initial);
            play(&mut game, "source", ACCEPT);
            let view = state(&game);
            assert_eq!(view["P1"]["field"].as_array().unwrap().len(), 5);
            assert_eq!(view["objects"]["source"]["evolved"], royal);
        }
    }
}

fn named(view: &Value, zone: &str, name: &str) -> usize {
    view["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| view["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp12_079_burns_both_leaders_or_makes_product_machines_with_mono() {
    for card in ["BP12-079", "BP12-SL22"] {
        let mut burn = new_game(&position(card));
        play(&mut burn, "source", ACCEPT);
        let burned = state(&burn);
        assert_eq!(burned["P2"]["leader"]["life"], 18_u8);
        assert_eq!(burned["P1"]["leader"]["life"], 19_u8);
        for mono in [false, true] {
            let mut initial = position(card);
            if mono {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"mono","card":"retest-mono"}]);
            }
            let mut game = new_game(&initial);
            play(&mut game, "source", Policy { mode: 2, ..ACCEPT });
            let view = state(&game);
            assert_eq!(
                named(&view, "field", "プロダクトマシーン"),
                if mono { 2 } else { 1 }
            );
            assert_eq!(named(&view, "ex", "プロダクトマシーン"), usize::from(!mono));
        }
    }
}

fn dies_attacking(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"killer"}))
        .unwrap();
    finish_attack(game);
}

#[test]
fn bp17_043_end_phase_damage_needs_three_ex_machines_and_lastword_stores_a_machine() {
    for machines in [2_usize, 3] {
        let mut initial = position("BP17-043");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-043"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..machines)
                .map(|i| json!({"id":format!("machine{i}"),"card":"retest-machine"}))
                .collect::<Vec<_>>()
        );
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut game, ACCEPT);
        let view = state(&game);
        assert_eq!(
            view["objects"]["enemy"]["damage"],
            if machines == 3 { 3_u8 } else { 0_u8 }
        );
        assert_eq!(
            view["P1"]["hand"],
            json!(["draw0"]),
            "the draw is not conditional"
        );
    }
    let mut initial = position("BP17-043");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-043"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"killer","card":"retest-killer","state":{"acted":true}}]);
    let mut game = new_game(&initial);
    dies_attacking(&mut game);
    assert_eq!(named(&state(&game), "ex", "プロダクトマシーン"), 1);
}

#[test]
fn bp17_047_fanfare_needs_three_ex_machines_and_lastword_stores_a_machine() {
    for card in ["BP17-047", "BP17-P12"] {
        for machines in [2_usize, 3] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0..machines)
                    .map(|i| json!({"id":format!("machine{i}"),"card":"retest-machine"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"killer","card":"retest-killer","state":{"acted":true}}]);
            let mut game = new_game(&initial);
            play(&mut game, "source", ACCEPT);
            let view = state(&game);
            let boosted = machines == 3;
            assert_eq!(
                view["objects"]["source"]["power"],
                if boosted { 4_u8 } else { 1_u8 }
            );
            assert_eq!(
                keywords(&view, "source").contains(&json!("指定攻撃")),
                boosted
            );
            assert!(keywords(&view, "source").contains(&json!("突進")));
            if !boosted {
                dies_attacking(&mut game);
                assert_eq!(named(&state(&game), "ex", "プロダクトマシーン"), 1);
            }
        }
    }
}

fn deck(view: &Value) -> Vec<String> {
    view["P1"]["deck"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn ecp02_063_keeps_a_chosen_card_on_top_and_sends_the_rest_to_the_bottom() {
    for card in ["ECP02-063", "ECP02-SL31"] {
        let mut fanfare = new_game(&position(card));
        play(&mut fanfare, "source", ACCEPT);
        let order = deck(&state(&fanfare));
        assert_eq!(order.len(), 5);
        let seen = ["draw0", "draw1"];
        assert!(
            seen.contains(&order[0].as_str()),
            "one looked card stays on top"
        );
        assert!(
            seen.contains(&order[4].as_str()),
            "the other goes to the bottom"
        );
        assert_ne!(order[0], order[4]);
        assert_eq!(order[1..4], ["draw2", "draw3", "draw4"]);

        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0_u8..3)
                .map(|i| json!({"id":format!("item{i}"),"card":"PR-155"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"idol","card":"retest-idol"},{"id":"tail","card":"BP01-171"}]);
        let mut game = new_game(&initial);
        activate(&mut game, "source", card, ACCEPT);
        let view = state(&game);
        assert_eq!(view["P1"]["hand"], json!(["idol"]));
        assert_eq!(
            view["P1"]["ex"].as_array().unwrap().len(),
            1,
            "Lesson 2 banishes two items"
        );
        game.activate_internal_card("source", activated(card), PLAYER)
            .unwrap_err();
    }
}
