#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Synthetic questions must retain their declared target and payment kinds."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain, InstructionOutcome};

fn tree() -> Value {
    let mut document = evolution_linked_movement::tree(&[
        (
            "activate",
            json!({"per_turn":1,"cost":{"banish":{"sel":{"who":"self","zone":"cemetery","trait":"fallen"},"n":3}},"targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],"do":[{"steal":{"ref":"enemy"}},{"stand":"enemy"}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"targets":[{"bind":"own","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],"do":[{"destroy":"own"}]}),
        ),
    ]);
    let victim = evolution_linked_movement::tree(&[
        ("fanfare", json!({"do":[{"life":{"who":"self","delta":1}}]})),
        (
            "lastword",
            json!({"do":[{"life":{"who":"self","delta":2}}]}),
        ),
    ]);
    document["cards"]["unit-amulet"] = victim["cards"]["unit-follower"].clone();
    document
}
fn programs() -> BTreeMap<String, WellFormedCardProgram> {
    lowerer().lower(&tree()).unwrap()
}
fn transfer_catalog(wrong_trait: bool) -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["traits"] = if wrong_trait && row["number"] == "unit-spell" {
                json!([])
            } else {
                json!(["fallen"])
            };
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー");
                row["faces"][0]["power"] = json!("3");
                row["faces"][0]["hp"] = json!("5");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("transfer.yaml".into(), document)])
            .unwrap(),
    )
}
fn initial(seat: u32, full: bool) -> Value {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let opponent = if seat == 1 { "P2" } else { "P1" };
    let mut initial = setup(seat, json!([]));
    initial["turn"]["active"] = json!(actor);
    initial["players"][actor]["leader"]["life"] = json!(10_u8);
    initial["players"][opponent]["leader"]["life"] = json!(10_u8);
    initial["players"][actor]["zones"]["hand"] = json!([]);
    initial["players"][actor]["zones"]["cemetery"] = json!([{"id":"c1","card":"unit-spell"},{"id":"c2","card":"unit-follower"},{"id":"c3","card":"unit-amulet"}]);
    initial["players"][actor]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    if full {
        initial["players"][actor]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .extend((0..4).map(|n| json!({"id":format!("filler{n}"),"card":"unit-follower"})));
    }
    initial["players"][opponent]["zones"]["field"] = json!([{"id":"victim","card":"unit-amulet","state":{"acted":true,"damage":1_u8,"power":7_u8,"hp":4_u8,"max_hp":5_u8,"counters":{"kept":2_u8}}}]);
    initial
}
fn loaded(
    initial: &Value,
    programs: &BTreeMap<String, WellFormedCardProgram>,
    wrong_trait: bool,
) -> Game {
    let mut game = Game::new(
        transfer_catalog(wrong_trait),
        initial,
        &Value::Null,
        &Value::Null,
        "transfer",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("question")
    };
    question
}
fn steal(
    game: &mut Game,
    programs: &BTreeMap<String, WellFormedCardProgram>,
    seat: u32,
) -> Activation {
    let mut play = game
        .activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat },
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(_) => Answer::Targets(vec![0]),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.cardinality.minimum,
                    Nat(3),
                    "the printed cost requires three cards"
                );
                Answer::CostMaterials(
                    domain
                        .members
                        .iter()
                        .take(3)
                        .map(|member| member.handle)
                        .collect(),
                )
            }
            _ => panic!("steal declaration or payment"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    resolved(play)
}
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
#[test]
fn transfer_preserves_the_same_field_object_and_full_field_still_pays_and_stands() {
    let programs = programs();
    for seat in [1_u32, 2] {
        for full in [false, true] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let opponent = if seat == 1 { "P2" } else { "P1" };
            let mut initial = initial(seat, full);
            initial["players"][actor]["zones"]["cemetery"]
                .as_array_mut()
                .unwrap()
                .extend([
                    json!({"id":"c4","card":"unit-spell"}),
                    json!({"id":"c5","card":"unit-follower"}),
                    json!({"id":"c6","card":"unit-amulet"}),
                ]);
            let mut game = loaded(&initial, &programs, false);
            let before = game.projection(View::Referee).unwrap();
            let played = steal(&mut game, &programs, seat);
            assert!(played.progress.receipts.values().any(|outcome| matches!(outcome,InstructionOutcome::Transferred(members) if members.len()==usize::from(!full))));
            finish(&mut game);
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "control change must not trigger Fanfare"
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[actor]["cemetery"].as_array().unwrap().len(), 3_usize);
            assert_eq!(after[actor]["banish"].as_array().unwrap().len(), 3_usize);
            let controller = if full { opponent } else { actor };
            assert!(
                after[controller]["field"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("victim"))
            );
            for key in ["generation", "power", "hp", "max_hp", "damage", "counters"] {
                assert_eq!(
                    after["objects"]["victim"][key],
                    before["objects"]["victim"][key]
                );
            }
            assert_eq!(after["objects"]["victim"]["acted"], false);
            assert_eq!(after[actor]["leader"]["life"], 10_u8);
            let before_retry = game.projection(View::Referee).unwrap();
            game.activate_internal_card(
                "source",
                programs["unit-follower"].card().abilities[0].node.id,
                PlayerId { seat },
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before_retry);
        }
    }
}
#[test]
fn transfer_rejects_insufficient_trait_payment_and_forged_target_without_mutation() {
    let programs = programs();
    let mut game = loaded(&initial(1, false), &programs, true);
    let before = game.projection(View::Referee).unwrap();
    let mut attempt = game.activate_internal_card(
        "source",
        programs["unit-follower"].card().abilities[0].node.id,
        PlayerId { seat: 1 },
    );
    while let Ok(CardPlay::Awaiting(question)) = attempt {
        let answer = match &question.domain {
            InputDomain::Targets(_) => Answer::Targets(vec![0]),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(domain.members.iter().map(|member| member.handle).collect())
            }
            _ => panic!("insufficient material question"),
        };
        attempt = game.answer_internal_card(question.actor, question.token, answer);
    }
    attempt.unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let mut game = loaded(&initial(1, false), &programs, false);
    let target = question(
        game.activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![99]))
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
#[test]
fn transfer_departure_uses_original_owner_and_lastword_uses_the_thief() {
    let programs = programs();
    let mut game = loaded(&initial(1, false), &programs, false);
    steal(&mut game, &programs, 1);
    finish(&mut game);
    let target = question(
        game.activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("own target")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("unit-amulet"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![handle]))
            .unwrap(),
    );
    finish(&mut game);
    let pending = game.internal_pending_choice().unwrap().unwrap();
    assert_eq!(pending.actor, PlayerId { seat: 1 });
    let InputDomain::Pending(options) = pending.domain else {
        panic!("Lastword")
    };
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["victim"]));
    assert_eq!(after["P1"]["leader"]["life"], 12_u8);
    assert_eq!(after["P2"]["leader"]["life"], 10_u8);
}
#[test]
fn transfer_lowering_and_admission_keep_multi_target_and_other_destinations_closed() {
    for (key, value) in [
        ("who", json!("self")),
        ("zone", json!("cemetery")),
        ("kind", json!("amulet")),
    ] {
        let mut document = tree();
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["from"][key] =
            value;
        lowerer().lower(&document).unwrap_err();
    }
    for n in [json!(2_u8), json!({"upto":1_u8})] {
        let mut document = tree();
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"][0]["n"] = n;
        lowerer().lower(&document).unwrap_err();
    }
    for action in [
        json!({"steal":{"ref":"this"}}),
        json!({"steal":{"ref":"enemy","to":"self"}}),
        json!({"steal":{"ref":"enemy","result":"stolen"}}),
        json!({"give":{"ref":"enemy"}}),
    ] {
        let mut document = tree();
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([action]);
        lowerer().lower(&document).unwrap_err();
    }
    let programs = programs();
    for destination in [PlayerRef::Active {}, PlayerRef::Owner {}] {
        let mut raw = programs["unit-follower"].card().clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("steal")
        };
        let Action::Transfer {
            destination_controller,
            ..
        } = &mut instruction.action
        else {
            panic!("transfer")
        };
        *destination_controller = destination;
        let program = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            transfer_catalog(false),
            &initial(1, false),
            &Value::Null,
            &Value::Null,
            "reject-transfer",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program)
            .unwrap_err();
    }
}

