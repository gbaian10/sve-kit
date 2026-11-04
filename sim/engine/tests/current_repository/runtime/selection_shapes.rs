#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the real selection scenarios."
)]
use super::*;
use std::{fs::read_to_string, path::PathBuf};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{DamageShare, DecisionView};

const CONTROLS: [(&str, u8, &[&str]); 9] = [
    ("plain", 0, &[]),
    ("two", 2, &[]),
    ("four", 4, &[]),
    ("passion-two", 2, &["パッション"]),
    ("passion-three", 3, &["パッション"]),
    ("passion-twelve", 12, &["パッション"]),
    ("passion-thirteen", 13, &["パッション"]),
    ("priconne", 1, &["プリコネ"]),
    ("devil", 1, &["魔界"]),
];

fn prepared() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot,keywords,mut input) = inputs().clone();
        for (number,cost,traits) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":number,"card_class":"ニュートラル","card_type":"フォロワー",
                "traits":traits,"cost":cost.to_string(),"power":"2","hp":"30","text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml");
        let mut rules:Value = serde_saphyr::from_str(&read_to_string(path).unwrap()).unwrap();
        rules["input"] = rule_fixtures::rules(&snapshot,json!([]),json!([]))["input"].clone();
        repository::select_with_rules(&snapshot,&keywords,&input,&rules.to_string(),&EngineIdentityInput::LegacyJp).unwrap()
    })
}

pub(super) fn fixture(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        prepared().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "selection-shapes",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(number, &empty).unwrap();
    }
    game
}

pub(super) fn awaiting(step: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = step else {
        panic!("printed choice")
    };
    question
}

pub(super) fn trigger(game: &mut Game, number: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    assert_eq!(
        options[0].card, number,
        "trigger belongs to the printed card"
    );
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

pub(super) fn enter(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the printed follower enters before its Fanfare resolves"
    );
}

pub(super) fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial
}

fn own_field() -> Selector {
    let mut objects = subjects();
    if let Selector::Zone { players, .. } = &mut objects {
        players.relation = PlayerRelation::SelfPlayer;
        players.cardinality = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
    }
    objects
}

#[test]
fn bp04_005_returns_zero_many_or_itself_and_counts_the_hand_after_movement() {
    for selected in [0_usize, 2, 3] {
        let mut initial = position("BP04-005");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"spare-one","card":"plain"}),
                json!({"id":"spare-two","card":"plain"}),
            ]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"ally-one","card":"two"},{"id":"ally-two","card":"two"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"two"}]);
        let mut game = fixture(&initial);
        enter(&mut game);
        let choice = awaiting(trigger(&mut game, "BP04-005"));
        let InputDomain::Targets(domain) = &choice.domain else {
            panic!("own cards")
        };
        assert_eq!(domain.members.len(), 3);
        assert_eq!(domain.cardinality.minimum, Nat(0));
        let handles = if selected == 3 {
            domain
                .members
                .iter()
                .filter(|member| member.card.as_deref() == Some("BP04-005"))
                .map(|member| member.handle)
                .collect()
        } else {
            domain
                .members
                .iter()
                .filter(|member| member.card.as_deref() == Some("two"))
                .take(selected)
                .map(|member| member.handle)
                .collect()
        };
        game.answer_internal_card(choice.actor, choice.token, Answer::Targets(vec![0, 0]))
            .unwrap_err();
        resolved(
            game.answer_internal_card(choice.actor, choice.token, Answer::Targets(handles))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        if selected == 3 {
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
            assert!(
                state["P1"]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
            assert_eq!(state["objects"]["source"]["power"], 1_u8);
        } else {
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2 + selected);
            assert_eq!(state["objects"]["source"]["power"], json!(3 + selected));
            assert_eq!(state["objects"]["source"]["hp"], json!(3 + selected));
        }
        assert_eq!(state["P2"]["field"], json!(["enemy"]));
    }
}

