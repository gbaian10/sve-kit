use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn program(cost: &Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"quick":true,"cost":cost,"targets":[{
        "bind":"chosen","from":{"who":"self","zone":"field","kind":"follower","acted":false},"n":1_u8
    }],"do":[{"draw":{"n":1_u8}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn source_and_material() -> Value {
    json!({"steps":[{"act_self":true},{"act":{"sel":{"who":"self","zone":"field"},"n":1_u8}}]})
}

fn board(extra: bool) -> Game {
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"source","card":"unit-follower","state":{"acted":false}})];
    if extra {
        field.push(json!({"id":"fuel","card":"unit-amulet","state":{"acted":false}}));
    }
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "quick-public-proof",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program(&source_and_material()))
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    game
}

#[expect(
    clippy::panic,
    reason = "A missing question invalidates the payment protocol fixture."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("Quick material question");
    };
    question
}

#[expect(
    clippy::panic,
    reason = "Each stage must expose the actual public material question."
)]
fn materials(game: &mut Game) -> DecisionView {
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick window");
    };
    assert_eq!(
        options.len(),
        1,
        "the source has a complete affordable plan"
    );
    let target = awaiting(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    assert!(
        matches!(target.domain, InputDomain::Targets(_)),
        "formal targets precede public cost materials"
    );
    awaiting(
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap(),
    )
}

#[test]
fn quick_quotes_find_a_complete_cost_plan_without_double_acting_the_source() {
    for extra in [false, true] {
        let mut game = board(extra);
        let before = game.projection(View::Referee).unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick window");
        };
        assert_eq!(
            options.len(),
            usize::from(extra),
            "one object cannot pay two Act costs"
        );
        let after = game.projection(View::Referee).unwrap();
        for key in ["objects", "P1", "P2"] {
            assert_eq!(
                after[key], before[key],
                "quoting cannot commit posture, cards or resources"
            );
        }
        if extra {
            let question = materials(&mut game);
            let InputDomain::CostMaterials(domain) = &question.domain else {
                panic!("cost materials");
            };
            assert_eq!(
                domain.members.len(),
                2,
                "the quote must test plans rather than exclude legal candidates globally"
            );
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap_err();
            let rejected = game.projection(View::Referee).unwrap();
            assert_eq!(
                rejected["objects"]["source"]["acted"], false,
                "failed payment restores the first Act"
            );
            assert_eq!(
                rejected["objects"]["fuel"]["acted"], false,
                "unpaid alternatives stay standing"
            );
            let retry = materials(&mut game);
            resolved(
                game.answer_internal_card(retry.actor, retry.token, Answer::CostMaterials(vec![1]))
                    .unwrap(),
            );
            let paid = game.projection(View::Referee).unwrap();
            assert_eq!(
                paid["objects"]["source"]["acted"], true,
                "the source pays its own Act"
            );
            assert_eq!(
                paid["objects"]["fuel"]["acted"], true,
                "a distinct object pays the material Act"
            );
            assert_eq!(
                paid["P1"]["hand"],
                json!(["drawn"]),
                "the target plan is checked before payment changes posture"
            );
        }
    }
}

fn reject(raw: CardProgram, expected: &str, number: &str) {
    let typed = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "quick-public-closed",
    )
    .unwrap();
    let error = game.load_internal_card(number, &typed).unwrap_err();
    assert!(
        error.to_string().contains(expected),
        "expected {expected}: {error}"
    );
}

fn single() -> CardProgram {
    program(&json!({"banish":{"sel":{"who":"self","zone":"field"},"n":1_u8}}))
        .card()
        .clone()
}

fn material(play: &mut PlaySpec) -> &mut Decision {
    play.parameters
        .iter_mut()
        .find(|d| {
            matches!(
                d.kind,
                DecisionKind::PlayParameter {
                    slot: PlaySlot::CostMaterials,
                    ..
                }
            )
        })
        .unwrap()
}

#[test]
fn quick_public_materials_still_reject_other_zones_variable_counts_unions_and_groups() {
    for (cost, expected) in [
        (
            json!({"banish":{"sel":{"who":"self","zone":"cemetery"},"n":1_u8}}),
            "fixed own field or EX Quick materials",
        ),
        (
            json!({"banish":{"sel":{"who":"self","zone":["field","ex"]},"n":1_u8}}),
            "fixed own field or EX Quick materials",
        ),
        (
            json!({"banish":{"sel":{"who":"self","zone":"field"},"any":true}}),
            "fixed own field or EX Quick materials",
        ),
    ] {
        reject(program(&cost).card().clone(), expected, "unit-follower");
    }
    for variant in ["opponent", "hand", "upto"] {
        let mut raw = single();
        let decision = material(raw.abilities[0].node.play.as_mut().unwrap());
        let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
            panic!("checked public material fixture shape")
        };
        let Selector::Zone { players, zone, .. } = &mut constraint.groups[0].selector else {
            panic!("checked public material fixture shape")
        };
        let expected = match variant {
            "opponent" => {
                players.relation = PlayerRelation::Opponents;
                "public material selector"
            }
            "hand" => {
                *zone = ZoneKind::Hand;
                "public material selector"
            }
            _ => {
                constraint.cardinality.minimum = Nat(0);
                constraint.cardinality.maximum = Some(Nat(1));
                constraint.groups[0].cardinality = constraint.cardinality;
                "strict single material group"
            }
        };
        if variant == "hand" {
            let error = WellFormedCardProgram::check(raw).unwrap_err();
            assert_eq!(
                error.ir_path,
                "/abilities/0/play/parameters/1/domain/groups/0"
            );
            assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
        } else {
            reject(raw, expected, "unit-follower");
        }
    }
    let mut raw = single();
    let mut second = material(raw.abilities[0].node.play.as_mut().unwrap()).clone();
    second.id = DecisionId(900);
    let ChoiceDomain::Objects {
        binding,
        constraint,
        ..
    } = &mut second.domain
    else {
        panic!("checked public material fixture shape")
    };
    *binding = Binding::new(900, raw.abilities[0].node.scope);
    let Selector::Zone {
        binding: candidate, ..
    } = &mut constraint.groups[0].selector
    else {
        panic!("checked public material fixture shape")
    };
    *candidate = Binding::new(901, ScopeId(901));
    raw.abilities[0]
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(second);
    reject(raw, "one public material group", "unit-follower");
}

#[test]
fn quick_spell_still_rejects_public_cost_materials() {
    let mut raw = card(vec![play::spell(vec![draw(1, 1)]), quick::permit()])
        .card()
        .clone();
    let mut baseline = single();
    let mut parameter = material(baseline.abilities[0].node.play.as_mut().unwrap()).clone();
    let ChoiceDomain::Objects { binding, .. } = &mut parameter.domain else {
        panic!("checked public material fixture shape")
    };
    *binding = Binding::new(900, raw.abilities[0].node.scope);
    parameter.id = DecisionId(900);
    raw.abilities[0]
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(parameter);
    reject(raw, "material fee Quick Spell preflight", "unit-spell");
}
