use super::*;
use crate::ir::lowering::{ExpandedLowerer, validator};
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

#[path = "../../../tests/support/expanded.rs"]
mod expanded;

#[cfg(feature = "runner")]
pub(crate) fn payment_expanded_tree() -> &'static Value {
    expanded::tree()
}

#[path = "current_keywords.rs"]
mod current_keywords;

#[path = "drive_mode_count.rs"]
mod drive_mode_count;

mod nested_ability;

use std::sync::OnceLock;

fn lowerer() -> &'static ExpandedLowerer {
    static LOWERER: OnceLock<ExpandedLowerer> = OnceLock::new();
    LOWERER.get_or_init(|| ExpandedLowerer::new().unwrap())
}

#[test]
fn keyword_sources_are_scope_free_and_keep_their_own_coordinate() {
    let mut tree = expanded::tree().clone();
    let mut seed = tree["cards"]["BP08-038"].clone();
    let mut declaration = seed["abilities"][0].clone();
    declaration.as_object_mut().unwrap().remove("fanfare");
    declaration["sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("ability_scope");
    declaration["keywords"] = json!(["quick"]);
    seed["abilities"] = json!([declaration.clone()]);
    tree["cards"] = json!({"synthetic": seed});
    let authored = validator("authored").unwrap();
    let expanded = validator("expanded").unwrap();
    assert!(authored.is_valid(&tree));
    assert!(expanded.is_valid(&tree));
    let lowerer = lowerer();
    let programs = lowerer.lower(&tree).unwrap();
    let source = &programs["synthetic"].card().abilities[0].source;
    assert_eq!(
        source.scope,
        "keyword_declaration:/cards/synthetic/abilities/0"
    );
    assert_eq!(source.author_path, "/cards/synthetic/abilities/0");
    assert_eq!(
        source.fragment.as_deref(),
        declaration["sources"][0]["binding_id"].as_str()
    );

    let pointer = "/cards/synthetic/abilities/0";
    let mut old = tree.clone();
    old.pointer_mut(pointer).unwrap()["sources"][0]["ability_scope"] = json!("ability_0");
    let mut ordinary = tree.clone();
    ordinary
        .pointer_mut(pointer)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("keywords");
    ordinary.pointer_mut(pointer).unwrap()["spell"] = json!({"do":[]});
    let mut imports = tree.clone();
    imports.pointer_mut(pointer).unwrap()["imports"] =
        json!({"p":{"binding_id":"b","ability_scope":"ability_0","port":"p"}});
    let mut port = tree.clone();
    port.pointer_mut(pointer).unwrap()["ports"]["exports"] = json!({"p":{"path":[],"bind":"x"}});
    let mut duplicate = tree.clone();
    duplicate.pointer_mut(pointer).unwrap()["keywords"] = json!(["quick", "quick"]);
    let mut generated = tree.clone();
    generated.pointer_mut(pointer).unwrap()["generated_by"] = json!({});
    for rejected in [old, ordinary, imports, port, duplicate, generated] {
        assert!(!authored.is_valid(&rejected));
        assert!(!expanded.is_valid(&rejected));
        lowerer.lower(&rejected).unwrap_err();
    }
    for keywords in [
        json!(["ward"]),
        json!(["aura"]),
        json!(["ward", "aura"]),
        json!(["storm"]),
        json!(["rush"]),
        json!(["bane"]),
        json!(["intimidate"]),
        json!(["assail"]),
        json!(["ward", "storm"]),
        json!(["drain"]),
        json!(["ward", "drain"]),
        json!(["stack"]),
        json!(["ward", "stack"]),
    ] {
        let mut supported = tree.clone();
        supported.pointer_mut(pointer).unwrap()["keywords"] = keywords;
        lowerer.lower(&supported).unwrap();
    }
    {
        let mut unsupported = tree.clone();
        unsupported.pointer_mut(pointer).unwrap()["keywords"] = json!(["quick", "aura"]);
        assert!(expanded.is_valid(&unsupported));
        let failure = lowerer.lower(&unsupported).unwrap_err();
        assert_eq!(failure.code, "unsupported_construct");
        assert_eq!(failure.path, format!("{pointer}/keywords"));
    }
}

#[test]
fn observed_optional_trait_choice_lowers_and_rejects_non_observation_sources() {
    let lowerer = lowerer();
    let tree = expanded::tree().clone();
    lowerer.lower(&tree).unwrap();

    let mut whole_deck = tree.clone();
    *whole_deck
        .pointer_mut("/cards/BP08-055/abilities/0/fanfare/do/1/choose/from")
        .unwrap() = json!({"who":"self","zone":"deck","trait":"ドラゴニュート"});
    lowerer.lower(&whole_deck).unwrap_err();

    let mut missing_observation = tree;
    *missing_observation
        .pointer_mut("/cards/BP08-055/abilities/0/fanfare/do/1/choose/from/in")
        .unwrap() = json!("not_seen");
    lowerer.lower(&missing_observation).unwrap_err();
}

#[test]
fn numeric_duration_and_continuous_preserve_distinct_semantics() {
    let mut tree = expanded::tree().clone();
    let steps = &mut tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"];
    *steps = json!([
        {"set_stat":{"on":"this","hp":1_i64,"until":"next_opp_turn"}},
        {"buff":{"on":"this","power":-1_i64,"until":"this_and_next_opp_turn"}},
        {"apply":{"on":"this","effect":{"stat":{"hp":2_i64}},"until":"turn"}},
        {"apply":{"on":{"who":"self","zone":"field"},"effect":{"stat":{"power":1_i64}}}}
    ]);
    let second = tree["cards"]["BP08-038"]["abilities"][0].clone();
    tree["cards"]["BP08-038"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let cards = lowerer().lower(&tree).unwrap();
    let body = cards["BP08-038"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let actions = body
        .steps
        .iter()
        .map(|step| match &step.node {
            Node::Instruction { instruction } => &instruction.action,
            Node::Control { .. } | Node::Capture { .. } | Node::Decision { .. } => {
                panic!("instruction")
            }
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        actions[0],
        Action::Modify {
            patch: CharacteristicPatch::Numeric {
                change: NumericChange::Set { .. },
                ..
            },
            expires: Duration {
                end: DurationEnd::NextTurnEnd { .. },
                ..
            },
            ..
        }
    ));
    assert!(
        matches!(actions[1], Action::Modify { expires: Duration { end: DurationEnd::CurrentTurnEnd {}, intervals, .. }, .. } if matches!(intervals.as_slice(), [Interval::DuringNextTurn { .. }]))
    );
    assert!(
        matches!(actions[2], Action::InstallEffect { effect, .. } if matches!(effect.as_ref(), Effect::Continuous { layer: ContinuousLayer::Numeric, .. }))
    );
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["set_stat"]["hp"] = json!("X");
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["define"]["X"] = json!({
        "agg":{"of":{"who":"self","zone":"field","kind":"follower"},"attr":"power","fn":"sum"}
    });
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct",
        "a Compute aggregate is a reader, rather than a literal composition"
    );
    let patch = tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0]["set_stat"].take();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"][0] = json!({"buff":patch});
    lowerer().lower(&tree).unwrap();
}

#[test]
fn printed_spell_cost_targets_distribution_and_choice_preserve_play_scopes() {
    let lowerer = lowerer();
    let mut tree = expanded::spell_tree().clone();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
    let initial_cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let initial_ability = &initial_cards["SD03-005"].card().abilities[0].node;
    assert_eq!(initial_ability.kind, AbilityKind::Spell);
    assert_eq!(initial_ability.active_zones, vec![ZoneKind::Resolution]);
    assert!(matches!(
        initial_ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    let body = "/cards/SD03-005/abilities/0/spell";
    *tree.pointer_mut(body).unwrap() = json!({
        "targets":[{"bind":"t","from":{"who":"opp","zone":"field"},"upto":2_u64}],
        "do":[{"damage":{"to":"t","n":3_u64,"distribute":true}}]
    });
    let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let ability = &cards["SD03-005"].card().abilities[0].node;
    assert_play_distribution(ability);
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["kind"] = json!("follower");
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["trait"] = json!("兵士");
    let filtered = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    assert_target_characteristics(&filtered["SD03-005"]);
    tree.pointer_mut(body).unwrap()["targets"][0]["from"]["power_max"] = json!(3_u64);
    lowerer.lower_with_printed_costs(&tree, &costs).unwrap_err();
    *tree.pointer_mut(body).unwrap() = json!({"targets":[{"bind":"mode_t","from":{"who":"opp","zone":"field"},"n":1_u64,"option":1_u64}], "do":[{"choice":{"n":1_u64,"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]}}]});
    let choices = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let program = choices["SD03-005"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Control {
        control:
            Control::Mode {
                modes,
                chosen,
                choice_kind,
                ..
            },
    } = &program.steps[0].node
    else {
        panic!("choice keyword");
    };
    assert_eq!(*choice_kind, ModeChoiceKind::ChoiceKeyword);
    assert!(matches!(
        chosen.kind,
        ModeDecisionKind::PlayParameter { .. }
    ));
    let ChoiceDomain::Objects { binding, .. } = modes[0].parameters[0].domain else {
        panic!("mode target");
    };
    assert_eq!(binding.key().0, modes[0].body.scope);
    assert_eq!(modes[1].parameters, []);
    assert_ne!(modes[0].body.scope, modes[1].body.scope);
    assert_ne!(modes[0].body.scope, program.scope);
    assert!(
        modes[0].body.steps[0]
            .source
            .author_path
            .ends_with("/spell/do/0/choice/options/0/0")
    );
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(3_u64);
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, &costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    assert_choice_extensions(lowerer, tree, &costs, body);
}

fn assert_choice_extensions(
    lowerer: &ExpandedLowerer,
    mut tree: Value,
    costs: &BTreeMap<String, Nat>,
    body: &str,
) {
    tree.pointer_mut(body).unwrap()["targets"][0]["option"] = json!(1_u64);
    tree.pointer_mut(body).unwrap()["do"][0]["choice"]["exclude_chosen"] = json!("this_turn");
    let excluded = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        &excluded["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps[0]
            .node,
        Node::Control {
            control: Control::Mode {
                chosen: ModeDecision {
                    domain: ChoiceDomain::Modes {
                        exclude_used: Some(ModeExclusion::CurrentTurnAbilityInstance),
                        ..
                    },
                    ..
                },
                ..
            }
        }
    ));
    tree.pointer_mut(body).unwrap()["do"][0]["choice"] = json!({"upto":2_u64,"instead":[{"if":{"spellchain":3_u64},"upto":1_u64}],"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]});
    let conditional = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        &conditional["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps[0]
            .node,
        Node::Control {
            control: Control::Mode {
                count: Expr::Compute(_),
                ..
            }
        }
    ));
    tree.pointer_mut(body).unwrap()["do"][0]["choice"]["instead"][0] =
        json!({"if":{"necro":3_u64},"n":2_u64});
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    let steps = &mut tree.pointer_mut(body).unwrap()["do"];
    steps[0]["choice"] = json!({"n":1_u64,"options":[[{"damage":{"to":"mode_t","n":2_u64}}],[{"draw":{"n":1_u64}}]]});
    steps
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"draw":{"n":1_u64}}));
    let surrounded = lowerer.lower_with_printed_costs(&tree, costs).unwrap();
    assert!(matches!(
        surrounded["SD03-005"].card().abilities[0]
            .node
            .body
            .as_ref()
            .unwrap()
            .steps
            .as_slice(),
        [
            Located {
                node: Node::Instruction { .. },
                ..
            },
            Located {
                node: Node::Control {
                    control: Control::Mode { .. }
                },
                ..
            }
        ]
    ));
    let second = tree.pointer(&format!("{body}/do/1")).unwrap().clone();
    tree.pointer_mut(&format!("{body}/do"))
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(second);
    assert_eq!(
        lowerer
            .lower_with_printed_costs(&tree, costs)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

fn assert_play_distribution(ability: &Ability) {
    let parameters = &ability.play.as_ref().unwrap().parameters;
    assert_eq!(parameters.len(), 2);
    let ChoiceDomain::Objects {
        binding: target,
        constraint,
        ..
    } = &parameters[0].domain
    else {
        panic!("targets");
    };
    assert_eq!(
        constraint.cardinality,
        Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        }
    );
    let ChoiceDomain::Distribution {
        recipients,
        total,
        minimum,
        binding,
    } = &parameters[1].domain
    else {
        panic!("distribution");
    };
    assert_eq!(
        recipients,
        &RecipientSet::Objects {
            objects: Selector::Bound { reference: *target }
        }
    );
    assert_eq!(total, &Expr::Literal(Nat(3)));
    assert_eq!(minimum, &Nat(1));
    let Node::Instruction { instruction } = &ability.body.as_ref().unwrap().steps[0].node else {
        panic!("damage");
    };
    assert!(
        matches!(instruction.action, Action::Damage { amount: DamageAmount::Allocation { binding: actual }, .. } if actual == *binding)
    );
}

#[test]
#[expect(
    clippy::default_numeric_fallback,
    reason = "Synthetic author JSON contains literal cost amounts."
)]
fn own_play_costs_preserve_explicit_intervals_and_reject_payment_shapes() {
    let mut tree = expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let count = json!({"count":{"who":"self","zone":"cemetery","kind":"spell"}});
    let threshold = |op, right| json!({"cmp":{"left":count,"op":op,"right":right}});
    entry["play"] = json!({"cost_delta":[
        {"if":threshold("ge",15),"delta":-9},
        {"if":{"all":[threshold("ge",10),threshold("lt",15)]},"delta":-6},
        {"if":{"all":[threshold("ge",5),threshold("lt",10)]},"delta":-3},-1,-1],"cost_set":7});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"synthetic":{"meta":meta,"abilities":[entry]}});
    let lowerer = lowerer();
    let cards = lowerer.lower(&tree).unwrap();
    let card = cards["synthetic"].card();
    assert_eq!(card.abilities, []);
    assert_eq!(card.card_play_costs[0].node.modifiers.len(), 6);
    assert!(
        card.card_play_costs[0].node.modifiers[1]
            .source
            .author_path
            .ends_with("/play/cost_delta/1")
    );
    let located = Located {
        source: source("synthetic"),
        node: card.clone(),
    };
    let requirements = Requirements::derive(&located);
    assert!(requirements.required.iter().any(|r| {
        r.capability.code == "card.play-cost"
            && r.locations
                .iter()
                .all(|location| location.ir_path.contains("card_play_costs"))
    }));
    tree["cards"]["synthetic"]["abilities"][0]["play"]["optional_cost"] = json!({"pp":0});
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().path,
        "/cards/synthetic/abilities/0/play/optional_cost"
    );
}

fn assert_target_characteristics(filtered: &WellFormedCardProgram) {
    let ChoiceDomain::Objects { constraint, .. } = &filtered.card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .parameters[0]
        .domain
    else {
        panic!("targets");
    };
    let Selector::Zone { predicate, .. } = &constraint.groups[0].selector else {
        panic!("field");
    };
    assert!(
        matches!(predicate.as_ref(), Expr::Compute(operation) if matches!(operation.as_ref(), Predicate::And { predicates } if matches!(predicates.as_slice(), [Expr::Compute(kind), Expr::Compute(traits)] if matches!(kind.as_ref(), Predicate::KindEquals { right: Expr::Literal(CardKind::Follower), .. }) && matches!(traits.as_ref(), Predicate::HasTrait { trait_id, .. } if trait_id.0 == "兵士"))))
    );
    assert_eq!(
        filtered.card().abilities[0].source.author_path,
        "/cards/SD03-005/abilities/0"
    );
}

#[test]
fn leader_targets_and_leader_follower_targets_keep_recipient_bindings() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    for from in [
        json!({"leader":"opp"}),
        json!({"leader":"self"}),
        json!({"leaders":"all"}),
        json!({"who":"opp","kind":"follower","leader_or":true}),
    ] {
        let mut tree = expanded::spell_tree().clone();
        *tree
            .pointer_mut("/cards/SD03-005/abilities/0/spell")
            .unwrap() = json!({
            "targets":[{"bind":"victim","from":from,"n":1_u64}],
            "do":[{"damage":{"to":"victim","n":3_u64}}]
        });
        let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
        let ability = &cards["SD03-005"].card().abilities[0].node;
        assert!(matches!(
            ability.play.as_ref().unwrap().parameters[0].domain,
            ChoiceDomain::Recipients { .. }
        ));
        let mut distinct = tree.clone();
        distinct["cards"]["SD03-005"]["abilities"][0]["spell"]["targets"][0]["distinct_by"] =
            json!("name");
        assert!(
            lowerer.lower_with_printed_costs(&distinct, &costs).is_err(),
            "leaders have no card-name distinctness contract"
        );
        tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] = json!([{"destroy":"victim"}]);
        assert!(
            lowerer.lower_with_printed_costs(&tree, &costs).is_err(),
            "recipient bindings cannot become object selectors"
        );
    }
}