#[test]
fn bp12_078_distributes_six_among_any_selected_followers_including_zero() {
    for count in [0_usize, 2] {
        let mut initial = position("BP12-078");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..6)
                .map(|index| json!({"id":format!("material-{index}"),"card":"plain"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-one","card":"two"},{"id":"enemy-two","card":"two"}
        ]);
        let mut game = fixture(&initial);
        enter(&mut game);
        let mut step = trigger(&mut game, "BP12-078");
        let mut allocated = false;
        while let CardPlay::Awaiting(question) = step {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => {
                    assert_eq!(domain.members.len(), 2);
                    Answer::Targets(
                        domain
                            .members
                            .iter()
                            .take(count)
                            .map(|member| member.handle)
                            .collect(),
                    )
                }
                InputDomain::CostMaterials(domain) => {
                    assert_eq!(domain.members.len(), 6);
                    Answer::CostMaterials(
                        domain.members.iter().map(|member| member.handle).collect(),
                    )
                }
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::Distribution(domain) => {
                    assert_eq!(count, 2);
                    assert_eq!(domain.total, Nat(6));
                    assert_eq!(domain.members.len(), 2);
                    allocated = true;
                    Answer::Distribution(
                        domain
                            .members
                            .iter()
                            .map(|member| DamageShare {
                                handle: member.handle,
                                amount: Nat(3),
                            })
                            .collect(),
                    )
                }
                unexpected @ (InputDomain::Abilities(_)
                | InputDomain::ResourceEntryPayment { .. }
                | InputDomain::WardEntry { .. }
                | InputDomain::Quick { .. }
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::Order(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::SearchReplacement
                | InputDomain::TokenCapacity { .. }
                | InputDomain::Modes { .. }) => panic!(
                    "unexpected decision instead of target, cost or allocation: {unexpected:?}"
                ),
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(step);
        assert_eq!(allocated, count > 0);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 6);
        for id in ["enemy-one", "enemy-two"] {
            assert_eq!(
                state["objects"][id]["damage"],
                if count == 0 { 0_u8 } else { 3 }
            );
        }
    }
}

#[test]
fn bp08_094_opponent_chooses_its_own_sacrifice_for_either_controller() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"source","card":"BP08-093"}]);
        initial["players"][seat]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-094"}]);
        initial["players"][other]["zones"]["field"] = json!([
            {"id":"enemy-one","card":"two"},{"id":"enemy-two","card":"four"}
        ]);
        initial["players"][other]["zones"]["ex"] = json!([{"id":"off-zone","card":"plain"}]);
        let mut game = fixture(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        let choice = awaiting(trigger(&mut game, "BP08-094"));
        assert_eq!(choice.actor.seat, seat_number(other));
        assert!(
            game.internal_decision(if seat == "P1" { View::P1 } else { View::P2 })
                .is_none()
        );
        let InputDomain::Targets(domain) = &choice.domain else {
            panic!("opponent sacrifice")
        };
        assert_eq!(domain.members.len(), 2);
        game.answer_internal_card(
            PlayerId {
                seat: seat_number(seat),
            },
            choice.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap_err();
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[other]["cemetery"], json!(["enemy-one"]));
        assert_eq!(state[other]["field"], json!(["enemy-two"]));
        assert_eq!(state[other]["ex"], json!(["off-zone"]));
        assert_eq!(state[seat]["field"], json!(["source"]));
    }
}

