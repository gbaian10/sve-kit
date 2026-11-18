//! Shared synthetic inputs and a passive wire player for the host tests.

#![allow(
    dead_code,
    clippy::panic,
    reason = "Each test binary uses a different part; unexpected fixtures fail the test."
)]

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sve_battle_contract::question::{DiceChoice, MainIntent, Question, QuestionShape};
use sve_battle_contract::{Answer, CardRef, ContentRef, Identity, Nat, OptionId, Region};
use sve_engine::battle::{PrintingFaceIdentity, ReferenceInput};

pub(crate) fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// The smallest rule resources the schema and evidence closure accept for a snapshot; the one
/// evidence entry is anchored on `anchor`.
pub(crate) fn rules(snapshot: &str, anchor: &str) -> String {
    let source = format!("src:v1:{}", "a".repeat(64));
    json!({"format":1_u8,"kind":"engine_rules",
        "scope":{"region":"jp","rules_version":"synthetic-1","rules_source_version_id":source},
        "input":{"kind":"legacy-jp","snapshot_sha256":hex(snapshot.as_bytes())},
        "evidence":[{"id":"synthetic","source_version_id":source,"rule_refs":["1.2.3"],
            "checked_on":"2026-10-03"}],
        "titles":[{"title_code":"synthetic_empty","capabilities":[],"evidence":["synthetic"],
            "anchor":{"kind":"legacy-face","region":"jp","card_no":anchor,"face_ordinal":0_u8}}],
        "resources":[]})
    .to_string()
}

/// The verified public manifest the synthetic tests pin their games to.
pub(crate) const MANIFEST: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";

/// The content identity of a game loaded from these programs and rules.
pub(crate) fn identity(selection: &str, rules: &str) -> Identity {
    let content = |id: &str, sha256: String| ContentRef {
        id: id.into(),
        sha256,
    };
    Identity {
        snapshot: content("snapshot-test", MANIFEST.into()),
        programs: content("programs-test", hex(selection.as_bytes())),
        engine: content("engine-test", "2".repeat(64)),
        rules: content("rules-test", hex(rules.as_bytes())),
        region: Region::Jp,
    }
}

/// One test identity per native face of every snapshot record.
///
/// The public test batch carries no permanent printing ids; production references come with
/// the native bytes from carddb's battle input reader. Binding test ids keeps the host path
/// identical without claiming those ids are published.
pub(crate) fn references(snapshot: &str) -> ReferenceInput {
    let faces = snapshot
        .lines()
        .filter(|line| !line.is_empty())
        .flat_map(|line| {
            let record: Value = serde_json::from_str(line).unwrap();
            let number = record
                .get("number")
                .and_then(Value::as_str)
                .unwrap()
                .to_owned();
            let count = record.get("faces").and_then(Value::as_array).unwrap().len();
            (0..count).map(move |face| {
                let ordinal = u64::try_from(face).unwrap();
                PrintingFaceIdentity {
                    card_no: number.clone(),
                    native_face: ordinal,
                    card: CardRef {
                        printing_id: format!("test-{number}"),
                        face: Nat::try_from(ordinal).unwrap(),
                    },
                }
            })
        })
        .collect();
    ReferenceInput {
        region: "jp".into(),
        snapshot_sha256: hex(snapshot.as_bytes()),
        faces,
        wording: None,
        rules: None,
    }
}

fn first(count: Nat, ids: impl IntoIterator<Item = OptionId>) -> Vec<OptionId> {
    ids.into_iter()
        .take(usize::try_from(count.get()).unwrap())
        .collect()
}

/// An answer of the question's own kind naming an option it never offered, or of another kind
/// where the question offers no options.
pub(crate) fn wrong(question: &Question) -> Answer {
    let absent = OptionId::new("not-offered".into());
    match &question.shape {
        QuestionShape::Choose { .. } => Answer::Choose {
            chosen: vec![absent],
        },
        QuestionShape::ActionWindow { .. } => Answer::ActionWindow {
            intent: absent,
            target: None,
        },
        QuestionShape::Quick { .. } => Answer::Quick {
            option: Some(absent),
        },
        QuestionShape::Select { .. } => Answer::Select {
            selected: vec![absent],
            groups: None,
            roles: vec![],
        },
        QuestionShape::Mulligan { .. } => Answer::Mulligan {
            redraw: true,
            bottom_order: vec![absent],
        },
        QuestionShape::Integer { .. } => Answer::Confirm { accept: true },
        QuestionShape::ResourcePayment { .. }
        | QuestionShape::EvolutionPayment { .. }
        | QuestionShape::Confirm { .. }
        | QuestionShape::DiceReroll { .. }
        | QuestionShape::Order { .. }
        | QuestionShape::DriveTrigger { .. }
        | QuestionShape::Partition { .. }
        | QuestionShape::Distribute { .. } => Answer::Integer { value: Nat::ZERO },
    }
}

/// A player who never acts: keeps the hand, ends every main phase, passes every window and
/// gives the smallest answer anything else allows. A match between such players ends by deck-out.
pub(crate) fn passive(question: &Question) -> Answer {
    match &question.shape {
        QuestionShape::Choose {
            options, bounds, ..
        } => Answer::Choose {
            chosen: first(
                bounds.min.max(Nat::try_from(1_u64).unwrap()),
                options.iter().map(|option| option.id.clone()),
            ),
        },
        QuestionShape::Mulligan { .. } => Answer::Mulligan {
            redraw: false,
            bottom_order: vec![],
        },
        QuestionShape::ActionWindow { intents } => Answer::ActionWindow {
            intent: intents
                .iter()
                .find_map(|intent| match intent {
                    MainIntent::EndPhase { id } => Some(id.clone()),
                    MainIntent::Play { .. }
                    | MainIntent::Activate { .. }
                    | MainIntent::Evolve { .. }
                    | MainIntent::Attack { .. } => None,
                })
                .unwrap(),
            target: None,
        },
        QuestionShape::Quick { .. } => Answer::Quick { option: None },
        QuestionShape::Select { selection } => Answer::Select {
            selected: first(
                selection.bounds.min,
                selection
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone()),
            ),
            groups: None,
            roles: vec![],
        },
        QuestionShape::ResourcePayment { materials, .. } => Answer::ResourcePayment {
            materials: first(
                materials.bounds.min,
                materials
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone()),
            ),
            use_ep: false,
        },
        QuestionShape::EvolutionPayment { payments } => {
            let payment = payments.first().unwrap();
            Answer::EvolutionPayment {
                pp: payment.pp,
                use_ep: payment.use_ep,
                super_evolution: payment.super_evolution,
            }
        }
        QuestionShape::Confirm { allowed, .. } => Answer::Confirm {
            accept: !allowed.contains(&false),
        },
        QuestionShape::Integer { bounds, .. } => Answer::Integer { value: bounds.min },
        QuestionShape::DiceReroll { allowed, .. } => Answer::DiceReroll {
            choice: if allowed.contains(&DiceChoice::Keep) {
                DiceChoice::Keep
            } else {
                DiceChoice::Reroll
            },
        },
        QuestionShape::Order { items, .. } => Answer::Order {
            order: items.iter().map(|item| item.id.clone()).collect(),
        },
        QuestionShape::DriveTrigger { allowed, .. } if allowed.contains(&false) => {
            Answer::DriveTrigger {
                accept: false,
                target: None,
            }
        }
        shape @ (QuestionShape::DriveTrigger { .. }
        | QuestionShape::Partition { .. }
        | QuestionShape::Distribute { .. }) => {
            panic!("a passive player never meets {:?}", shape.kind())
        }
    }
}