#[test]
fn activated_zones_preserve_effective_zones_and_reject_combinations() {
    let lowerer = lowerer();
    for (zone, expected) in [
        (json!("field"), ZoneKind::Field),
        (json!(["field"]), ZoneKind::Field),
        (json!("ex"), ZoneKind::Ex),
        (json!(["ex"]), ZoneKind::Ex),
        (json!("hand"), ZoneKind::Hand),
        (json!(["hand"]), ZoneKind::Hand),
        (json!("cemetery"), ZoneKind::Cemetery),
        (json!(["cemetery"]), ZoneKind::Cemetery),
    ] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["zone"] = zone.clone();
        let cards = lowerer.lower(&tree).unwrap();
        assert_eq!(
            cards["BP01-041"].card().abilities[0].node.active_zones,
            [expected]
        );
    }
    for zone in [
        json!("deck"),
        json!(["hand", "cemetery"]),
        json!(["field", "ex"]),
        json!([]),
        json!(["ex", "ex"]),
    ] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["zone"] = zone;
        lowerer.lower(&tree).unwrap_err();
    }
}

#[test]
fn filtered_counts_reject_hidden_card_characteristics() {
    for zone in ["hand", "deck"] {
        for filter in [
            json!({"kind":"spell"}),
            json!({"trait":"学院"}),
            json!({"name":"example"}),
            json!({"name_contains":"example"}),
            json!({"orig_cost_eq":2_u8}),
        ] {
            let mut query = json!({"who":"self","zone":zone});
            query
                .as_object_mut()
                .unwrap()
                .extend(filter.as_object().unwrap().clone());
            let mut tree = expanded::tree().clone();
            tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([
                {"draw":{"n":{"count":query}}}
            ]);
            assert_eq!(
                lowerer().lower(&tree).unwrap_err().code,
                "unsupported_construct"
            );
        }
    }
}

