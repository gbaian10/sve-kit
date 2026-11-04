use super::triggers::lastword;
use super::*;
use sve_engine::game::internal::InputDomain;

fn source_tree(head: &Value, number: &str) -> Value {
    let mut tree = equipment_played::tree(head, number);
    tree["cards"][number]["abilities"][0]["line"] = json!(1);
    tree
}

fn observer(field_source: bool) -> Value {
    let mut auto = json!({
        "on":{"dealt_damage":{
            "who":"self", "sel":{"trait":"妖精"},
            "recipient":{"who":"opp","zone":"field","kind":"follower"},
            "during":{"turn":"self"}
        }},
        "aggregation":"batch",
        "do":[{"damage":{"to":{"leaders":"opponents"},"n":1}}]
    });
    if field_source {
        auto["on"]["dealt_damage"]["zone"] = json!("field");
    } else {
        auto["on"]["dealt_damage"]["kind"] = json!("ability");
        auto["per_turn"] = json!(1);
    }
    auto
}

fn printed_tree(auto: &Value) -> Value {
    source_tree(&json!({"auto":auto}), "unit-follower")
}

fn accepts_printed_shape(field_source: bool) {
    let auto = observer(field_source);
    let valid = lowerer()
        .lower(&printed_tree(&auto))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "printed-damage-source",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &valid).unwrap();
    for case in 0_u8..11 {
        let mut invalid = auto.clone();
        let header = invalid["on"]["dealt_damage"].as_object_mut().unwrap();
        match case {
            0 => {
                header.insert("who".into(), json!("opp"));
            }
            1 => {
                header.remove("sel");
            }
            2 => {
                header.remove("during");
            }
            3 => {
                header.insert("during".into(), json!({"turn":"opp"}));
            }
            4 => {
                header.insert("kind".into(), json!("combat"));
            }
            5 => {
                header.insert("zone".into(), json!("hand"));
            }
            6 => {
                header["sel"]["kind"] = json!("follower");
            }
            7 => {
                header["sel"]["name"] = json!("unit-follower");
            }
            8 => {
                header["sel"]["class"] = json!("ニュートラル");
            }
            9 => {
                header.insert("ref".into(), json!("this"));
            }
            _ => {
                if field_source {
                    header.insert("kind".into(), json!("ability"));
                } else {
                    header.remove("kind");
                }
            }
        }
        lowerer().lower(&printed_tree(&invalid)).unwrap_err();
    }
    let mut per_hit = auto;
    per_hit.as_object_mut().unwrap().remove("aggregation");
    let valid = lowerer()
        .lower(&printed_tree(&per_hit))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    game.load_internal_card("unit-follower", &valid)
        .unwrap_err();
}

#[test]
fn printed_own_field_trait_damage_accepts_any_kind_only_during_own_turn() {
    accepts_printed_shape(true);
}

#[test]
fn printed_own_trait_ability_damage_accepts_off_field_sources_with_a_turn_limit() {
    accepts_printed_shape(false);
}

fn trait_catalog(has_trait: bool) -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] != "unit-amulet" && has_trait {
                row["faces"][0]["traits"] = json!(["妖精"]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

fn field_hit(id: u64, amount: i64, own: bool) -> Located<Node> {
    let candidate = Binding::new(
        id.checked_add(90).unwrap(),
        ScopeId(id.checked_add(100).unwrap()),
    );
    let mut hit = damage(
        id,
        RecipientSet::Objects {
            objects: Selector::Zone {
                players: players(if own {
                    PlayerRelation::SelfPlayer
                } else {
                    PlayerRelation::Opponents
                }),
                zone: ZoneKind::Field,
                view: SelectionView::Current {},
                binding: candidate,
                predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
                    left: Expr::Live(Box::new(KindRead::Kind {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding: candidate }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    right: Expr::Literal(CardKind::Follower),
                }))),
            },
        },
        amount,
    );
    if let Node::Instruction { instruction } = &mut hit.node
        && let Action::Damage { source, .. } = &mut instruction.action
    {
        *source = ObjectView::SourceInformation {};
    }
    hit
}

fn source_position(zone: &str, number: &str, state: &Value) -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"watch","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"f","card":"unit-follower","state":{"acted":true}},
        {"id":"g","card":"unit-follower","state":{"acted":true}}
    ]);
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"s","card":number,"state":state}));
    initial
}

