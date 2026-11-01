#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the subject and capture scenarios."
)]
use super::event_cemetery::{finish_checks, move_named};
use super::*;
use alloc::collections::BTreeMap;

const CONTROLS: [(&str, &str, &str, u8, u8); 10] = [
    ("subject-soldier", "subject soldier", "兵士", 2, 4),
    ("subject-shikigami", "subject shikigami", "式神", 2, 4),
    ("subject-dragon", "subject dragon", "武闘竜人", 2, 4),
    ("subject-low", "subject low", "武闘竜人", 2, 3),
    ("capture-low", "capture low", "反例", 3, 4),
    ("capture-high", "capture high", "反例", 5, 4),
    ("capture-base", "鋼鉄の魔獣・レガロア", "機械", 8, 4),
    ("subject-draw", "subject draw", "反例", 1, 1),
    ("history-base", "衝動の人狼・リベルテ", "獣", 3, 2),
    ("combat-base", "スットゥング", "巨人", 5, 4),
];

#[test]
fn event_subject_is_rejected_without_an_object_event() {
    for activated in [false, true] {
        let mut tree = expanded::tree().clone();
        let mut observer = tree["cards"]["BP01-041"].clone();
        let mut ability = observer["abilities"][0].clone();
        let steps = json!([{"buff":{"on":"this","power":1_u8}}]);
        let head = if activated {
            ability["activate"] = json!({"cost":{"pp":1_u8},"do":steps});
            "activate"
        } else {
            ability.as_object_mut().unwrap().remove("activate");
            ability["auto"] = json!({"on":{"phase":{"whose":"self","which":"end"}},"do":steps});
            "auto"
        };
        observer["abilities"] = json!([ability]);
        tree["cards"] = json!({"subject-boundary":observer});
        lowerer().lower(&tree).unwrap();
        tree["cards"]["subject-boundary"]["abilities"][0][head]["do"][0]["buff"]["on"] =
            json!("it");
        assert_eq!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

fn selected() -> &'static SelectedEffects {
    static READY: OnceLock<SelectedEffects> = OnceLock::new();
    READY.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = producer_inputs();
        for (number, name, tribe, cost, power) in CONTROLS {
            snapshot.push_str(&json!({"number":number,"faces":[{
                "name":name,"card_class":match number { "capture-base" => "ウィッチ", "history-base" => "ナイトメア", _ => "ニュートラル" },
                "card_type":"フォロワー","traits":[tribe],"cost":cost.to_string(),
                "power":power.to_string(),"hp":"6","text":null,"sections":[]
            }]}).to_string());
            snapshot.push('\n');
            input["documents"].as_array_mut().unwrap().push(json!([
                format!("{number}.yaml"),
                json!({"format":1_u8,"kind":"effect_set","cards":{number:{"status":"complete","abilities":[]}}}).to_string()
            ]));
        }
        repository::selection_with_rules_from(&snapshot, &keywords, &input)
    })
}

fn base_program(requested: &str) -> &'static WellFormedCardProgram {
    static BASE: OnceLock<BTreeMap<String, WellFormedCardProgram>> = OnceLock::new();
    &BASE.get_or_init(|| {
        let mut programs = BTreeMap::new();
        for number in ["capture-base", "history-base", "combat-base"] {
            let mut tree = expanded::tree().clone();
            let mut base = tree["cards"]["BP01-041"].clone();
            let mut ability = base["abilities"][0].clone();
            ability.as_object_mut().unwrap().remove("activate");
            ability["evolve"] = json!({"cost":{"pp":1_u8}});
            base["abilities"] = json!([ability]);
            tree["cards"] = json!({number:base});
            programs.extend(lowerer().lower(&tree).unwrap());
        }
        programs
    })[requested]
}

pub(super) fn production(mut actions: Vec<Action>) -> WellFormedCardProgram {
    for (index, action) in actions.iter_mut().enumerate() {
        let (Action::Move { objects, .. } | Action::Destroy { objects }) = action else {
            panic!("named movement fixture");
        };
        let Selector::Zone {
            binding, predicate, ..
        } = objects
        else {
            panic!("named movement selector");
        };
        *binding = Binding::new(
            50_u64.checked_add(u64::try_from(index).unwrap()).unwrap(),
            ScopeId(1),
        );
        let Expr::Compute(operation) = predicate.as_mut() else {
            panic!("named movement filter");
        };
        let Predicate::NamesIntersect { left, .. } = operation.as_mut() else {
            panic!("named movement subject");
        };
        *left = Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding: *binding }),
            },
        }));
    }
    producer_steps(actions)
}

pub(super) fn prepared(initial: &Value, actions: Vec<Action>) -> Game {
    prepared_program(initial, &production(actions))
}

pub(super) fn prepared_program(initial: &Value, program: &WellFormedCardProgram) -> Game {
    let mut initial = initial.clone();
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"producer","card":"producer"}));
    let mut game = Game::from_selected_effects(
        selected().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "event-subject-capture",
    )
    .unwrap();
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    for (number, ..) in CONTROLS {
        game.load_internal_card(
            number,
            if matches!(number, "capture-base" | "history-base" | "combat-base") {
                base_program(number)
            } else {
                &empty
            },
        )
        .unwrap();
    }
    game.load_internal_card("producer", program).unwrap();
    game
}

pub(super) fn position(number: &str, hand: bool) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][if hand { "hand" } else { "field" }] =
        json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"subject-draw"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"capture-low","state":{"acted":true}}]);
    initial
}

pub(super) fn enter(from: ZoneKind, name: &str) -> Action {
    let mut action = move_named(from, OwnerZone::Hand, name);
    let Action::Move { movement, .. } = &mut action else {
        panic!("named field entry");
    };
    movement.to = MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind: ZoneKind::Field,
        },
    };
    action
}

