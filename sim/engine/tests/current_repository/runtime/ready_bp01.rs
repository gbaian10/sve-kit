#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn position(number: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"drawn","card":"BP01-037"},{"id":"remaining","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8}}
    ]);
    initial
}

fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-bp01",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => {
                assert_eq!(domain.members.len(), 1, "only the intended target matches");
                Answer::Targets(vec![domain.members[0].handle])
            }
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            InputDomain::Search(domain) => {
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Modes { modes, .. } => Answer::Modes(vec![modes[0]]),
            _ => panic!("unexpected printed decision: {:?}", question.domain),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish(game, play));
    game.resume_internal_card().unwrap();
}

fn play(game: &mut Game) {
    let play = game.play_internal_card("source", PLAYER).unwrap();
    let result = finish(game, play);
    assert!(
        matches!(result, CardPlay::Entered(_) | CardPlay::Resolved(_)),
        "the printed card play completes"
    );
    if matches!(game.internal_card_stage(), Some(ResumeStage::CheckTiming)) {
        game.resume_internal_card().unwrap();
    }
}

fn activate_card(game: &mut Game, number: &str, line: u32) {
    let id = selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.source.line == line)
        .unwrap()
        .node
        .id;
    let play = game.activate_internal_card("source", id, PLAYER).unwrap();
    resolved(finish(game, play));
    game.resume_internal_card().unwrap();
}

fn evolve(base: &str, form: &str, pp: u8, hand: Value) -> Game {
    let mut initial = position(base, "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial["players"]["P1"]["zones"]["hand"] = hand;
    let mut game = new_game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the matching form evolves for the declared cost"
    );
    game
}

#[test]
fn rhinoceroach_declares_storm_or_its_power_as_damage_and_rejects_a_targetless_mode() {
    for (enemy, mode) in [(true, 1_u64), (true, 2), (false, 1)] {
        let mut initial = position("BP01-004", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-005"}]);
        if !enemy {
            initial["players"]["P2"]["zones"]["field"] = json!([]);
        }
        let mut game = new_game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":0_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution trigger")
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("mode declaration")
        };
        let InputDomain::Modes { modes, .. } = &question.domain else {
            panic!("printed modes")
        };
        if !enemy {
            assert_eq!(modes, &vec![ModeId(1)]);
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap_err();
        }
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(mode)]),
            )
            .unwrap();
        resolved(finish(&mut game, step));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走")),
            mode == 1
        );
        if enemy {
            assert_eq!(state["objects"]["enemy"]["damage"], u8::from(mode == 2));
        }
    }
}

#[test]
fn fairy_beast_banishes_a_fairy_in_ex_to_heal_and_draw_only_once() {
    let mut initial = position("BP01-013", "field");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"fairy","card":"BP01-T03"},{"id":"spare","card":"BP01-T03"}]);
    let mut game = new_game(&initial);
    activate_card(&mut game, "BP01-013", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 23_u8);
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["ex"], json!(["spare"]));
    game.activate_internal_card(
        "source",
        selection().current["BP01-013"].program.card().abilities[0]
            .node
            .id,
        PLAYER,
    )
    .unwrap_err();
}

#[test]
fn fairy_beast_cannot_pay_with_a_non_fairy_ex_card() {
    let mut initial = position("BP01-013", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"wrong","card":"BP01-171"}]);
    let mut game = new_game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let id = selection().current["BP01-013"].program.card().abilities[0]
        .node
        .id;
    game.activate_internal_card("source", id, PLAYER)
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn spectral_wizard_evolution_discards_a_spell_before_four_damage() {
    for form in ["BP01-060", "BP01-P13"] {
        let mut game = evolve("BP01-059", form, 2, json!([{"id":"fee","card":"BP01-168"}]));
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["fee"]));
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
}

#[test]
fn dragon_mage_counts_a_played_spell_then_spends_three_spell_counters() {
    let mut initial = position("BP01-062", "field");
    initial["players"]["P1"]["zones"]["field"][0]["state"] =
        json!({"counters":{"スペルカウンター":2_u8}});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP01-023"}]);
    let mut game = new_game(&initial);
    let step = game.play_internal_card("spell", PLAYER).unwrap();
    resolved(finish(&mut game, step));
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["スペルカウンター"],
        3_u8
    );
    activate_card(&mut game, "BP01-062", 2);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(
        state["objects"]["source"]["counters"]["スペルカウンター"],
        0_u8
    );
}