#[test]
fn evolve_counts_reject_opponents_and_hidden_zone_face_filters() {
    for query in [
        json!({"who":"opp","zone":"evolve_deck","kind":"evolve_follower","face_up":true}),
        json!({"who":"self","zone":"hand","face_up":true}),
        json!({"who":"self","zone":"deck","face_up":false}),
    ] {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([
            {"draw":{"n":{"count":query}}}
        ]);
        assert_eq!(
            lowerer().lower(&tree).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

#[test]
fn mandatory_play_cost_uses_ordered_cost_operations_and_a_card_play_receipt() {
    let mut tree = expanded::spell_tree().clone();
    let card = tree["cards"]["SD03-005"].clone();
    tree["cards"] = json!({"SD03-005":card});
    let entry = &mut tree["cards"]["SD03-005"]["abilities"][0];
    entry["play"] = json!({"additional_cost":{
        "steps":[
            {"act":{"n":1_u8,"sel":{"who":"self","zone":"field","name":"first"}}},
            {"banish":{"n":1_u8,"sel":{"who":"self","zone":"ex"}}}
        ],"result":"extra"}});
    entry["spell"]["do"] =
        json!([{"if":{"cond":{"paid":{"receipt":"extra"}},"then":[{"draw":{"n":1_u8}}]}}]);
    let mut payment_entry = tree["cards"]["SD03-005"]["abilities"][0].clone();
    payment_entry.as_object_mut().unwrap().remove("spell");
    tree["cards"]["SD03-005"]["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("play");
    let spell_entry = tree["cards"]["SD03-005"]["abilities"][0].clone();
    tree["cards"]["SD03-005"]["abilities"] = json!([payment_entry, spell_entry]);
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let checked = lowerer().lower_with_printed_costs(&tree, &costs).unwrap();
    let payment = &checked["SD03-005"].card().card_play_costs[0]
        .node
        .additional
        .as_ref()
        .unwrap()
        .node;
    assert_eq!(payment.play.parameters.len(), 2);
    let CostSpec::Ordered { steps } = &payment.play.costs else {
        panic!("ordered costs");
    };
    assert!(matches!(
        steps[0].instruction.action,
        Action::Posture { .. }
    ));
    assert!(matches!(steps[1].instruction.action, Action::Banish { .. }));
    let mut duplicate = tree.clone();
    duplicate["cards"]["SD03-005"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(tree["cards"]["SD03-005"]["abilities"][0].clone());
    lowerer()
        .lower_with_printed_costs(&duplicate, &costs)
        .unwrap_err();
    tree["cards"]["SD03-005"]["abilities"][1]["spell"]["do"][0]["if"]["cond"]["paid"]["receipt"] =
        json!("foreign");
    lowerer()
        .lower_with_printed_costs(&tree, &costs)
        .unwrap_err();
}

#[test]
fn mandatory_play_cost_does_not_relax_ordinary_activation_material_group_limit() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"] = json!({"steps":[
        {"act":{"n":1_u8,"sel":{"who":"self","zone":"field","kind":"follower"}}},
        {"act":{"n":1_u8,"sel":{"who":"self","zone":"field","kind":"follower"}}}
    ]});
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn buff_replacement_rejects_unadmitted_stats_and_multiple_alternatives() {
    let mut tree = expanded::tree().clone();
    let replacement = json!({"if":{"history":{"event":"ub_activated","who":"self","window":"this_turn","exclude":"this_execution","at_least":2_u64}},"power":2_u64});
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] =
        json!([{"buff":{"on":"this","power":1_u64,"instead":[replacement]}}]);
    lowerer().lower(&tree).unwrap();
    for step in [
        json!({"buff":{"on":"this","power":1_u64,"hp":1_u64,"instead":[replacement]}}),
        json!({"buff":{"on":"this","hp":1_u64,"instead":[replacement]}}),
        json!({"buff":{"on":"this","power":1_u64,"instead":[]}}),
        json!({"buff":{"on":"this","power":1_u64,"instead":[replacement,replacement]}}),
        json!({"buff":{"on":"this","power":1_u64,"instead":[{"if":{"combo":2_u64},"hp":2_u64}]}}),
        json!({"set_stat":{"on":"this","power":1_u64,"instead":[replacement]}}),
    ] {
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([step]);
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn original_cost_totals_bound_public_targets_and_reject_leader_recipients() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let mut tree = expanded::spell_tree().clone();
    *tree
        .pointer_mut("/cards/SD03-005/abilities/0/spell")
        .unwrap() = json!({
        "targets":[{"bind":"picked","from":{"who":"self","zone":"cemetery","kind":"follower"},
            "upto":3_u64,"cost_sum_le":6_u64}],
        "do":[{"move":{"ref":"picked","to":"field","result":"returned"}}]
    });
    let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let ChoiceDomain::Objects { constraint, .. } = &cards["SD03-005"].card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .parameters[0]
        .domain
    else {
        panic!("cemetery targets");
    };
    assert_eq!(
        constraint.aggregate_bounds,
        [AggregateBound {
            property: NumericProperty::OriginalCost,
            comparison: Comparison::LessEqual,
            value: Expr::Literal(Int(6)),
        }]
    );
    let mut leaders = tree.clone();
    leaders["cards"]["SD03-005"]["abilities"][0]["spell"]["targets"][0] = json!({
        "bind":"picked","from":{"who":"opp","kind":"follower","leader_or":true},
        "n":1_u64,"cost_sum_le":6_u64
    });
    leaders["cards"]["SD03-005"]["abilities"][0]["spell"]["do"] =
        json!([{"damage":{"to":"picked","n":1_u64}}]);
    assert!(
        lowerer.lower_with_printed_costs(&leaders, &costs).is_err(),
        "leaders have no original cost"
    );
    leaders["cards"]["SD03-005"]["abilities"][0]["spell"]["targets"][0]
        .as_object_mut()
        .unwrap()
        .remove("cost_sum_le");
    lowerer.lower_with_printed_costs(&leaders, &costs).unwrap();
}

#[test]
fn search_original_cost_totals_join_the_search_constraint_and_reject_split_destinations() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let mut tree = expanded::spell_tree().clone();
    *tree
        .pointer_mut("/cards/SD03-005/abilities/0/spell")
        .unwrap() = json!({
        "do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},
            "upto":2_u64,"cost_sum_le":3_u64,"to":"field"}}]
    });
    let cards = lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    let [step] = cards["SD03-005"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps
        .as_slice()
    else {
        panic!("one search instruction");
    };
    let Node::Instruction { instruction } = &step.node else {
        panic!("search instruction");
    };
    let Action::Search {
        groups: Some(constraint),
        cardinality,
        ..
    } = &instruction.action
    else {
        panic!("a capped search keeps one selection constraint");
    };
    let [group] = constraint.groups.as_slice() else {
        panic!("one group spans the filtered deck");
    };
    assert!(
        matches!(&group.selector, Selector::Zone { zone: ZoneKind::Deck, predicate, .. }
            if matches!(predicate.as_ref(), Expr::Literal(Bool(true)))),
        "the group adds no filter beyond the search's own"
    );
    assert_eq!(group.cardinality, *cardinality);
    assert_eq!(constraint.distinct_by, None);
    assert_eq!(
        constraint.aggregate_bounds,
        [AggregateBound {
            property: NumericProperty::OriginalCost,
            comparison: Comparison::LessEqual,
            value: Expr::Literal(Int(3)),
        }]
    );
    let mut split = tree.clone();
    split["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][0]["search"]["to"] = json!({
        "order":"printed","partition":[{"zone":"hand","n":1_u64},{"zone":"cemetery","n":1_u64}]
    });
    assert!(
        lowerer.lower_with_printed_costs(&split, &costs).is_err(),
        "a hand and cemetery partition keeps its aggregate-free contract"
    );
    split["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][0]["search"]
        .as_object_mut()
        .unwrap()
        .remove("cost_sum_le");
    lowerer.lower_with_printed_costs(&split, &costs).unwrap();
}