pub(super) fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("one printed target or Ward posture");
            };
            assert_eq!(domain.members.len(), 1, "the printed target is unambiguous");
            Answer::Targets(vec![domain.members[0].handle])
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed ability resolves"
    );
    finish_checks(game);
}

pub(super) fn resolve(game: &mut Game, number: &str, count: usize, origin: Option<&str>) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("one committed event subject");
    };
    assert_eq!(
        options.len(),
        count,
        "only actual entry or attack facts subscribe"
    );
    let chosen = options
        .iter()
        .find(|option| {
            option.card == number && origin.is_none_or(|origin| option.event.contains(origin))
        })
        .unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(chosen.handle),
        )
        .unwrap();
    finish(game, play);
}

#[test]
fn mars_printings_modify_only_the_matching_own_entering_soldier() {
    for number in ["BP04-020", "BP04-SL04"] {
        for (control, side, expected) in [
            ("subject-soldier", "P1", true),
            ("subject-shikigami", "P1", false),
            ("subject-soldier", "P2", false),
        ] {
            let mut initial = position(number, false);
            initial["turn"]["active"] = json!(side);
            initial["players"][side]["zones"]["hand"] = json!([{"id":"material","card":control}]);
            let mut game = prepared(&initial, vec![]);
            let before =
                game.projection(View::Referee).unwrap()["objects"]["source"]["power"].clone();
            let play = game
                .play_internal_card(
                    "material",
                    PlayerId {
                        seat: if side == "P1" { 1 } else { 2 },
                    },
                )
                .unwrap();
            finish(&mut game, play);
            assert_eq!(game.internal_pending_choice().unwrap().is_some(), expected);
            if expected {
                resolve(&mut game, number, 1, None);
            }
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state[side]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("material"))
            );
            assert_eq!(
                state["objects"]["material"]["power"],
                4_u8 + u8::from(expected)
            );
            assert_eq!(state["objects"]["source"]["power"], before);
        }
    }
}

#[test]
fn shikigami_printings_grant_rush_to_the_exact_entering_follower() {
    for number in ["BP06-044", "BP06-P11"] {
        let mut initial = position(number, false);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"material","card":"subject-shikigami"}]);
        let mut game = prepared(&initial, vec![]);
        let play = game
            .play_internal_card("material", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        resolve(&mut game, number, 1, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["material"]["power"], 5_u8);
        assert!(
            state["objects"]["material"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進"))
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"material","target":"enemy"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
    }
}

#[test]
fn an_earlier_entry_does_not_follow_its_subject_through_cemetery_to_a_new_generation() {
    let mut initial = position("BP06-044", false);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"material","card":"subject-shikigami"}]);
    let mut game = prepared(
        &initial,
        vec![
            enter(ZoneKind::Ex, "subject shikigami"),
            move_named(ZoneKind::Field, OwnerZone::Cemetery, "subject shikigami"),
            enter(ZoneKind::Cemetery, "subject shikigami"),
        ],
    );
    activate(&mut game, "P1");
    finish_checks(&mut game);
    resolve(&mut game, "BP06-044", 2, Some("Ex"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["material"]["power"],
        4_u8
    );
    resolve(&mut game, "BP06-044", 1, Some("Cemetery"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["material"]["power"],
        5_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn dragon_commander_printings_modify_the_frozen_attacker_after_the_commit_filter() {
    for number in ["BP18-066", "BP18-P26"] {
        for (control, power, expected) in [
            ("subject-dragon", 4_u8, true),
            ("subject-low", 3, false),
            ("subject-soldier", 4, false),
        ] {
            let mut initial = position(number, false);
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"material","card":control,"state":{"entered_this_turn":false}}));
            let mut game = prepared(&initial, vec![]);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"material","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            assert_eq!(game.internal_pending_choice().unwrap().is_some(), expected);
            if expected {
                resolve(&mut game, number, 1, None);
            }
            let question = game.internal_quick_choice().unwrap().unwrap();
            let play = game
                .answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap();
            finish(&mut game, play);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                20_u8 - power - u8::from(expected)
            );
        }
    }
}

#[test]
fn dragon_hunt_printings_capture_original_cost_before_the_selected_card_is_destroyed() {
    for number in ["BP21-072", "BP21-P39"] {
        for (enemy, effective_cost, expected) in
            [("capture-low", 9_u8, 0_usize), ("capture-high", 0, 1)]
        {
            let mut initial = position(number, true);
            initial["players"]["P2"]["zones"]["field"][0] =
                json!({"id":"enemy","card":enemy,"state":{"cost":effective_cost}});
            let mut game = prepared(&initial, vec![]);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy"))
            );
            assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), expected);
        }
    }
}

#[test]
fn regalore_uses_the_original_selected_cost_after_bounce_to_modify_its_created_machine() {
    for (enemy, effective_cost, original) in [("capture-low", 9_u8, 3_u8), ("capture-high", 0, 5)] {
        let mut initial = position("capture-base", false);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP12-039"}]);
        initial["players"]["P2"]["zones"]["field"][0] =
            json!({"id":"enemy","card":enemy,"state":{"cost":effective_cost}});
        let mut game = prepared(&initial, vec![]);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        resolve(&mut game, "BP12-039", 1, None);
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state["P2"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("enemy"))
        );
        let (_, machine) = state["objects"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(id, _)| {
                id.as_str() != "source"
                    && id.as_str() != "producer"
                    && state["P1"]["field"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(id))
            })
            .unwrap();
        assert_eq!(machine["power"], 1_u8 + original);
        assert_eq!(machine["hp"], 1_u8 + original);
    }
}