fn source_game(
    initial: &Value,
    has_trait: bool,
    number: &str,
    source: &WellFormedCardProgram,
    watch: &WellFormedCardProgram,
) -> Game {
    let mut game = Game::new(
        trait_catalog(has_trait),
        initial,
        &Value::Null,
        &Value::Null,
        "damage-source-authority",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", watch).unwrap();
    game.load_internal_card(number, source).unwrap();
    if number != "unit-follower" {
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
    }
    game
}

fn printed_observer(field_source: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&source_tree(
            &json!({"auto":observer(field_source)}),
            "unit-amulet",
        ))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn source_board(
    field_source: bool,
    has_trait: bool,
    steps: Vec<Located<Node>>,
    zone: &str,
    number: &str,
    state: &Value,
) -> Game {
    let source = if number == "unit-follower" {
        card(vec![ability(1, steps)])
    } else {
        card(vec![play::spell(steps)])
    };
    source_game(
        &source_position(zone, number, state),
        has_trait,
        number,
        &source,
        &printed_observer(field_source),
    )
}

fn finish_source_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn observe_all(game: &mut Game) -> usize {
    finish_source_checks(game);
    let mut count = 0_usize;
    while let Some(question) = game.internal_pending_choice().unwrap() {
        assert!(
            matches!(&question.domain, InputDomain::Pending(_)),
            "the observer produces a pending ability question"
        );
        if let InputDomain::Pending(options) = question.domain {
            assert!(
                options.iter().all(|option| option.card == "unit-amulet"),
                "only the printed observer is pending"
            );
            assert_ne!(
                options.len(),
                0,
                "the pending question contains an occurrence"
            );
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            count = count.checked_add(1).unwrap();
            assert!(
                count <= 2,
                "only the expected damage instructions can trigger"
            );
            finish_source_checks(game);
        }
    }
    count
}

#[test]
fn trait_damage_batches_merge_hits_but_keep_separate_instructions_and_lethal_hits() {
    for (steps, expected) in [
        (vec![field_hit(1, 1, false)], 1),
        (vec![field_hit(1, 1, false), field_hit(2, 1, false)], 2),
        (vec![field_hit(1, 3, false)], 1),
        (vec![field_hit(1, 0, false)], 0),
        (vec![field_hit(1, 1, true)], 0),
    ] {
        let mut game = source_board(true, true, steps, "field", "unit-follower", &json!({}));
        activate(&mut game, 1);
        assert_eq!(observe_all(&mut game), expected);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            20 - expected
        );
    }
}

#[test]
fn departed_sources_keep_their_field_zone_and_trait_without_reading_the_hand_generation() {
    for has_trait in [true, false] {
        let departure = movement::put(
            1,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        );
        let mut game = source_board(
            true,
            has_trait,
            vec![departure, field_hit(2, 1, false)],
            "field",
            "unit-follower",
            &json!({}),
        );
        activate(&mut game, 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["s"])
        );
        assert_eq!(observe_all(&mut game), usize::from(has_trait));
    }
}

#[test]
fn off_field_trait_spells_only_match_the_ability_shape_and_its_shared_turn_limit() {
    for field_source in [true, false] {
        let mut game = source_board(
            field_source,
            true,
            vec![field_hit(1, 1, false), field_hit(2, 1, false)],
            "hand",
            "unit-spell",
            &json!({}),
        );
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        assert_eq!(observe_all(&mut game), usize::from(!field_source));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if field_source { 20 } else { 19 }
        );
    }
}

#[test]
fn combat_captures_each_sides_source_and_zero_attack_is_not_damage() {
    for (field_source, power, expected) in [(true, 2, 1), (true, 0, 0), (false, 2, 0)] {
        let mut game = source_board(
            field_source,
            true,
            vec![],
            "field",
            "unit-follower",
            &json!({"entered_this_turn":false,"power":power}),
        );
        game.internal_rule_action(&json!({"do":"attack","attacker":"s","target":"f"}))
            .unwrap();
        let quick = game.internal_quick_choice().unwrap().unwrap();
        assert!(matches!(
            game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ));
        assert_eq!(observe_all(&mut game), expected);
    }
}