#[test]
fn holder_name_exclusion_is_rejected_where_public_counts_have_no_card_needing_it() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let mut tree = expanded::spell_tree().clone();
    *tree
        .pointer_mut("/cards/SD03-005/abilities/0/spell")
        .unwrap() = json!({
        "do":[{"damage":{"to":{"leader":"opp"},
            "n":{"distinct_names":{"who":"self","zone":"field","kind":"follower"}}}}]
    });
    lowerer.lower_with_printed_costs(&tree, &costs).unwrap();
    tree["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][0]["damage"]["n"]["distinct_names"]
        ["not_same_name_as_this"] = json!(true);
    assert!(
        lowerer.lower_with_printed_costs(&tree, &costs).is_err(),
        "only cost materials and existence checks exclude the holder's name"
    );
}

#[test]
fn trait_unions_filter_public_targets_and_reject_single_traits_counts_and_leaders() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let spell = |targets: Value, step: Value| {
        let mut tree = expanded::spell_tree().clone();
        *tree
            .pointer_mut("/cards/SD03-005/abilities/0/spell")
            .unwrap() = json!({"targets": targets, "do": [step]});
        tree
    };
    let cemetery = |traits: Value| {
        spell(
            json!([{"bind":"picked","n":1_u64,"from":{"who":"self","zone":"cemetery",
                "kind":"follower","traits_any":traits}}]),
            json!({"move":{"ref":"picked","to":"field","result":"returned"}}),
        )
    };
    let cards = lowerer
        .lower_with_printed_costs(&cemetery(json!(["獣", "魔法生物"])), &costs)
        .unwrap();
    let ChoiceDomain::Objects { constraint, .. } = &cards["SD03-005"].card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .parameters[0]
        .domain
    else {
        panic!("cemetery targets");
    };
    let Selector::Zone { predicate, .. } = &constraint.groups[0].selector else {
        panic!("cemetery zone");
    };
    assert!(
        matches!(predicate.as_ref(), Expr::Compute(and) if matches!(and.as_ref(),
            Predicate::And { predicates } if predicates.iter().any(|item|
                matches!(item, Expr::Compute(or) if matches!(or.as_ref(),
                    Predicate::Or { predicates: branches } if branches.len() == 2))))),
        "the union is one OR of trait reads"
    );
    for traits in [json!(["獣"]), json!(["獣", "獣"]), json!(["獣", ""])] {
        assert!(
            lowerer
                .lower_with_printed_costs(&cemetery(traits.clone()), &costs)
                .is_err(),
            "{traits} is not a union of distinct traits"
        );
    }
    let leaders = spell(
        json!([{"bind":"picked","n":1_u64,"from":{"who":"opp","kind":"follower",
            "leader_or":true,"traits_any":["獣","魔法生物"]}}]),
        json!({"damage":{"to":"picked","n":1_u64}}),
    );
    lowerer
        .lower_with_printed_costs(&leaders, &costs)
        .unwrap_err();
    let count = spell(
        json!([]),
        json!({"damage":{"to":{"leader":"opp"},"n":{"distinct_names":{"who":"self",
            "zone":"field","traits_any":["獣","魔法生物"]}}}}),
    );
    assert!(
        lowerer.lower_with_printed_costs(&count, &costs).is_err(),
        "public counts have no card needing a trait union"
    );
}

