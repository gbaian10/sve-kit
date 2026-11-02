#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::{DamageShare, SearchAnswer};
fn fixture() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
  let (mut snapshot,keywords,mut input)=producer_inputs();
  // Minimal companions isolate search filters, current costs, and ordinary evolution.
  for (number,name,kind,traits) in [
   ("ready-deremas","プリコネ companion","フォロワー",vec!["プリコネ","なかよし部","ディアボロス","ドラゴンズネスト"]),
   ("ready-rin","ヴァンガード material","フォロワー",vec!["プリコネ","〈ジオ・ニヴルヘル〉"]),
   ("ready-mio","ブラスター・ブレード","フォロワー",vec![]),
   ("ready-amulet","plain amulet","アミュレット",vec![]),
   ("ready-expensive","expensive princess","フォロワー",vec!["プリコネ"]),
   ("ready-tough","durable enemy","フォロワー",vec![]),
  ] {
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":if number=="ready-expensive" {"4"} else {"2"},"power":if kind=="フォロワー" {"3"} else {"-"},"hp":if number=="ready-tough" {"8"} else if kind=="フォロワー" {"4"} else {"-"},"traits":traits,"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  }
  let form = "CP04-056";
   let name=snapshot.lines().map(|line|serde_json::from_str::<Value>(line).unwrap()).find(|row|row["number"]==form).unwrap()["faces"][0]["name"].clone();
   let number=format!("ready-base-{form}");
   snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":"フォロワー","cost":"0","power":"3","hp":"4","traits":[],"text":null,"sections":[]}]}).to_string());snapshot.push('\n');
   input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"),json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
  repository::selection_with_rules_from(&snapshot,&keywords,&input)
 })
}
fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["sep"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("draw{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}
fn load_companions(game: &mut Game) {
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-deremas",
        "ready-rin",
        "ready-mio",
        "ready-amulet",
        "ready-expensive",
        "ready-tough",
    ] {
        game.load_internal_card(number, &empty).unwrap();
    }
}
fn game(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        fixture().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-cp04-2",
    )
    .unwrap();
    load_companions(&mut game);
    game
}
fn finish(game: &mut Game, mut step: CardPlay, mode: u64) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { .. } => Answer::Modes(vec![ModeId(mode)]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(mode != 2),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(if mode == 3 { 2 } else { 1 })
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
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
            }
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ),
            InputDomain::Distribution(domain) => Answer::Distribution(if mode == 3 {
                domain
                    .members
                    .iter()
                    .map(|m| DamageShare {
                        handle: m.handle,
                        amount: Nat(1),
                    })
                    .collect()
            } else {
                vec![DamageShare {
                    handle: domain.members[0].handle,
                    amount: domain.total,
                }]
            }),
            InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. } => {
                panic!("unexpected printed question: {:?}", question.domain)
            }
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
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
            panic!("pending");
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step, mode);
    }
}
fn play(game: &mut Game, id: &str, mode: u64) {
    let step = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, step, mode);
    settle(game, mode);
}
fn evolution_position(form: &str) -> Value {
    let mut initial = position(form);
    let base = format!("ready-base-{}", fixture().current[form].card_no);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn prepare_evolution(initial: &Value, form: &str) -> Game {
    let base = format!("ready-base-{}", fixture().current[form].card_no);
    let mut game = game(initial);
    let mut tree = expanded::tree().clone();
    tree["cards"] = json!({base.clone():tree["cards"]["synthetic-evolution"]});
    let program = lowerer().lower(&tree).unwrap().remove(&base).unwrap();
    game.load_internal_card(&base, &program).unwrap();
    let pay = if initial["players"]["P1"]["sep"] == 1_u8 {
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
        "the ordinary fixture evolution must resolve"
    );
    game
}
fn evolved(initial: &Value, form: &str) -> Game {
    let mut game = prepare_evolution(initial, form);
    settle(&mut game, 1);
    game
}
fn native_activate(game: &mut Game, card: &str, index: usize) {
    let ability = fixture().current[card]
        .program
        .card()
        .abilities
        .iter()
        .filter(|a| a.node.kind == AbilityKind::Activated)
        .nth(index)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(game, step, 1);
    settle(game, 1);
}

fn finish_attack(game: &mut Game, mode: u64) {
    loop {
        settle(game, mode);
        let question = game
            .internal_decision(View::Referee)
            .filter(|question| matches!(question.domain, InputDomain::Quick { .. }))
            .map_or_else(|| game.internal_quick_choice().unwrap(), Some);
        let Some(question) = question else { break };
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, step, mode);
    }
}
#[test]
fn cp04_049_searches_a_spell_without_taking_a_follower() {
    for card in ["CP04-049", "CP04-P37"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"wrong","card":"BP01-037"},{"id":"spell","card":"CP04-053"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["spell"])
        );
    }
}

