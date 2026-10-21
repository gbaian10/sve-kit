use super::*;
use crate::ir::lowering::{ExpandedLowerer, validator};
use serde_json::json;

#[path = "../../../tests/support/expanded.rs"]
mod expanded;

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
        .push(json!({"destroy":"this"}));
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