#[test]
fn live_attack_reads_a_single_chosen_public_object() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let spell = |reader: &str| {
        let mut tree = expanded::spell_tree().clone();
        *tree
            .pointer_mut("/cards/SD03-005/abilities/0/spell")
            .unwrap() = json!({
            "targets":[
                {"bind":"ally","from":{"who":"self","zone":"field","kind":"follower"},"n":1_u64},
                {"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_u64}
            ],
            "do":[
                {"buff":{"on":"ally","power":1_u64,"hp":1_u64}},
                {"damage":{"to":"enemy","n":{"power_of":reader}}}
            ]
        });
        tree
    };
    let cards = lowerer
        .lower_with_printed_costs(&spell("ally"), &costs)
        .unwrap();
    let rendered = &cards["SD03-005"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps;
    let Node::Instruction { instruction } = &rendered[1].node else {
        panic!("damage instruction");
    };
    let Action::Damage {
        amount: DamageAmount::Uniform { amount },
        ..
    } = &instruction.action
    else {
        panic!("uniform damage");
    };
    assert!(
        matches!(amount, Expr::Compute(sum) if matches!(sum.as_ref(),
            IntOperation::Sum { objects: Selector::Bound { .. }, property: NumericProperty::Attack })),
        "the amount reads the chosen follower when the damage resolves"
    );
    // 「後者の攻撃力」 may name any single public target, not only an own follower.
    lowerer
        .lower_with_printed_costs(&spell("enemy"), &costs)
        .unwrap();
    // A body choose of exactly one own field follower is the same public single object.
    let mut chosen = expanded::spell_tree().clone();
    *chosen
        .pointer_mut("/cards/SD03-005/abilities/0/spell")
        .unwrap() = json!({
        "do":[
            {"choose":{"bind":"ally","from":{"who":"self","zone":"field","kind":"follower"},"n":1_u64}},
            {"buff":{"on":"ally","power":1_u64,"hp":1_u64}},
            {"damage":{"to":{"leader":"opp"},"n":{"power_of":"ally"}}}
        ]
    });
    lowerer.lower_with_printed_costs(&chosen, &costs).unwrap();
    chosen["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][0]["choose"]["upto"] = json!(2_u64);
    chosen["cards"]["SD03-005"]["abilities"][0]["spell"]["do"][0]["choose"]
        .as_object_mut()
        .unwrap()
        .remove("n");
    lowerer
        .lower_with_printed_costs(&chosen, &costs)
        .unwrap_err();
}

#[test]
fn material_cost_floors_accept_only_any_number_of_acted_followers() {
    let lowerer = lowerer();
    let activated = |cost: Value| {
        let mut tree = expanded::tree().clone();
        tree["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"] = cost;
        tree
    };
    let sel = json!({"who":"self","zone":"field","kind":"follower"});
    let cards = lowerer
        .lower(&activated(
            json!({"act":{"any":true,"cost_sum_ge":5_u64,"sel":sel}}),
        ))
        .unwrap();
    let parameters = &cards["BP01-041"].card().abilities[0]
        .node
        .play
        .as_ref()
        .unwrap()
        .parameters;
    assert!(
        parameters.iter().any(|decision| matches!(&decision.domain,
        ChoiceDomain::Objects { constraint, .. } if constraint.aggregate_bounds == [AggregateBound {
            property: NumericProperty::OriginalCost,
            comparison: Comparison::GreaterEqual,
            value: Expr::Literal(Int(5)),
        }])),
        "the floor joins the act material choice"
    );
    for cost in [
        json!({"act":{"n":2_u64,"cost_sum_ge":5_u64,"sel":sel}}),
        json!({"to_cemetery":{"any":true,"cost_sum_ge":5_u64,"sel":sel}}),
        json!({"act":{"any":true,"cost_sum_ge":0_u64,"sel":sel}}),
    ] {
        lowerer.lower(&activated(cost)).unwrap_err();
    }
}

#[test]
fn self_to_ex_cost_is_paid_only_by_a_hand_activation() {
    let lowerer = lowerer();
    let activated = |zone: Option<&str>| {
        let mut tree = expanded::tree().clone();
        let head = &mut tree["cards"]["BP01-041"]["abilities"][0]["activate"];
        head["cost"] = json!({"steps":[{"pp":1_u64},{"self_to_ex":true}]});
        if let Some(zone) = zone {
            head["zone"] = json!(zone);
        }
        tree
    };
    let cards = lowerer.lower(&activated(Some("hand"))).unwrap();
    let ability = &cards["BP01-041"].card().abilities[0].node;
    let CostSpec::Ordered { steps } = &ability.play.as_ref().unwrap().costs else {
        panic!("ordered cost")
    };
    assert!(steps.iter().any(|step| matches!(
        &step.instruction.action,
        Action::Move { movement, .. }
            if movement.to == (MoveDestination::Owners { zone: OwnerZone::Ex })
    )));
    for zone in [None, Some("field"), Some("ex"), Some("cemetery")] {
        lowerer.lower(&activated(zone)).unwrap_err();
    }
    // Quick candidates would not see a full EX before offering the payment.
    let mut quick = activated(Some("hand"));
    quick["cards"]["BP01-041"]["abilities"][0]["activate"]["quick"] = json!(true);
    lowerer.lower(&quick).unwrap_err();
    quick["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"] = json!({"pp":1_u64});
    lowerer.lower(&quick).unwrap();
}

#[test]
fn own_dice_events_and_unconditional_field_rerolls_lower() {
    let mut tree = expanded::tree().clone();
    let entry = &mut tree["cards"]["BP08-038"]["abilities"][0];
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["auto"] = json!({"on":{"dice_rolled":{"who":"self"}},"do":[{"damage":{"to":{"leaders":"opponents"},"n":{"event":"value"}}}]});
    lowerer().lower(&tree).unwrap();
    let reroll = &mut tree["cards"]["BP08-038"]["abilities"][0];
    reroll.as_object_mut().unwrap().remove("auto");
    reroll["static"] = json!({"effects":[{"reroll":{"who":"self","times":1_u8}}]});
    lowerer().lower(&tree).unwrap();
    tree["cards"]["BP08-038"]["abilities"][0]["static"]["while"] = json!({"awakened":true});
    assert_eq!(
        lowerer().lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn public_top_reveal_reads_only_its_own_published_observation() {
    let lowerer = lowerer();
    let costs = BTreeMap::from([("SD03-005".to_owned(), Nat(1))]);
    let body = |first: Value, read: Value| {
        let mut tree = expanded::spell_tree().clone();
        *tree
            .pointer_mut("/cards/SD03-005/abilities/0/spell")
            .unwrap() = json!({"do":[
            first,
            {"life":{"who":"self","delta":read}}
        ]});
        tree
    };
    let reveal = json!({"reveal":{"top":{"who":"self","n":1_u8},"bind":"shown"}});
    for read in [
        json!({"agg":{"of":{"in":"shown"},"attr":"orig_cost","fn":"sum"}}),
        json!({"distinct_names":{"in":{"objects_of":"shown","view":"before"}}}),
    ] {
        lowerer
            .lower_with_printed_costs(&body(reveal.clone(), read.clone()), &costs)
            .unwrap();
        let nested =
            json!({"if":{"cond":{"leader_life":{"who":"self","le":20_u8}},"then":[reveal]}});
        let failure = lowerer
            .lower_with_printed_costs(&body(nested, read.clone()), &costs)
            .unwrap_err();
        assert_eq!(failure.code, "unsupported_construct");
        assert!(
            failure.path.ends_with("/if/then/0"),
            "the nested producer itself must be rejected"
        );
        for invalid in [
            json!({"look":{"top":{"who":"self","n":1_u8},"bind":"shown"}}),
            json!({"reveal":{"top":{"who":"opp","n":1_u8},"bind":"shown"}}),
            json!({"reveal":{"top":{"who":"self","n":0_u8},"bind":"shown"}}),
            json!({"reveal":{"top":{"who":"self","n":1_u8},"ref":"picked","bind":"shown"}}),
        ] {
            lowerer
                .lower_with_printed_costs(&body(invalid, read.clone()), &costs)
                .unwrap_err();
        }
    }
}