#[test]
fn cp04_053_draws_only_with_a_friendship_follower() {
    for card in ["CP04-053", "CP04-P41"] {
        for friend in [false, true] {
            let mut initial = position(card);
            if friend {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-deremas"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
            assert_eq!(
                state["P1"]["hand"],
                if friend { json!(["draw0"]) } else { json!([]) }
            );
        }
    }
}

#[test]
fn cp04_056_super_evolution_adds_the_separate_all_enemy_damage() {
    for card in ["CP04-056", "CP04-SL14", "CP04-SP07", "CP04-U07"] {
        for super_evolution in [false, true] {
            let mut initial = evolution_position(card);
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"other","card":"ready-mio"}));
            if super_evolution {
                initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
                initial["players"]["P1"]["sep"] = json!(1_u8);
            }
            let game = evolved(&initial, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["cemetery"],
                if super_evolution {
                    json!(["enemy"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                state["objects"]["other"]["damage"],
                if super_evolution { 3_u8 } else { 0_u8 }
            );
            if !super_evolution {
                assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
            }
        }
    }
}

#[test]
fn cp04_063_awake_activation_hits_both_the_follower_and_leader() {
    for card in ["CP04-063", "CP04-P47", "PCS01-026"] {
        for maximum in [6_u8, 7] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            let mut game = game(&initial);
            if maximum == 6 {
                let ability = fixture().current[card]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.node.kind == AbilityKind::Activated)
                    .unwrap()
                    .node
                    .id;
                game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                    .unwrap_err();
            } else {
                native_activate(&mut game, card, 0);
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], maximum == 7);
            assert_eq!(state["objects"]["enemy"]["damage"], u8::from(maximum == 7));
            assert_eq!(
                state["P2"]["leader"]["life"],
                if maximum == 7 { 19_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn cp04_064_awake_activation_hits_both_the_follower_and_leader() {
    for card in ["CP04-064", "CP04-P48", "PR-498"] {
        for maximum in [6_u8, 7] {
            let mut initial = position(card);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
            let mut game = game(&initial);
            if maximum == 6 {
                let ability = fixture().current[card]
                    .program
                    .card()
                    .abilities
                    .iter()
                    .find(|a| a.node.kind == AbilityKind::Activated)
                    .unwrap()
                    .node
                    .id;
                game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                    .unwrap_err();
            } else {
                native_activate(&mut game, card, 0);
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], maximum == 7);
            assert_eq!(state["objects"]["enemy"]["damage"], u8::from(maximum == 7));
            assert_eq!(
                state["P2"]["leader"]["life"],
                if maximum == 7 { 19_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn cp04_069_pays_two_pp_on_attack_for_three_leader_damage() {
    for card in ["CP04-069", "CP04-P53", "PCS01-027"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        let mut game = game(&initial);
        let power = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
            .as_u64()
            .unwrap();
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        finish_attack(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
        assert_eq!(state["P2"]["leader"]["life"], 17 - power);
    }
}

#[test]
fn cp04_071_dragon_nest_changes_four_damage_to_six() {
    for card in ["CP04-071", "CP04-P55"] {
        for nest in [false, true] {
            let mut initial = position(card);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"ready-tough"}]);
            if nest {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"ready-deremas"}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["cemetery"], json!([]));
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if nest { 6_u8 } else { 4_u8 }
            );
        }
    }
}

#[test]
fn cp04_076_revives_only_a_low_original_cost_princess_follower() {
    for card in ["CP04-076", "CP04-SL20"] {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"wrong","card":"BP01-037"},{"id":"too-costly","card":"ready-expensive"},{"id":"revive","card":"ready-deremas"}]);
        let mut game = game(&initial);
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["source", "revive"]));
        let grave = state["P1"]["cemetery"].as_array().unwrap();
        assert_eq!(grave.len(), 2);
        assert!(grave.contains(&json!("wrong")));
        assert!(grave.contains(&json!("too-costly")));
    }
}

#[test]
fn cp04_079_draws_discards_and_then_mills_the_next_two_cards() {
    for card in ["CP04-079", "CP04-P59"] {
        let mut game = game(&position(card));
        play(&mut game, "source", 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["draw0", "draw1", "draw2"]));
        assert_eq!(state["P1"]["deck"], json!(["draw3", "draw4"]));
    }
}

