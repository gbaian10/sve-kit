#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed counter-cost scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

// This named pool isolates counter payment without pretending to admit Gigablade's other abilities.
fn selected() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        if !snapshot.ends_with('\n') {
            snapshot.push('\n');
        }
        snapshot.push_str(
            &json!({"number":"counter-cost-blade","faces":[{
                "name":"ギガブレード","card_class":"ロイヤル","card_type":"アミュレット",
                "traits":[],"cost":"1","power":"-","hp":"-","text":null,"sections":[]
            }]})
            .to_string(),
        );
        input["documents"].as_array_mut().unwrap().push(json!([
            "counter-cost-blade.yaml",
            json!({"format":1_u8,"kind":"effect_set","cards":{
                "counter-cost-blade":{"status":"complete","abilities":[]}
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
        "counter-cost-holders",
    )
    .unwrap();
    game.load_internal_card(
        "counter-cost-blade",
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
        .find(|entry| entry.source.line == line)
        .unwrap()
        .node
        .id
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    1,
                    "only the own named pool with the full balance can pay"
                );
                assert_eq!(
                    domain.members[0].player, PLAYER,
                    "the payer owns the complete counter pool"
                );
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected payment decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed effect completes"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "exactly the printed entry or evolution trigger is pending"
    );
    assert_eq!(
        options[0].line, 1_u64,
        "only the printed entry trigger is pending"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
    game.resume_internal_card().unwrap();
}

#[test]
fn furnace_dragon_and_printing_evolve_then_pay_a_real_hot_spring_once_per_turn() {
    for number in ["BP14-061", "BP14-P15"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"BP14-060","state":{"counters":{"神湯カウンター":99_u8}}},
            {"id":"pool","card":"BP14-059","state":{"counters":{"神湯カウンター":4_u8}}},
            {"id":"wrong-name","card":"counter-cost-blade","state":{"counters":{"神湯カウンター":99_u8}}}
        ]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"ex","card":"BP14-059","state":{"counters":{"神湯カウンター":99_u8}}}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"draw-1","card":"BP01-037"},{"id":"draw-2","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"damage":0_u8}},
            {"id":"opponent","card":"BP14-059","state":{"counters":{"神湯カウンター":99_u8}}}
        ]);
        let mut game = prepared(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
        let play = game
            .activate_internal_card("source", definition(number, 2), PLAYER)
            .unwrap();
        finish(&mut game, play);
        game.resume_internal_card().unwrap();
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["pool"]["counters"]["神湯カウンター"], 2_u8);
        assert_eq!(view["P1"]["hand"], json!(["draw-1"]));
        assert_eq!(view["P1"]["pp"]["current"], 9_u8);
        for id in ["source", "wrong-name", "ex", "opponent"] {
            assert_eq!(view["objects"][id]["counters"]["神湯カウンター"], 99_u8);
        }
        let before = format!("{game:?}");
        game.activate_internal_card("source", definition(number, 2), PLAYER)
            .expect_err("the printed once-per-turn limit still applies");
        assert!(
            format!("{game:?}") == before,
            "an illegal activation preserves the entire authority state"
        );
    }
}

fn gamma_position(number: &str, balance: u8) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"pool","card":"counter-cost-blade","state":{"counters":{"ギガカウンター":balance}}},
        {"id":"poor","card":"counter-cost-blade","state":{"counters":{"ギガカウンター":9_u8}}},
        {"id":"ally","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"drawn","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex","card":"counter-cost-blade","state":{"counters":{"ギガカウンター":99_u8}}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8,"damage":0_u8}},
        {"id":"opponent","card":"counter-cost-blade","state":{"counters":{"ギガカウンター":99_u8}}}
    ]);
    initial
}

#[test]
fn gamma_and_printing_pay_ten_from_one_gigablade_then_damage_and_gain_stats_and_storm() {
    for number in ["BP18-022", "BP18-SL07"] {
        let mut game = prepared(&gamma_position(number, 12));
        let played = game.play_internal_card("source", PLAYER).unwrap();
        finish(&mut game, played);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["drawn"])
        );
        let play = game
            .activate_internal_card("source", definition(number, 2), PLAYER)
            .unwrap();
        finish(&mut game, play);
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["pool"]["counters"]["ギガカウンター"], 2_u8);
        assert_eq!(view["objects"]["poor"]["counters"]["ギガカウンター"], 9_u8);
        assert_eq!(view["objects"]["enemy"]["damage"], 5_u8);
        assert_eq!(view["objects"]["ally"]["damage"], 0_u8);
        assert_eq!(view["objects"]["source"]["power"], 5_u8);
        assert_eq!(view["objects"]["source"]["hp"], 5_u8);
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .iter()
                .any(|keyword| keyword == "疾走"),
            "the printed effect grants Storm"
        );
        assert_eq!(view["P1"]["pp"]["current"], 7_u8);
        assert_eq!(view["P2"]["leader"]["life"], 20_u8);
        for id in ["ex", "opponent"] {
            assert_eq!(view["objects"][id]["counters"]["ギガカウンター"], 99_u8);
        }
    }
}

#[test]
fn gamma_cannot_sum_insufficient_gigablades_or_use_an_ex_or_opposing_pool() {
    let number = "BP18-022";
    let mut game = prepared(&gamma_position(number, 9));
    let played = game.play_internal_card("source", PLAYER).unwrap();
    finish(&mut game, played);
    trigger(&mut game);
    let before = format!("{game:?}");
    assert!(
        matches!(
            game.activate_internal_card("source", definition(number, 2), PLAYER),
            Err(EngineFailure::IllegalPlay(_))
        ),
        "one own field holder must supply all ten counters"
    );
    assert!(
        format!("{game:?}") == before,
        "an illegal activation preserves the entire authority state"
    );
}
