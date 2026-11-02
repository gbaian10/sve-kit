#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected choices invalidate the printed counter scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

// The named amulet isolates these effects without inventing an official Gigablade program.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        if !snapshot.ends_with('\n') {
            snapshot.push('\n');
        }
        snapshot.push_str(
            &json!({"number":"counter-light-blade","faces":[{
                "name":"ギガブレード","card_class":"ロイヤル","card_type":"アミュレット",
                "traits":[],"cost":"1","power":"-","hp":"-","text":null,"sections":[]
            }]})
            .to_string(),
        );
        input["documents"].as_array_mut().unwrap().push(json!([
            "counter-light-blade.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "counter-light-blade":{"status":"complete","abilities":[]}
            }})
            .to_string()
        ]));
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}
fn prepared(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selected().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "counter-light",
    )
    .unwrap();
    game.load_internal_card(
        "counter-light-blade",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}
fn definition(number: &str, line: u32) -> AbilityId {
    selected().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}
fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"damage":0_u8}}]);
    initial
}
fn finish(game: &mut Game, mut play: CardPlay, choose: &[&str], modes: &[u64], accept: bool) {
    let mut choices = choose.iter();
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Targets(domain) => {
                let member = choices.next().map_or_else(
                    || domain.members.first().unwrap(),
                    |number| {
                        domain
                            .members
                            .iter()
                            .find(|m| m.card.as_deref() == Some(*number))
                            .unwrap_or_else(|| panic!("the requested printed target is eligible"))
                    },
                );
                Answer::Targets(vec![member.handle])
            }
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the full own pool is eligible"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .first()
                    .map(|c| vec![c.handle])
                    .unwrap_or_default(),
            )),
            InputDomain::Modes { .. } => Answer::Modes(modes.iter().copied().map(ModeId).collect()),
            InputDomain::Integer(_) => Answer::Integer(Nat(u64::from(accept))),
            InputDomain::OptionalCost => Answer::OptionalCost(accept),
            InputDomain::Order(cards) => {
                Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected light counter decision"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        choices.next().is_none(),
        "all requested target decisions were presented"
    );
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed ability completes"
    );
}
fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn trigger(game: &mut Game, number: &str, line: u64, choose: Option<&str>, accept: bool) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    let selected = options
        .iter()
        .find(|p| p.card == number && p.line == line)
        .unwrap();
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(selected.handle))
        .unwrap();
    finish(game, play, choose.as_slice(), &[], accept);
    finish_checks(game);
}
fn play(game: &mut Game) {
    let play = game.play_internal_card("source", PLAYER).unwrap();
    finish(game, play, &[], &[], true);
    finish_checks(game);
}

#[test]
fn alice_and_printing_spend_the_fanfare_counter_on_a_real_ex_fairy_tale_follower() {
    for number in ["BP03-107", "BP03-SL20"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"field","card":"BP03-090"},{"id":"wrong","card":"BP14-059"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"extra","card":"BP03-004"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opposing","card":"BP03-004"}]);
        let mut game = prepared(&initial);
        play(&mut game);
        trigger(&mut game, number, 2, None, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["童話カウンター"],
            1_u8
        );
        let activation = game
            .activate_internal_card("source", definition(number, 3), PLAYER)
            .unwrap();
        let CardPlay::Awaiting(decision) = &activation else {
            panic!("Alice declares a formal target")
        };
        let InputDomain::Targets(domain) = &decision.domain else {
            panic!("Alice target domain")
        };
        let mut eligible: Vec<_> = domain
            .members
            .iter()
            .map(|member| member.card.as_deref().unwrap())
            .collect();
        eligible.sort_unstable();
        let mut expected = vec!["BP03-004", "BP03-090", number];
        expected.sort_unstable();
        assert_eq!(
            eligible, expected,
            "only own Field or EX fairy tale followers qualify"
        );
        finish(&mut game, activation, &["BP03-004"], &[], true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["source"]["counters"]["童話カウンター"],
            0_u8
        );
        assert_eq!(view["objects"]["extra"]["counters"]["童話カウンター"], 1_u8);
        for id in ["field", "wrong", "opposing"] {
            assert_eq!(
                view["objects"][id]["counters"]["童話カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                0
            );
        }
    }
}

#[test]
fn battle_tactics_and_printing_resolve_both_modes_on_a_real_field_kagemitsu() {
    for number in ["BP15-031", "PR-389"] {
        let mut initial = position(number);
        // The admitted evolved face is a real named receiver; its base's EX auto is outside this batch.
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"kagemitsu","card":"BP15-022"}]);
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"opposing","card":"BP15-022"}));
        let mut game = prepared(&initial);
        let result = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, result, &["BP01-037", "BP15-022"], &[1, 2], true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(
            view["objects"]["kagemitsu"]["counters"]["戦意カウンター"],
            1_u8
        );
        assert_eq!(
            view["objects"]["opposing"]["counters"]["戦意カウンター"]
                .as_u64()
                .unwrap_or(0),
            0
        );
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn mad_hatter_requires_another_fairy_tale_card_then_may_counter_the_searched_ex_generation() {
    for companion in [false, true] {
        for accept in [false, true] {
            let mut initial = position("BP07-016");
            if companion {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"ally","card":"BP03-004"}]);
            }
            initial["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"found","card":"BP03-004"},{"id":"wrong","card":"BP01-037"}]);
            let mut game = prepared(&initial);
            play(&mut game);
            trigger(&mut game, "BP07-016", 1, None, accept);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["ex"],
                if companion {
                    json!(["found"])
                } else {
                    json!([])
                }
            );
            assert_eq!(
                view["objects"]["found"]["counters"]["童話カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                u64::from(companion && accept)
            );
            assert_eq!(
                view["objects"]["source"]["counters"]["童話カウンター"]
                    .as_u64()
                    .unwrap_or(0),
                0
            );
            if companion {
                assert_eq!(view["objects"]["found"]["generation"], 1_u8);
            }
        }
    }
}

