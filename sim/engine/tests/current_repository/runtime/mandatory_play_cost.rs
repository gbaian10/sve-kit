#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate printed payment scenarios."
)]
use super::*;

// Share the real repository boundary once; blank companions isolate material payment.
fn test_inputs() -> &'static (String, String, Value) {
    static INPUTS: OnceLock<(String, String, Value)> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, traits) in [
            ("cost-tree", "ナテラの大樹", "アミュレット", vec!["自然"]),
            ("cost-dragon", "material", "フォロワー", vec!["ドラゴニュート"]),
            ("cost-enemy", "enemy", "フォロワー", vec![]),
        ] {
            if !snapshot.ends_with('\n') { snapshot.push('\n'); }
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":kind,"traits":traits,
                "cost":"1","power":if kind == "アミュレット" {"-"} else {"1"},"hp":if kind == "アミュレット" {"-"} else {"20"},"text":null,"sections":[]
            }]}).to_string());
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        (snapshot, keywords, input)
    })
}
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = test_inputs();
        repository::selection_with_rules_from(snapshot, keywords, input)
    })
}
// Re-select author variants so an admitted definition is never overwritten.
fn variant(number: &str, change: impl FnOnce(&mut Value)) -> SelectedEffects {
    let (snapshot, keywords, mut input) = test_inputs().clone();
    let author = input["authors"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|author| author["input"]["card_no"] == number)
        .unwrap();
    change(&mut author["input"]);
    input["authors"]
        .as_array_mut()
        .unwrap()
        .retain(|candidate| candidate["input"]["card_no"] == number);
    input["documents"]
        .as_array_mut()
        .unwrap()
        .retain(|document| {
            document[0]
                .as_str()
                .is_some_and(|path| path.starts_with("cost-"))
        });
    repository::select(&snapshot, &keywords, &input).unwrap()
}
fn prepared(initial: &Value) -> Game {
    prepared_with(initial, selected().clone())
}
fn prepared_with(initial: &Value, selected: SelectedEffects) -> Game {
    let mut game = Game::from_selected_effects(
        selected,
        initial,
        &Value::Null,
        &Value::Null,
        "mandatory-play-cost",
    )
    .unwrap();
    let blank = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in ["cost-tree", "cost-dragon", "cost-enemy"] {
        game.load_internal_card(number, &blank).unwrap();
    }
    game
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"cost-enemy"}]);
    initial
}
fn finish(game: &mut Game, mut play: CardPlay) -> Activation {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            ),
            _ => panic!("unexpected question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play)
}
#[test]
fn bp12_024_acts_two_named_trees_before_spell_damage_and_rejects_insufficient_or_acted_materials() {
    for (count, acted) in [(2_usize, false), (1, false), (2, true)] {
        let mut initial = position("BP12-024");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..count)
                .map(
                    |i| json!({"id":format!("tree{i}"),"card":"cost-tree","state":{"acted":acted}})
                )
                .collect::<Vec<_>>()
        );
        let mut game = prepared(&initial);
        let before = game.projection(View::Referee).unwrap();
        let play = game.play_internal_card("spell", PlayerId { seat: 1 });
        if count < 2 || acted {
            assert!(
                matches!(play, Err(EngineFailure::IllegalPlay(_))),
                "payment rejection: {:?}",
                play.as_ref().err()
            );
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        let play = play.unwrap();
        assert!(
            matches!(&play, CardPlay::Awaiting(question) if matches!(question.domain, InputDomain::CostMaterials(_)))
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8
        );
        let result = finish(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["tree0"]["acted"], true);
        assert_eq!(state["objects"]["tree1"]["acted"], true);
        assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
        let played = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::CardPlayed { .. }))
            .unwrap();
        assert_eq!(
            result.progress.facts[..played]
                .iter()
                .filter(|fact| matches!(fact, Fact::Posture(_)))
                .count(),
            2
        );
    }
}
#[test]
fn bp13_070_sends_the_selected_dragonewt_to_cemetery_before_damage_and_draw() {
    let mut initial = position("BP13-070");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"cost-dragon"}]);
    let mut game = prepared(&initial);
    let play = game
        .play_internal_card("spell", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("material"))
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert!(
        state["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("drawn"))
    );
}
#[test]
fn bp14_011_banishes_ex_material_before_creating_three_puppets_and_cannot_use_opponents_ex() {
    for own in [true, false] {
        let mut initial = position("BP14-011");
        initial["players"][if own { "P1" } else { "P2" }]["zones"]["ex"] =
            json!([{"id":"material","card":"cost-tree"}]);
        let mut game = prepared(&initial);
        let before = game.projection(View::Referee).unwrap();
        let play = game.play_internal_card("spell", PlayerId { seat: 1 });
        if !own {
            assert!(
                matches!(play, Err(EngineFailure::IllegalPlay(_))),
                "payment rejection: {:?}",
                play.as_ref().err()
            );
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        finish(&mut game, play.unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P1"]["banish"]
                .as_array()
                .unwrap()
                .contains(&json!("material"))
        );
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3);
    }
}