#[test]
fn cp04_109_and_all_printings_pay_two_priconne_then_opponent_privately_discards_two() {
    for number in ["CP04-109", "CP04-SL25", "PCS01-047"] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"payment-one","card":"priconne"}),
                json!({"id":"payment-two","card":"priconne"}),
                json!({"id":"wrong","card":"plain"}),
            ]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw-one","card":"plain"},{"id":"draw-two","card":"plain"},{"id":"tail","card":"plain"}
        ]);
        initial["players"]["P2"]["zones"]["hand"] = json!([
            {"id":"secret-one","card":"two"},{"id":"secret-two","card":"four"},{"id":"secret-three","card":"plain"}
        ]);
        let mut game = fixture(&initial);
        enter(&mut game);
        let mut step = trigger(&mut game, number);
        let mut discarded = false;
        while let CardPlay::Awaiting(question) = step {
            let answer = match &question.domain {
                InputDomain::OptionalCost => Answer::OptionalCost(true),
                InputDomain::CostMaterials(domain) => {
                    assert_eq!(question.actor, PlayerId { seat: 1 });
                    assert_eq!(domain.members.len(), 2);
                    Answer::CostMaterials(
                        domain.members.iter().map(|member| member.handle).collect(),
                    )
                }
                InputDomain::Targets(domain) => {
                    assert_eq!(question.actor, PlayerId { seat: 2 });
                    assert_eq!(domain.members.len(), 3);
                    assert!(game.internal_decision(View::P1).is_none());
                    assert!(
                        !game
                            .projection(View::P1)
                            .unwrap()
                            .to_string()
                            .contains("secret-one")
                    );
                    game.answer_internal_card(
                        PlayerId { seat: 1 },
                        question.token,
                        Answer::Targets(vec![0, 1]),
                    )
                    .unwrap_err();
                    discarded = true;
                    Answer::Targets(
                        domain
                            .members
                            .iter()
                            .take(2)
                            .map(|member| member.handle)
                            .collect(),
                    )
                }
                unexpected @ (InputDomain::Abilities(_)
                | InputDomain::ResourceEntryPayment { .. }
                | InputDomain::WardEntry { .. }
                | InputDomain::Quick { .. }
                | InputDomain::BeforeDraw
                | InputDomain::Pending(_)
                | InputDomain::TriggerQuota(_)
                | InputDomain::Integer(_)
                | InputDomain::Order(_)
                | InputDomain::DamageReplacements(_)
                | InputDomain::DriveTrigger { .. }
                | InputDomain::Search(_)
                | InputDomain::SearchPartition(_)
                | InputDomain::Distribution(_)
                | InputDomain::SearchReplacement
                | InputDomain::TokenCapacity { .. }
                | InputDomain::Modes { .. }) => panic!(
                    "unexpected decision instead of payment or private discard: {unexpected:?}"
                ),
            };
            step = game
                .answer_internal_card(question.actor, question.token, answer)
                .unwrap();
        }
        resolved(step);
        assert!(discarded);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            json!(["wrong", "draw-one", "draw-two"])
        );
        assert_eq!(state["P2"]["hand"], json!(["secret-three"]));
        assert_eq!(state["P2"]["cemetery"], json!(["secret-one", "secret-two"]));
    }
}

#[test]
fn bp04_070_destroys_all_tied_lowest_effective_cost_opposing_followers() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP04-069"},{"id":"own-zero","card":"plain"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP04-070"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"low-one","card":"two"},{"id":"low-two","card":"four","state":{"cost":2_u8}},
        {"id":"high","card":"four"},
        {"id":"opposing-base","card":"BP04-069","state":{"evolved":true,"evolved_with":"opposing-form"}}
    ]);
    initial["players"]["P2"]["zones"]["evolution"] =
        json!([{"id":"opposing-form","card":"BP04-070"}]);
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"off-zone-zero","card":"plain"}]);
    let mut game = fixture(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    resolved(trigger(&mut game, "BP04-070"));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["high", "opposing-base"]));
    assert_eq!(state["P2"]["cemetery"], json!(["low-one", "low-two"]));
    assert_eq!(state["P2"]["ex"], json!(["off-zone-zero"]));
    assert_eq!(state["P1"]["field"], json!(["source", "own-zero"]));
}

pub(super) fn passion_sum(numbers: &[&str]) {
    for number in numbers {
        for (extra, damage, leader) in [
            (None, 4_u8, 20_u8),
            (Some("passion-two"), 4, 20),
            (Some("passion-three"), 8, 20),
            (Some("passion-twelve"), 8, 20),
            (Some("passion-thirteen"), 8, 16),
        ] {
            let mut initial = position(number);
            let mut field = vec![json!({"id":"wrong-trait","card":"four","state":{"cost":99_u8}})];
            if let Some(extra) = extra {
                field.push(json!({"id":"passion","card":extra,"state":{"cost":0_u8}}));
            }
            initial["players"]["P1"]["zones"]["field"] = json!(field);
            initial["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"off-zone","card":"passion-thirteen"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([
                {"id":"enemy-one","card":"passion-thirteen"},{"id":"enemy-two","card":"two"}
            ]);
            let mut game = fixture(&initial);
            enter(&mut game);
            resolved(trigger(&mut game, number));
            let state = game.projection(View::Referee).unwrap();
            for id in ["enemy-one", "enemy-two"] {
                assert_eq!(
                    state["objects"][id]["damage"], damage,
                    "{number} uses the original Passion cost threshold"
                );
            }
            assert_eq!(
                state["P2"]["leader"]["life"], leader,
                "{number} independently checks the twenty-cost leader damage"
            );
            assert_eq!(
                state["objects"]["wrong-trait"]["damage"], 0_u8,
                "own non-Passion follower is not damaged"
            );
        }
    }
}