#[test]
fn cp04_081_damages_only_when_its_entry_was_not_from_hand() {
    for card in ["CP04-081", "CP04-P61", "PCS01-034"] {
        for from_ex in [false, true] {
            let mut initial = position(card);
            if from_ex {
                initial["players"]["P1"]["zones"]["hand"] = json!([]);
                initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":card}]);
            }
            let mut game = game(&initial);
            play(&mut game, "source", 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if from_ex { 2_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn cp04_063_evolves_for_one_pp_then_064_searches_a_different_dragon_nest_card() {
    let mut initial = position("CP04-063");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-063"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP04-064"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"same-name","card":"CP04-063"},
        {"id":"wrong-trait","card":"ready-rin"},
        {"id":"searched","card":"ready-deremas"}
    ]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    settle(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["hand"], json!(["searched"]));
    let deck = state["P1"]["deck"].as_array().unwrap();
    assert_eq!(deck.len(), 2);
    assert!(deck.contains(&json!("same-name")));
    assert!(deck.contains(&json!("wrong-trait")));
}

#[test]
fn cp04_069_has_rush_and_storm_only_with_another_princess_follower() {
    for companion in [false, true] {
        let mut initial = position("CP04-069");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-069"}]);
        if companion {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"ready-rin"}));
        }
        let game = game(&initial);
        let state = game.projection(View::Referee).unwrap();
        let keywords = state["objects"]["source"]["keywords"].as_array().unwrap();
        assert!(keywords.contains(&json!("突進")));
        assert_eq!(keywords.contains(&json!("疾走")), companion);
    }
}

#[test]
fn cp04_076_requires_two_discards_to_move_itself_from_cemetery_to_ex() {
    for count in [1_u8, 2] {
        let mut initial = position("CP04-076");
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("material{i}"),"card":"ready-rin"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"CP04-076"}]);
        let mut game = game(&initial);
        if count == 1 {
            let ability = fixture().current["CP04-076"]
                .program
                .card()
                .abilities
                .iter()
                .find(|a| a.node.kind == AbilityKind::Activated)
                .unwrap()
                .node
                .id;
            game.activate_internal_card("source", ability, PlayerId { seat: 1 })
                .unwrap_err();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["material0"]));
            assert_eq!(state["P1"]["cemetery"], json!(["source"]));
            assert_eq!(state["P1"]["ex"], json!([]));
        } else {
            native_activate(&mut game, "CP04-076", 0);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!([]));
            assert_eq!(state["P1"]["cemetery"], json!(["material0", "material1"]));
            assert_eq!(state["P1"]["ex"], json!(["source"]));
        }
    }
}

#[test]
fn cp04_079_gains_ward_only_with_another_nivlhel_follower() {
    for companion in [false, true] {
        let mut initial = position("CP04-079");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-079"}]);
        if companion {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"ally","card":"ready-rin"}));
        }
        let game = game(&initial);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            companion
        );
    }
}

#[test]
fn cp04_069_can_decline_its_attack_cost_when_two_pp_are_unavailable() {
    let mut initial = position("CP04-069");
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP04-069"}]);
    let mut game = game(&initial);
    let power = game.projection(View::Referee).unwrap()["objects"]["source"]["power"]
        .as_u64()
        .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    finish_attack(&mut game, 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20 - power);
}

#[test]
fn cp04_053_cannot_be_played_without_an_enemy_follower_and_never_draws() {
    let mut initial = position("CP04-053");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"ready-deremas"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    game.play_internal_card("source", PlayerId { seat: 1 })
        .unwrap_err();
    let state = game.projection(View::Referee).unwrap();
    for key in ["hand", "cemetery", "deck", "pp"] {
        assert_eq!(state["P1"][key], before["P1"][key]);
    }
}

#[test]
fn cp04_056_can_distribute_one_damage_to_each_of_two_followers() {
    let mut initial = evolution_position("CP04-056");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other","card":"ready-mio"}));
    let mut game = prepare_evolution(&initial, "CP04-056");
    settle(&mut game, 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["other"]["damage"], 1_u8);
}

#[test]
fn cp04_076_can_enter_without_a_legal_revival_and_does_not_activate_ub() {
    let mut initial = position("CP04-076");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"too-costly","card":"ready-expensive"}]);
    let mut game = game(&initial);
    play(&mut game, "source", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["P1"]["cemetery"], json!(["too-costly"]));
    assert_eq!(
        state["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
        0_u8
    );
}