fn model_catalog() -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let mut rows = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["traits"] = json!(["fallen"]);
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー");
                row["faces"][0]["power"] = json!("3");
                row["faces"][0]["hp"] = json!("5");
            }
            row.to_string()
        })
        .collect::<Vec<_>>();
    for (number, kind, power, hp, name) in [
        ("unit-form", "フォロワー・エボルヴ", "5", "6", "unit-amulet"),
        (
            "unit-gear",
            "イクイップメント・トークン",
            "-",
            "-",
            "unit-gear",
        ),
    ] {
        rows.push(json!({"number":number,"faces":[{"name":name,"card_class":"ニュートラル","card_type":kind,"cost":"-","power":power,"hp":hp,"traits":[],"text":"","sections":[]}]}).to_string());
    }
    let mut document: Value = serde_json::from_str(&document).unwrap();
    for number in ["unit-form", "unit-gear"] {
        document["cards"][number] = json!({"status":"complete","abilities":[]});
    }
    Arc::new(
        Catalog::from_documents(
            &rows.join("\n"),
            &keywords,
            &[("model.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}
fn model_loaded(
    initial: &Value,
    programs: &BTreeMap<String, WellFormedCardProgram>,
    keyword: &str,
) -> Game {
    let mut game = Game::new(
        model_catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "transfer-model",
    )
    .unwrap();
    for (number, program) in programs {
        game.load_internal_card(number, program).unwrap();
    }
    for number in ["unit-spell", "unit-gear"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let form = if keyword.is_empty() {
        card(vec![])
    } else {
        let mut definition = static_keywords::conditional_keyword();
        definition.source = source("unit-form");
        definition.source.line = 2;
        let Effect::Continuous {
            patch: CharacteristicPatch::GrantAbility { abilities },
            ..
        } = &mut definition.node.declarations[0]
        else {
            panic!("keyword fixture");
        };
        abilities[0] = GrantedAbility::Keyword {
            keyword: if keyword == "rush" {
                KeywordId::Rush
            } else {
                KeywordId::Storm
            },
        };
        card(vec![definition])
    };
    game.load_internal_card("unit-form", &form).unwrap();
    game
}
#[test]
fn transfer_keeps_evolution_and_equipment_but_previous_evolution_does_not_allow_an_attack() {
    let programs = programs();
    for keyword in ["", "rush", "storm"] {
        let mut initial = initial(1, false);
        initial["players"]["P2"]["zones"]["field"][0]["state"]["evolved"] = json!(true);
        initial["players"]["P2"]["zones"]["field"][0]["state"]["evolved_with"] = json!("form");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"z-target","card":"unit-follower","state":{"acted":true}}));
        initial["players"]["P2"]["zones"]["evolution"] = json!([{"id":"form","card":"unit-form"}]);
        initial["players"]["P2"]["zones"]["equipment"] =
            json!([{"id":"gear","card":"unit-gear","state":{"equipped_to":"victim"}}]);
        let mut game = model_loaded(&initial, &programs, keyword);
        let before = game.projection(View::Referee).unwrap();
        steal(&mut game, &programs, 1);
        finish(&mut game);
        let transferred = game.projection(View::Referee).unwrap();
        assert_eq!(transferred["objects"]["victim"]["evolved_with"], "form");
        assert_eq!(transferred["P2"]["evolution"], json!(["form"]));
        assert_eq!(transferred["P1"]["equipment"], json!(["gear"]));
        assert_eq!(
            transferred["objects"]["gear"]["generation"],
            before["objects"]["gear"]["generation"]
        );
        let target = if keyword == "storm" {
            "P2.leader"
        } else {
            "z-target"
        };
        game.internal_rule_action(&json!({"do":"attack","attacker":"victim","target":target}))
            .unwrap();
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["victim"]["acted"], !keyword.is_empty());
        assert_eq!(after["objects"]["victim"]["evolved_with"], "form");
    }
}
#[test]
fn transfer_base_uses_the_thiefs_evolution_resource_and_returns_each_to_its_owner() {
    let mut document = tree();
    let mut evolution = document["cards"]["unit-amulet"]["abilities"][0].clone();
    evolution.as_object_mut().unwrap().remove("fanfare");
    evolution["line"] = json!(3_u8);
    evolution["evolve"] = json!({"cost":{"pp":1}});
    document["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(evolution);
    let programs = lowerer().lower(&document).unwrap();
    let mut initial = initial(1, false);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"unit-form"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"z-target","card":"unit-follower","state":{"acted":true}}));
    let mut game = model_loaded(&initial, &programs, "");
    steal(&mut game, &programs, 1);
    finish(&mut game);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"victim","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    finish(&mut game);
    let evolved = game.projection(View::Referee).unwrap();
    assert_eq!(evolved["objects"]["victim"]["evolved_with"], "form");
    assert_eq!(evolved["P1"]["evolution"], json!(["form"]));
    for (target, allowed) in [("P2.leader", false), ("z-target", true)] {
        let mut attack = game.clone();
        attack
            .internal_rule_action(&json!({"do":"attack","attacker":"victim","target":target}))
            .unwrap();
        assert_eq!(
            attack.projection(View::Referee).unwrap()["objects"]["victim"]["acted"],
            allowed
        );
    }

    let target = question(
        game.activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let InputDomain::Targets(domain) = &target.domain else {
        panic!("own target")
    };
    let handle = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("unit-amulet"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![handle]))
            .unwrap(),
    );
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["victim"]));
    assert_eq!(after["P1"]["evolve_deck"], json!(["form"]));
    assert_eq!(after["objects"]["form"]["face_up"], true);
}