fn play_pending_source(game: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_source_checks(game);
    let question = game.internal_pending_choice().unwrap().unwrap();
    let options = if let InputDomain::Pending(options) = &question.domain {
        Some(options)
    } else {
        None
    }
    .unwrap();
    let option = options
        .iter()
        .find(|option| option.card == number && option.line == line)
        .unwrap();
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn a_fanfare_pending_before_its_source_is_bounced_keeps_the_field_trait_snapshot() {
    let mut watch = source_tree(&json!({"auto":observer(true)}), "unit-amulet");
    let mut bounce = watch["cards"]["unit-amulet"]["abilities"][0].clone();
    bounce["line"] = json!(2);
    bounce["auto"] = json!({
        "on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},
        "targets":[{"bind":"arriving","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"move":{"ref":"arriving","to":"hand"}}]
    });
    watch["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(bounce);
    let watch_program = lowerer()
        .lower(&watch)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let follower = source_tree(
        &json!({"fanfare":{"do":[{"damage":{"to":{"who":"opp","zone":"field","kind":"follower"},"n":1}}]}}),
        "unit-follower",
    );
    let follower_program = lowerer()
        .lower(&follower)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut game = source_game(
        &source_position("hand", "unit-follower", &json!({})),
        true,
        "unit-follower",
        &follower_program,
        &watch_program,
    );
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let play = play_pending_source(&mut game, "unit-amulet", 2);
    let question = if let CardPlay::Awaiting(question) = play {
        Some(question)
    } else {
        None
    }
    .unwrap();
    let targets = if let InputDomain::Targets(targets) = &question.domain {
        Some(targets)
    } else {
        None
    }
    .unwrap();
    assert_eq!(targets.members.len(), 1);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![targets.members[0].handle]),
        )
        .unwrap(),
    );
    finish_source_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s"])
    );
    resolved(play_pending_source(&mut game, "unit-follower", 1));
    assert_eq!(observe_all(&mut game), 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19
    );
}

#[test]
fn a_granted_trait_is_sealed_before_departure_clears_the_current_generation() {
    let granted = numeric::modify(
        1,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        CharacteristicPatch::AddTrait {
            trait_id: TraitId("妖精".into()),
        },
        DurationEnd::NoTemporalExpiry {},
    );
    let departure = movement::put(
        2,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Hand,
    );
    let mut game = source_board(
        true,
        false,
        vec![granted, departure, field_hit(3, 1, false)],
        "field",
        "unit-follower",
        &json!({}),
    );
    activate(&mut game, 1);
    assert_eq!(observe_all(&mut game), 1);
}

#[test]
fn lastword_damage_uses_the_same_pre_departure_field_information_as_other_pending_sources() {
    for has_trait in [true, false] {
        let program = card(vec![
            ability(1, vec![field_hit(1, 3, true)]),
            lastword(ability(2, vec![field_hit(3, 1, false)])),
        ]);
        let mut game = source_game(
            &source_position("field", "unit-follower", &json!({})),
            has_trait,
            "unit-follower",
            &program,
            &printed_observer(true),
        );
        activate(&mut game, 1);
        finish_source_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["cemetery"],
            json!(["s"])
        );
        resolved(play_pending_source(&mut game, "unit-follower", 2));
        assert_eq!(observe_all(&mut game), usize::from(has_trait));
    }
}

#[test]
fn an_opponents_departed_trait_damage_source_during_our_turn_neither_errors_nor_matches() {
    let program = card(vec![
        ability(1, vec![field_hit(1, 3, false)]),
        lastword(ability(2, vec![field_hit(3, 1, true)])),
    ]);
    let mut initial = source_position("field", "unit-follower", &json!({}));
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"survivor","card":"unit-follower","state":{"hp":10,"max_hp":10}}));
    let mut game = source_game(
        &initial,
        true,
        "unit-follower",
        &program,
        &printed_observer(true),
    );
    activate(&mut game, 1);
    resolved(play_pending_source(&mut game, "unit-amulet", 1));
    for _ in 0..2 {
        resolved(play_pending_source(&mut game, "unit-follower", 2));
        finish_source_checks(&mut game);
    }
    assert_eq!(observe_all(&mut game), 0);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 19);
    assert_eq!(after["objects"]["s"]["damage"], 0);
    assert_eq!(after["objects"]["survivor"]["damage"], 5);
}
