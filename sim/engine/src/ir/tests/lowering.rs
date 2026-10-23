use super::*;
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer, validator};
use alloc::collections::BTreeMap;
use serde_json::{Value, json};

#[path = "../../../tests/support/expanded.rs"]
mod expanded;

use std::{path::PathBuf, process::Command, sync::OnceLock};

fn current_inputs() -> &'static Value {
    static CURRENT: OnceLock<Value> = OnceLock::new();
    CURRENT.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("uv")
            .args(["run", "--directory"])
            .arg(root.join("carddb"))
            .args([
                "--isolated",
                "--locked",
                "--no-default-groups",
                "python",
                "-c",
                "
import json
from tempfile import TemporaryDirectory
from pathlib import Path
from tests.support.current_lowering_fixtures import current_lowering_inputs
with TemporaryDirectory() as temporary:
    print(json.dumps(current_lowering_inputs(Path(temporary))))
",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "current C0 fixture failed: {}",
            output.status
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn assert_current_sources(admitted: &CurrentCardProgram, input: &Value, face: &str) {
    let evidence = admitted
        .source_at("/cards/BP01-009/abilities/0/evolve/cost/pp")
        .unwrap();
    assert_eq!(evidence.source.card_id, admitted.card_id);
    assert_eq!(evidence.source.face, face);
    assert_eq!(evidence.source.field, "effect");
    assert_eq!(evidence.source.section, None);
    assert_eq!(evidence.source.line, 1);
    assert_eq!(
        evidence.source.fragment.as_deref(),
        evidence.source_use["binding_id"].as_str()
    );
    assert_eq!(
        evidence.source.owner,
        evidence.source_use["occurrence"]["owner"].to_string()
    );
    assert_eq!(evidence.input_hash, input["sidecar"][0]["input_hash"]);
    assert_eq!(
        evidence.source_input_hash,
        input["sidecar"][0]["source_input_hash"]
    );
    assert!(admitted.source_at("/cards/BP01-009/abilities/01").is_none());
    let requirements = Requirements::derive(&Located {
        source: admitted.program.card().abilities[0].source.clone(),
        node: admitted.program.card().clone(),
    });
    for requirement in requirements.required {
        for location in requirement.locations {
            let checked = admitted.source_at(&location.source.author_path).unwrap();
            assert_eq!(checked.source, location.source);
        }
    }
}

#[test]
fn current_reader_hands_permanent_owner_and_token_selection_to_lowering() {
    let input = &current_inputs()["accepted"];
    let lowerer = ExpandedLowerer::new().unwrap();
    let admitted = lowerer.lower_current(input).unwrap();
    let face = "f:8708790ca2995a0f9f9e5ffd1af52668";
    let revision = "rev:v1:c9b82c4e4e8df5c8ab2b4975c3058c7e0ffdd179e76429d4e47f4c557cac7304";
    assert_eq!(admitted.card_id, "c:9c5459446e0254818099800b4208cd96");
    assert_eq!(admitted.card_no, "BP01-009");
    assert_eq!(admitted.source_version, input["source_version"]);
    assert_eq!(admitted.source_payload["token_links"], json!([]));
    assert_eq!(admitted.expanded, input["tree"]);
    assert_current_sources(&admitted, input, face);
    let ability = &admitted.program.card().abilities[0];
    assert_eq!(ability.source.card_id, admitted.card_id);
    assert_eq!(ability.source.card_no, admitted.card_no);
    assert_eq!(ability.source.face, face);
    assert_eq!(
        serde_json::from_str::<Value>(&ability.source.owner).unwrap(),
        json!({"kind":"face_revision", "revision_id":revision})
    );
    assert_eq!(ability.source.field, "effect");
    let body = ability.node.body.as_ref().unwrap();
    assert_eq!(body.steps[0].source.card_id, admitted.card_id);
    assert_eq!(body.steps[0].source.face, face);
    let AbilityOrigin::Keyword { origin, .. } = &ability.node.source.origin else {
        panic!("evolution");
    };
    assert!(
        matches!(origin.as_ref(), AbilityOrigin::Printed { face: owner, .. } if owner.0 == face)
    );
    let mut container = input.clone();
    let abilities = container["tree"]["cards"]["BP01-009"]
        .as_object_mut()
        .unwrap()
        .remove("abilities")
        .unwrap();
    container["tree"]["cards"]["BP01-009"]["faces"] = json!({face:{"abilities":abilities}});
    container["sidecar"][0]["path"] =
        json!(["cards", "BP01-009", "faces", face, "abilities", 0_u64]);
    let nested = lowerer.lower_current(&container).unwrap();
    assert_eq!(nested.program.card().abilities[0].source.face, face);
    assert!(
        nested.program.card().abilities[0]
            .source
            .author_path
            .contains("/faces/")
    );
}

#[test]
fn provisional_current_printing_and_unresolved_handoffs_cannot_lower() {
    let inputs = current_inputs();
    assert_eq!(inputs["rejected"]["reason"], "provisional_record_key");
    let lowerer = ExpandedLowerer::new().unwrap();
    assert_eq!(
        lowerer.lower_current(&inputs["rejected"]).unwrap_err().code,
        "invalid_current_identity"
    );
    for pointer in [
        "/card_id",
        "/source_payload/faces/0/face_id",
        "/source_payload/faces/0/revision_id",
    ] {
        let mut input = inputs["accepted"].clone();
        *input.pointer_mut(pointer).unwrap() = json!("unresolved");
        assert_eq!(
            lowerer.lower_current(&input).unwrap_err().code,
            "invalid_current_identity"
        );
    }
    let mut input = inputs["accepted"].clone();
    input["source_payload"]["token_links"] = json!([{"token_face_id":"unknown"}]);
    assert_eq!(
        lowerer.lower_current(&input).unwrap_err().code,
        "unsupported_token_selection"
    );
    for pointer in [
        "/sidecar/0/source_proof",
        "/sidecar/0/source_input_hash",
        "/sidecar/0/input_hash",
        "/sidecar/0/source_use/occurrence/owner/revision_id",
        "/sidecar/0/path/1",
        "/tree/cards/BP01-009/abilities/0/sources/0/occurrence/owner/revision_id",
        "/tree/cards/BP01-009/abilities/0/sources/0/occurrence/line_ordinal",
    ] {
        let mut mismatched = inputs["accepted"].clone();
        *mismatched.pointer_mut(pointer).unwrap() = json!("mismatch");
        assert_eq!(
            lowerer.lower_current(&mismatched).unwrap_err().code,
            "invalid_current_source"
        );
    }
    let mut other_hash = inputs["accepted"].clone();
    other_hash["sidecar"][0]["source_input_hash"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert_eq!(
        lowerer.lower_current(&other_hash).unwrap_err().code,
        "invalid_current_source"
    );
    let mut zero_line = inputs["accepted"].clone();
    zero_line["tree"]["cards"]["BP01-009"]["abilities"][0]["line"] = json!(0_u32);
    for use_path in [
        "/tree/cards/BP01-009/abilities/0/sources/0/occurrence",
        "/sidecar/0/source_use/occurrence",
    ] {
        zero_line
            .pointer_mut(use_path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("line_ordinal");
    }
    assert_eq!(
        lowerer.lower_current(&zero_line).unwrap_err().code,
        "invalid_current_source"
    );
    let mut missing = inputs["accepted"].clone();
    missing["sidecar"] = json!([]);
    assert_eq!(
        lowerer.lower_current(&missing).unwrap_err().code,
        "invalid_current_source"
    );
    let mut duplicate = inputs["accepted"].clone();
    duplicate["sidecar"]
        .as_array_mut()
        .unwrap()
        .push(inputs["accepted"]["sidecar"][0].clone());
    assert_eq!(
        lowerer.lower_current(&duplicate).unwrap_err().code,
        "invalid_current_source"
    );
}

#[test]
fn expanded_dynamic_scope_rejects_a_parameter_that_authored_accepts() {
    let expanded = validator("expanded").unwrap();
    let mut tree = expanded::tree().clone();
    assert!(expanded.is_valid(&tree));
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["define"]["X"]["minus"][1] =
        json!({"param":"offset"});
    assert!(validator("authored").unwrap().is_valid(&tree));
    assert!(!expanded.is_valid(&tree));
}

#[test]
fn bp08_candidate_matches_the_handwritten_card_program() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let mut actual = cards["BP08-038"].card().clone();
    let expected = bp08().node;
    let [ability] = actual.abilities.as_mut_slice() else {
        panic!("one fanfare");
    };
    assert_eq!(ability.source.line, 2);
    assert_eq!(ability.source.scope, "ability_0");
    assert_eq!(ability.source.author_path, "/cards/BP08-038/abilities/0");
    ability.source = source("BP08-038");
    normalize_printed_origin(&mut ability.node.source);
    let body = ability.node.body.as_mut().unwrap();
    let [step] = body.steps.as_mut_slice() else {
        panic!("one draw");
    };
    assert_eq!(
        step.source.author_path,
        "/cards/BP08-038/abilities/0/fanfare/do/0"
    );
    step.source = source("BP08-038");
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("draw instruction");
    };
    normalize_printed_origin(&mut instruction.source);
    assert_eq!(actual, expected);
}

fn normalize_printed_origin(source: &mut AbilitySource) {
    let AbilityOrigin::Keyword { origin, .. } = &mut source.origin else {
        panic!("fanfare origin");
    };
    let AbilityOrigin::Printed { face, line } = origin.as_mut() else {
        panic!("printed origin");
    };
    *face = FaceId("base".to_owned());
    *line = Nat(1);
}

#[test]
fn unsupported_step_rejects_the_entire_candidate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"banish":"this"}));
    let failure = ExpandedLowerer::new().unwrap().lower(&tree).unwrap_err();
    assert_eq!(failure.code, "unsupported_construct");
    assert_eq!(failure.path, "/cards/BP08-038/abilities/0");
}