#[test]
fn transfer_does_not_turn_an_old_field_object_into_an_entered_this_turn_target() {
    let mut document = tree();
    let mut probe = document["cards"]["unit-follower"]["abilities"][0].clone();
    probe["line"] = json!(3_u8);
    probe["activate"] = json!({"cost":{"pp":0},"targets":[{"bind":"fresh","from":{"who":"self","zone":"field","kind":"follower","other":true,"entered_this_turn":true},"n":1}],"do":[{"stand":"fresh"}]});
    document["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(probe);
    let programs = lowerer().lower(&document).unwrap();
    for entered in [false, true] {
        let mut initial = initial(1, false);
        initial["players"]["P2"]["zones"]["field"][0]["state"]["entered_this_turn"] =
            json!(entered);
        let mut game = loaded(&initial, &programs, false);
        steal(&mut game, &programs, 1);
        finish(&mut game);
        let probe = game.activate_internal_card(
            "source",
            programs["unit-follower"].card().abilities[2].node.id,
            PlayerId { seat: 1 },
        );
        if entered {
            let question = question(probe.unwrap());
            let InputDomain::Targets(domain) = question.domain else {
                panic!("actual entry target")
            };
            assert!(
                domain
                    .members
                    .iter()
                    .any(|member| member.card.as_deref() == Some("unit-amulet"))
            );
        } else {
            match probe {
                Err(_) => {}
                Ok(CardPlay::Awaiting(question)) => {
                    let InputDomain::Targets(domain) = question.domain else {
                        panic!("entry target")
                    };
                    assert_eq!(domain.members, Vec::new());
                }
                Ok(_) => panic!("an old field object cannot satisfy entered_this_turn"),
            }
        }
    }
}

#[test]
fn transfer_does_not_rebind_a_target_to_a_later_generation_after_it_leaves_the_field() {
    let mut document = tree();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"move":{"ref":"enemy","to":"hand"}},{"steal":{"ref":"enemy"}},{"stand":"enemy"}
    ]);
    let programs = lowerer().lower(&document).unwrap();
    let mut game = loaded(&initial(1, false), &programs, false);
    let before = game.projection(View::Referee).unwrap();
    let played = steal(&mut game, &programs, 1);
    finish(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert!(played.progress.receipts.values().any(
        |outcome| matches!(outcome,InstructionOutcome::Transferred(members) if members.is_empty())
    ));
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P2"]["hand"], json!(["victim"]));
    assert_eq!(
        after["objects"]["victim"]["generation"].as_u64().unwrap(),
        before["objects"]["victim"]["generation"].as_u64().unwrap() + 1_u64
    );
}