#[test]
fn lapis_and_all_printings_draw_the_prayer_count_that_preceded_their_departure() {
    for number in ["BP07-088", "BP07-SL18", "BP07-U06"] {
        for count in [0_u8, 3] {
            let mut initial = position("BP03-004");
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP03-004","state":{"counters":{"童話カウンター":1_u8}}}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"victim","card":number,"state":{"counters":{"祈りカウンター":count}}}]);
            initial["players"]["P2"]["zones"]["hand"] = json!([]);
            initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"d1","card":"BP01-037"},{"id":"d2","card":"BP01-037"},{"id":"d3","card":"BP01-037"}]);
            let mut game = prepared(&initial);
            let action = game
                .activate_internal_card("source", definition("BP03-004", 2), PLAYER)
                .unwrap();
            finish(&mut game, action, &[], &[], true);
            finish_checks(&mut game);
            trigger(&mut game, number, 2, None, true);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["hand"].as_array().unwrap().len(),
                usize::from(count)
            );
            assert_eq!(view["P2"]["cemetery"], json!(["victim"]));
            assert_eq!(
                view["objects"]["victim"]["counters"]["祈りカウンター"]
                    .as_u64()
                    .unwrap_or(0),
                0
            );
        }
    }
}

fn blade_pools(initial: &mut Value) {
    initial["players"]["P1"]["zones"]["field"].as_array_mut().unwrap().extend([
        json!({"id":"blade-1","card":"counter-light-blade","state":{"counters":{"ギガカウンター":10_u8}}}),
        json!({"id":"blade-2","card":"counter-light-blade","state":{"counters":{"ギガカウンター":3_u8}}})
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-blade","card":"counter-light-blade","state":{"counters":{"ギガカウンター":99_u8}}}]);
    initial["players"]["P2"]["zones"]["field"].as_array_mut().unwrap().push(json!({"id":"opposing-blade","card":"counter-light-blade","state":{"counters":{"ギガカウンター":88_u8}}}));
}
fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real admitted base performs its printed evolution"
    );
}

#[test]
fn shinra_and_all_printings_evolve_then_act_and_pay_a_gigablade_for_leader_damage() {
    for number in ["BP18-021", "BP18-SL06", "BP18-U02"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":if number == "BP18-SL06" { "BP18-SL05" } else { "BP18-020" }},{"id":"tokyo-1","card":"BP18-024"},{"id":"tokyo-2","card":"BP18-024"}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        blade_pools(&mut initial);
        let mut game = prepared(&initial);
        evolve(&mut game);
        trigger(&mut game, number, 1, Some("BP01-037"), true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8
        );
        let action = game
            .activate_internal_card("source", definition(number, 3), PLAYER)
            .unwrap();
        finish(&mut game, action, &[], &[], true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["leader"]["life"], 12_u8);
        assert_eq!(view["P1"]["leader"]["life"], 20_u8);
        assert_eq!(view["objects"]["source"]["acted"], true);
        assert_eq!(
            view["objects"]["blade-1"]["counters"]["ギガカウンター"],
            0_u8
        );
        assert_eq!(
            view["objects"]["blade-2"]["counters"]["ギガカウンター"],
            3_u8
        );
        assert_eq!(
            view["objects"]["ex-blade"]["counters"]["ギガカウンター"],
            99_u8
        );
        assert_eq!(
            view["objects"]["opposing-blade"]["counters"]["ギガカウンター"],
            88_u8
        );
    }
}

#[test]
fn surveyor_and_printing_evolve_then_place_two_on_each_own_field_gigablade_when_attacking() {
    for number in ["BP18-029", "BP18-P13"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":if number == "BP18-P13" { "BP18-P12" } else { "BP18-028" }}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        blade_pools(&mut initial);
        let mut game = prepared(&initial);
        evolve(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game, number, 2, None, true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["objects"]["blade-1"]["counters"]["ギガカウンター"],
            12_u8
        );
        assert_eq!(
            view["objects"]["blade-2"]["counters"]["ギガカウンター"],
            5_u8
        );
        assert_eq!(
            view["objects"]["ex-blade"]["counters"]["ギガカウンター"],
            99_u8
        );
        assert_eq!(
            view["objects"]["opposing-blade"]["counters"]["ギガカウンター"],
            88_u8
        );
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn heretical_cooperation_and_printing_search_real_shinra_then_counter_every_own_field_blade() {
    for number in ["BP18-031", "PR-471"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP18-020"}]);
        blade_pools(&mut initial);
        let mut game = prepared(&initial);
        let result = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, result, &[], &[], true);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["hand"], json!(["found"]));
        assert_eq!(view["P1"]["cemetery"], json!(["source"]));
        assert_eq!(
            view["objects"]["blade-1"]["counters"]["ギガカウンター"],
            12_u8
        );
        assert_eq!(
            view["objects"]["blade-2"]["counters"]["ギガカウンター"],
            5_u8
        );
        assert_eq!(
            view["objects"]["ex-blade"]["counters"]["ギガカウンター"],
            99_u8
        );
        assert_eq!(
            view["objects"]["opposing-blade"]["counters"]["ギガカウンター"],
            88_u8
        );
    }
}