#[test]
fn bp01_118_is_one_uniform_instruction_for_all_leaders() {
    let cards = ExpandedLowerer::new()
        .unwrap()
        .lower(expanded::tree())
        .unwrap();
    let [ability] = cards["BP01-118"].card().abilities.as_slice() else {
        panic!("one fanfare");
    };
    let [step] = ability.node.body.as_ref().unwrap().steps.as_slice() else {
        panic!("one damage instruction");
    };
    let Node::Instruction { instruction } = &step.node else {
        panic!("damage instruction");
    };
    assert_eq!(instruction.id, InstructionId(1));
    assert_eq!(instruction.receipt, ReceiptId(1));
    assert_eq!(
        instruction.action,
        Action::Damage {
            recipients: RecipientSet::Leaders {
                players: PlayerSet {
                    relation: PlayerRelation::All,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None
                    },
                    text_quantifier: TextQuantifier::All,
                }
            },
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(1))
            },
            flags: DamageFlags::Ability,
            source: ObjectView::SourceInformation {},
            batch: BatchPolicy::Instruction {},
        }
    );
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
    let cards = ExpandedLowerer::new().unwrap().lower(&tree).unwrap();
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
        ExpandedLowerer::new()
            .unwrap()
            .lower(&tree)
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
}

#[test]
fn search_groups_partition_and_successful_ex_discount_are_scoped() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"] = json!([
        {"search":{"groups":[
            {"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1_u64},
            {"sel":{"who":"self","zone":"deck","kind":"spell"},"upto":1_u64}
        ],"distinct_by":"name","to":{"order":"printed","partition":[{"zone":"hand","n":1_u64},{"zone":"cemetery","n":1_u64}]}}},
        {"search":{"sel":{"who":"self","zone":"deck","name":"コスモスファング"},"n":1_u64,"to":"ex","result":"searched"}},
        {"apply":{"on":{"follow":"searched"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"delta":-2_i64}},"until":"turn"}},
        {"look":{"top":{"who":"self","n":3_u64},"bind":"seen"}},
        {"move":{"ref":{"in":"seen"},"to":"ex","result":"moved"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}}
    ]);
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let body = cards["BP03-002"].card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap();
    let Node::Instruction { instruction: first } = &body.steps[0].node else {
        panic!("search");
    };
    let Action::Search {
        groups: Some(groups),
        destination: Some(SearchDestination::Partition { destinations }),
        ..
    } = &first.action
    else {
        panic!("partition");
    };
    assert_eq!(
        groups.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(2))
        }
    );
    assert_eq!(destinations.len(), 2);
    let Node::Instruction {
        instruction: searched,
    } = &body.steps[1].node
    else {
        panic!("search");
    };
    let Node::Instruction {
        instruction: installed,
    } = &body.steps[2].node
    else {
        panic!("installation");
    };
    assert!(
        matches!(&installed.action, Action::InstallEffect { subjects: InstallationSubjects::Objects { objects: Selector::MoveAfter { receipt: MovementReceipt::Search { receipt }, .. }, .. }, .. } if *receipt == ReceiptReference::new(searched.receipt))
    );
    let Node::Instruction {
        instruction: ordered,
    } = &body.steps[5].node
    else {
        panic!("order");
    };
    assert!(
        matches!(&ordered.action, Action::Order { cards: Selector::Difference { except, .. }, .. } if matches!(except.as_ref(), Selector::ReceiptMembers { receipt: ObjectReceipt::Movement { receipt: MovementReceipt::Move { .. } }, projection: MemberProjection::Before }))
    );
    tree["cards"]["BP03-002"]["abilities"][0]["fanfare"]["do"][1]["search"]["to"] = json!("hand");
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn activated_payment_order_and_trigger_limit_remain_separate() {
    let mut tree = expanded::tree().clone();
    tree["cards"]["BP08-067"]["abilities"][0]["auto"]["per_turn"] = json!(1_u64);
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["cost"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"self_to_cemetery":true}));
    let lowerer = ExpandedLowerer::new().unwrap();
    let cards = lowerer.lower(&tree).unwrap();
    let ability = &cards["BP01-041"].card().abilities[0].node;
    assert_eq!(ability.kind, AbilityKind::Activated);
    assert!(matches!(
        ability.source.origin,
        AbilityOrigin::Printed { .. }
    ));
    assert!(ability.trigger.is_none());
    let CostSpec::Ordered { steps } = &ability.play.as_ref().unwrap().costs else {
        panic!("ordered payment");
    };
    assert!(matches!(
        steps[0].instruction.action,
        Action::Resource {
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(3)),
            ..
        }
    ));
    assert!(matches!(
        steps[1].instruction.action,
        Action::Posture {
            posture: Posture::Acted,
            ..
        }
    ));
    assert!(matches!(&steps[2].instruction.action, Action::Move {
        objects: Selector::Objects { objects },
        movement: MoveSpec { to: MoveDestination::Owners { zone: OwnerZone::Cemetery }, position: None, order: None, initial_posture: None, entry_effects },
        reason: MoveReason::Put,
    } if objects == &[ObjectOperand::SourceHolder {}] && entry_effects.is_empty()));
    assert_eq!(
        cards["BP08-067"].card().abilities[0]
            .node
            .trigger
            .as_ref()
            .unwrap()
            .limit,
        Some(LimitSpec::once_per_turn())
    );
    tree["cards"]["BP01-041"]["abilities"][0]["activate"]["per_turn"] = json!(2_u64);
    assert_eq!(
        lowerer.lower(&tree).unwrap()["BP01-041"].card().abilities[0]
            .node
            .play
            .as_ref()
            .unwrap()
            .limit,
        Some(LimitSpec::activated_per_turn(Nat(2)))
    );
}

#[test]
fn printed_spell_cost_targets_distribution_and_choice_preserve_play_scopes() {
    let lowerer = ExpandedLowerer::new().unwrap();
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
    assert_choice_extensions(&lowerer, tree, &costs, body);
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
    let lowerer = ExpandedLowerer::new().unwrap();
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
    tree["cards"]["synthetic"]["abilities"][0]["play"]["optional_cost"] = json!({"pp":1});
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().path,
        "/cards/synthetic/abilities/0"
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