#[test]
fn ecp02_005_and_sl05_sum_original_passion_cost_at_ten_and_twenty() {
    passion_sum(&["ECP02-005", "ECP02-SL05"]);
}
#[test]
fn bp22_088_and_p49_buff_only_after_cemetery_entry_and_opponents_sacrifice_their_own_aura_card() {
    for number in ["BP22-088", "BP22-P49"] {
        for seat in ["P1", "P2"] {
            let other = if seat == "P1" { "P2" } else { "P1" };
            for cemetery in [false, true] {
                let mut initial = draw_position();
                initial["turn"]["active"] = json!(seat);
                initial["players"][seat]["zones"]["field"] = json!([]);
                initial["players"][seat]["zones"][if cemetery { "cemetery" } else { "hand" }] =
                    json!([{"id":"source","card":number}]);
                let mut game = if cemetery {
                    let movement = Action::Move {
                        objects: subjects_in(ZoneKind::Cemetery),
                        movement: MoveSpec {
                            to: MoveDestination::Shared {
                                zone: ZoneRef {
                                    player: PlayerRef::Controller {},
                                    kind: ZoneKind::Field,
                                },
                            },
                            initial_posture: None,
                            entry_effects: vec![],
                            position: None,
                            order: None,
                        },
                        reason: MoveReason::Put,
                    };
                    let mut game = with_producer(&initial, movement);
                    activate(&mut game, seat);
                    game.resume_internal_card().unwrap();
                    game
                } else {
                    let mut game = game(&initial);
                    assert!(matches!(
                        game.play_internal_card(
                            "source",
                            PlayerId {
                                seat: seat_number(seat)
                            }
                        )
                        .unwrap(),
                        CardPlay::Entered(_)
                    ));
                    game
                };
                resolved(trigger(&mut game, number));
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
                    if cemetery { 3_u8 } else { 2 }
                );
            }
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] = json!([{"id":"source","card":number}]);
            initial["players"][other]["zones"]["field"] = json!([
                {"id":"aura-one","card":"BP01-138"}, {"id":"aura-two","card":"BP01-138"}
            ]);
            let mut game = with_producer(
                &initial,
                Action::Destroy {
                    objects: own_field(),
                },
            );
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            let CardPlay::Awaiting(choice) = trigger(&mut game, number) else {
                panic!("opponent sacrifice")
            };
            assert_eq!(choice.actor.seat, seat_number(other));
            let InputDomain::Targets(domain) = &choice.domain else {
                panic!("public self-selected target")
            };
            assert_eq!(
                domain.members.len(),
                2,
                "Aura does not stop its controller from choosing its own card"
            );
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[other]["field"].as_array().unwrap().len(), 1);
            assert_eq!(state[other]["cemetery"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn bp09_077_opponent_selects_one_maximum_effective_attack_including_own_aura_ties() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        for raised in [false, true] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] = json!([
                {"id":"source","card":"BP09-076"},
                {"id":"own-aura","card":"BP01-138","state":{"power":10_u8}}
            ]);
            initial["players"][seat]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP09-077"}]);
            initial["players"][other]["zones"]["field"] = json!([
                {"id":"aura-one","card":"BP01-138"},
                {"id":"aura-two","card":"BP01-138"},
                {"id":"low","card":"two","state":{"power":if raised {6_u8} else {2}}}
            ]);
            let mut game = fixture(&initial);
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            let choice = awaiting(trigger(&mut game, "BP09-077"));
            assert_eq!(choice.actor.seat, seat_number(other));
            let InputDomain::Targets(domain) = &choice.domain else {
                panic!("maximum follower selection")
            };
            assert_eq!(domain.members.len(), if raised { 1 } else { 2 });
            assert!(
                domain.members.iter().all(|member| member.card.as_deref()
                    == Some(if raised { "two" } else { "BP01-138" }))
            );
            let before = game.projection(View::Referee).unwrap();
            game.answer_internal_card(
                PlayerId {
                    seat: seat_number(seat),
                },
                choice.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            resolved(
                game.answer_internal_card(
                    choice.actor,
                    choice.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[other]["cemetery"].as_array().unwrap().len(), 1);
            assert_eq!(state[other]["field"].as_array().unwrap().len(), 2);
            assert_eq!(state[seat]["field"], json!(["source", "own-aura"]));
            if raised {
                assert_eq!(state[other]["cemetery"], json!(["low"]));
            } else {
                assert!(
                    state[other]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("low"))
                );
            }
        }
    }
}