#[test]
fn additional_cost_receipt_is_read_by_the_same_spell_and_failed_payment_restores_the_attempt() {
    let selection = variant("BP13-070", |input| {
        let faces = input["tree"]["cards"]["BP13-070"]["faces"]
            .as_object_mut()
            .unwrap();
        let face = faces.values_mut().next().unwrap();
        face["abilities"][0]["play"]["additional_cost"]["result"] = json!("extra");
        face["abilities"][1]["spell"]["do"] = json!([{"if":{"cond":{"paid":{"receipt":"extra"}},"then":[{"draw":{"who":"self","n":1_u8}}]}}]);
    });
    for pp in [0_u8, 10] {
        let mut initial = position("BP13-070");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"cost-dragon"}]);
        initial["players"]["P1"]["pp"]["current"] = json!(pp);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"material","card":"cost-dragon"}]);
        let mut game = prepared_with(&initial, selection.clone());
        let before = game.projection(View::Referee).unwrap();
        let mut play = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        let mut rejected = false;
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::CostMaterials(_) => Answer::CostMaterials(vec![0]),
                InputDomain::Targets(_) => Answer::Targets(vec![0]),
                _ => panic!("unexpected question"),
            };
            match game.answer_internal_card(question.actor, question.token, answer) {
                Ok(next) => play = next,
                Err(EngineFailure::IllegalPlay(_)) => {
                    rejected = true;
                    break;
                }
                Err(error) => panic!("unexpected failure: {error}"),
            }
        }
        assert_eq!(rejected, pp == 0);
        let state = game.projection(View::Referee).unwrap();
        if pp == 0 {
            assert_eq!(state, before);
        } else {
            assert!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("drawn"))
            );
        }
    }
}

#[test]
fn multiple_additional_groups_pay_in_order_with_a_discount_and_duplicate_act_payment_rolls_back_pp()
{
    let selection = variant("BP12-024", |input| {
        let face = input["tree"]["cards"]["BP12-024"]["faces"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        face["abilities"][0]["play"]["additional_cost"] = json!({"steps":[
            {"act":{"n":1_u8,"sel":{"who":"self","zone":"field","name":"ナテラの大樹"}}},
            {"act":{"n":1_u8,"sel":{"who":"self","zone":"field","name":"ナテラの大樹"}}}
        ]});
        face["abilities"][0]["play"]["cost_delta"] = json!(-1_i8);
    });
    for duplicate in [false, true] {
        let mut initial = position("BP12-024");
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"cost-dragon"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"first","card":"cost-tree"},{"id":"second","card":"cost-tree"}
        ]);
        let mut game = prepared_with(&initial, selection.clone());
        let before = game.projection(View::Referee).unwrap();
        let mut play = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap();
        let mut material = 0_u64;
        let mut rejected = false;
        while let CardPlay::Awaiting(question) = play {
            let answer = match &question.domain {
                InputDomain::CostMaterials(_) => {
                    let handle = if duplicate { 0 } else { material };
                    material += 1;
                    Answer::CostMaterials(vec![handle])
                }
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(usize::try_from(domain.cardinality.minimum.0).unwrap())
                        .map(|member| member.handle)
                        .collect(),
                ),
                _ => panic!("unexpected additional question"),
            };
            match game.answer_internal_card(question.actor, question.token, answer) {
                Ok(next) => play = next,
                Err(EngineFailure::IllegalPlay(_)) => {
                    rejected = true;
                    break;
                }
                Err(error) => panic!("unexpected payment failure: {error}"),
            }
        }
        assert_eq!(material, 2);
        assert_eq!(rejected, duplicate);
        let state = game.projection(View::Referee).unwrap();
        if duplicate {
            assert_eq!(
                state, before,
                "the second Act fails after PP and first Act, restoring both"
            );
        } else {
            assert_eq!(state["P1"]["pp"]["current"], 9_u8);
            assert_eq!(state["objects"]["first"]["acted"], true);
            assert_eq!(state["objects"]["second"]["acted"], true);
            assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
        }
    }
}