#[test]
fn evolved_cerberus_adds_mimi_and_coco_to_ex() {
    let mut initial = position("ready-cerberus", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP01-102"}]);
    let mut game = extra_game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let names: Vec<_> = state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
        })
        .collect();
    assert_eq!(names, vec!["冥焔の右腕・ミミ", "冥焔の左腕・ココ"]);
}

#[test]
fn medusa_stacks_each_independent_discount_then_acts_to_destroy() {
    for crimson in [false, true] {
        for necro in [false, true] {
            let mut initial = position("BP01-104", "hand");
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["field"] = json!([]);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..if necro { 10_u8 } else { 0 })
                    .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
            let mut game = with_producer(
                &initial,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(if crimson { -1 } else { 0 })),
                    },
                },
            );
            activate(&mut game, "P1");
            game.resume_internal_card().unwrap();
            play(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                4_u8 + u8::from(crimson) + u8::from(necro)
            );
            activate_card(&mut game, "BP01-104", 4);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["field"], json!([]));
            assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
            assert_eq!(state["objects"]["source"]["acted"], true);
        }
    }
}

// Only supporting cards are synthetic; each tested card keeps its accepted program.
fn extra_fixture() -> &'static (SelectedEffects, WellFormedCardProgram) {
    static READY: OnceLock<(SelectedEffects, WellFormedCardProgram)> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        for (number, name, kind, cost) in [
            ("ready-glass", "ツインプリズナー・グラス", "フォロワー", "3"),
            ("ready-combined", "フラム＝グラス", "フォロワー", "3"),
            ("ready-cerberus", "ケルベロス", "フォロワー", "3"),
            ("ready-expensive-amulet", "expensive amulet", "アミュレット", "6"),
        ] {
            snapshot.push_str(&json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":if kind=="アミュレット" {"-"} else {"3"},"hp":if kind=="アミュレット" {"-"} else {"3"},"traits":[],"text":null,"sections":[]}]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([format!("{number}.yaml"), json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()]));
        }
        let selected = repository::selection_with_rules_from(&snapshot, &keywords, &input);
        let mut tree = expanded::tree().clone();
        tree["cards"] = json!({"ready-cerberus":tree["cards"]["synthetic-evolution"]});
        let base = lowerer().lower(&tree).unwrap().remove("ready-cerberus").unwrap();
        (selected, base)
    })
}
fn extra_game(initial: &Value) -> Game {
    let (selected, base) = extra_fixture();
    let mut game = Game::from_selected_effects(
        selected.clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "ready-support",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for number in [
        "ready-glass",
        "ready-combined",
        "ready-cerberus",
        "ready-expensive-amulet",
    ] {
        game.load_internal_card(
            number,
            if number == "ready-cerberus" {
                base
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game
}

#[test]
fn flam_sacrifices_itself_and_glass_to_search_the_combined_follower() {
    let mut initial = position("BP01-166", "field");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"glass","card":"ready-glass"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"combined","card":"ready-combined"}]);
    let mut game = extra_game(&initial);
    activate_card(&mut game, "BP01-166", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source", "glass"]));
    assert_eq!(state["P1"]["field"], json!(["combined"]));
}

fn amulet_return(number: &str, fanfare: bool) {
    let mut initial = position(number, "hand");
    let count = if fanfare { 1_u8 } else { 3 };
    let mut cemetery: Vec<_> = (0..count)
        .map(|i| json!({"id":format!("amulet{i}"),"card":"BP01-141"}))
        .collect();
    cemetery.push(json!({"id":"expensive","card":"ready-expensive-amulet"}));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    let mut game = extra_game(&initial);
    if fanfare {
        play(&mut game);
        trigger(&mut game);
    } else {
        let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap()
        else {
            panic!("formal cemetery targets")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal amulet targets")
        };
        assert_eq!(domain.members.len(), 3, "the six-cost amulet is excluded");
        assert_eq!(
            domain.cardinality.maximum,
            Some(Nat(3)),
            "the printed selection allows at most three amulets"
        );
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
            )
            .unwrap();
        resolved(finish(&mut game, step));
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    for i in 0..count {
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("amulet{i}"))),
            "every chosen amulet enters the controller field"
        );
    }
    assert_eq!(
        state["P1"]["cemetery"],
        if fanfare {
            json!(["expensive"])
        } else {
            json!(["expensive", "source"])
        },
        "the over-cost card stays in the cemetery and the resolved spell joins it"
    );
}
#[test]
fn prayer_reanimates_three_amulets_and_excludes_six_cost() {
    amulet_return("BP01-132", false);
}
#[test]
fn cruel_priestess_reanimates_a_five_cost_amulet_and_excludes_six_cost() {
    amulet_return("BP01-142", true);
}
