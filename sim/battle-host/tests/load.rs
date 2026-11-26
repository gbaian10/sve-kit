//! Session loading failures on the smallest inputs that reach each stage.

#![expect(
    clippy::unwrap_used,
    clippy::unreachable,
    reason = "A malformed synthetic fixture invalidates these tests."
)]

#[path = "support/fixture.rs"]
mod fixture;

use serde_json::{Value, json};
use sve_battle_contract::question::{MainIntent, QuestionShape};
use sve_battle_contract::{Answer, ClientMessage, Command, HostMessage};
use sve_battle_host::load::{LoadError, PreparedBattle};
use sve_battle_host::wire::{BattleInput, BattleUpdate, RequestId, Submit};
use sve_battle_host::{Delivery, LocalSession};

const KEYWORDS: &str = r#"{"format":1,"kind":"keyword_registry","keywords":{}}"#;

/// The anchor of the rule evidence and a printing no deck uses.
fn snapshot() -> String {
    ["t-anchor", "t-unused"]
        .map(|number| {
            json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル",
                "card_type":"フォロワー","cost":"1","power":"1","hp":"1","traits":[],"text":(number == "t-anchor").then_some("Draw a card twice."),
                "sections":[],"title":"synthetic"}]})
            .to_string()
        })
        .join("\n")
}

fn setup(card: &str) -> String {
    let deck = json!({"construction":{"kind":"class","value":"ニュートラル"},
        "main":vec![card; 8],"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p1","seed":"load","control":["player","policy"],
        "max_submissions":10_u8})
    .to_string()
}

fn published() -> (Value, String) {
    let native = snapshot();
    let card = format!("c:{}", "a".repeat(32));
    let face = format!("f:{}", "b".repeat(32));
    let unused = format!("f:{}", "c".repeat(32));
    let mut rules: Value = serde_json::from_str(&fixture::rules(&native, "t-anchor")).unwrap();
    *rules.pointer_mut("/input/kind").unwrap() = json!("resolved");
    *rules.pointer_mut("/titles/0/anchor").unwrap() =
        json!({"kind":"face-id","region":"jp","face_id":face});
    let rules = rules.to_string();
    let mut refs = fixture::references(&native);
    refs.wording = Some(
        serde_json::from_value(json!({
            "manifest_sha256": format!("sha256:{}", fixture::MANIFEST),
            "faces": [
                {"card_no":"t-anchor","native_face":0_u8,"face_id":face,
                    "revisions":[{"revision_id":"revision","fields":[{"kind":"effect"}]}]},
                {"card_no":"t-unused","native_face":0_u8,"face_id":unused,
                    "revisions":[{"revision_id":"unused","fields":[]}]}
            ]
        }))
        .unwrap(),
    );
    let draw = json!({"line":1_u8,"fanfare":{"do":[{"draw":{"n":1_u8}}]},
        "ports":{"exports":{},"imports":{}}});
    let programs = json!({
        "format_version":"4.2.0",
        "entries": [{"id":"synthetic","dsl_version":"1.0",
            "ast":{"card_id":card,"scope":"shared","faces":{(face.clone()):{"abilities":[draw,draw]}}}}],
        "printings": {(card.clone()):{"representative":"test-t-anchor","tokens":[]}}
    })
    .to_string();
    let identity = fixture::identity(&programs, &rules);
    let document = json!({
        "snapshot":native,"keywords":KEYWORDS,"references":refs,"rules":rules,"identity":identity,
        "engine_identity":{"Resolved":{"region":"jp","snapshot_sha256":refs.snapshot_sha256,
            "faces":[{"card_no":"t-anchor","face_ordinal":0_u8,"face_id":face,
                "rules_name_id":"synthetic-name","title_code":"synthetic_empty"}]}},
        "printings":[
            {"printing_id":"test-t-anchor","card_no":"t-anchor","card_id":card,
                "faces":[{"face_id":face,"revision_id":"revision","cost":1_u8,"rules_name":"t-anchor"}]},
            {"printing_id":"test-t-unused","card_no":"t-unused","card_id":format!("c:{}", "c".repeat(32)),
                "faces":[{"face_id":unused,"revision_id":"unused","cost":1_u8,"rules_name":"t-unused"}]}
        ],
    });
    (document, programs)
}

#[test]
fn a_published_program_opens_a_deck_by_permanent_printing() {
    let (document, programs) = published();
    let prepared = PreparedBattle::new(&document.to_string(), &programs).unwrap();
    let opening = prepared.open(&setup("test-t-anchor")).unwrap().start();
    assert!(
        opening
            .iter()
            .any(|delivery| matches!(delivery.message, HostMessage::Ready(_)))
    );
    assert_eq!(prepared.open("{}").err(), Some(LoadError::Format));
    for unavailable in ["test-t-unused", "t-anchor"] {
        assert_eq!(
            prepared.open(&setup(unavailable)).err(),
            Some(LoadError::Match)
        );
    }
}

fn update(deliveries: &[Delivery]) -> &BattleUpdate {
    deliveries
        .iter()
        .rev()
        .find_map(|delivery| {
            if let HostMessage::Update(update) = &delivery.message {
                Some(update.as_ref())
            } else {
                None
            }
        })
        .unwrap()
}

fn submit(session: &mut LocalSession, update: &BattleUpdate, answer: Answer) -> Vec<Delivery> {
    let BattleInput::Question { question } = &update.input else {
        unreachable!("the synthetic match must be waiting for its player")
    };
    let submit = ClientMessage::Submit(Submit {
        request_id: RequestId::new(format!("r{}", update.update_seq.get())),
        command: Command::Answer {
            question_id: question.id.clone(),
            answer,
        },
    });
    session.submit_named("p1", &serde_json::to_string(&submit).unwrap())
}

#[test]
fn two_published_abilities_on_one_line_both_resolve() {
    let (document, programs) = published();
    let prepared = PreparedBattle::new(&document.to_string(), &programs).unwrap();
    let mut session = prepared.open(&setup("test-t-anchor")).unwrap();
    let mut output = session.start();
    for _ in 0_u8..3 {
        let current = update(&output);
        let BattleInput::Question { question } = &current.input else {
            unreachable!("the synthetic match must be waiting for its player")
        };
        if let QuestionShape::ActionWindow { intents } = &question.shape {
            let play = intents
                .iter()
                .find_map(|intent| {
                    if let MainIntent::Play { id, .. } = intent {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .unwrap();
            let hand = current.observation.players[0].zones.hand.count.get();
            let after_play = submit(
                &mut session,
                current,
                Answer::ActionWindow {
                    intent: play,
                    target: None,
                },
            );
            let pending = update(&after_play);
            let BattleInput::Question {
                question: trigger_order,
            } = &pending.input
            else {
                unreachable!("both published abilities must be pending")
            };
            assert!(
                matches!(&trigger_order.shape, QuestionShape::Choose { options, .. }
                if options.len() == 2)
            );
            let mut result = submit(&mut session, pending, fixture::passive(trigger_order));
            for _ in 0_u8..3 {
                let resolving = update(&result);
                if resolving.observation.players[0].zones.hand.count.get() == hand + 1 {
                    break;
                }
                let BattleInput::Question {
                    question: next_trigger,
                } = &resolving.input
                else {
                    unreachable!("the second ability must be pending")
                };
                result = submit(&mut session, resolving, fixture::passive(next_trigger));
            }
            let visible = &update(&result).observation.players[0].zones;
            assert_eq!(visible.field.count.get(), 1);
            assert_eq!(visible.hand.count.get(), hand + 1, "one played, two drawn");
            return;
        }
        output = submit(&mut session, current, fixture::passive(question));
    }
    unreachable!("did not reach a playable main phase")
}

#[test]
fn published_inputs_reject_another_attachment_or_incomplete_references() {
    let (document, programs) = published();
    assert_eq!(
        PreparedBattle::new("{}", &programs).err(),
        Some(LoadError::Format)
    );
    let another_attachment = programs.replace("synthetic", "another");
    assert_eq!(
        PreparedBattle::new(&document.to_string(), &another_attachment).err(),
        Some(LoadError::Identity)
    );

    for (pointer, value) in [
        ("/printings/0/faces/0/revision_id", json!("foreign")),
        ("/references/faces", json!([])),
        ("/references/region", json!("en")),
        ("/references/snapshot_sha256", json!("0".repeat(64))),
        (
            "/references/rules/manifest_sha256",
            json!(format!("sha256:{}", "a".repeat(64))),
        ),
        ("/identity/snapshot/sha256", json!("b".repeat(64))),
    ] {
        let mut changed = document.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            PreparedBattle::new(&changed.to_string(), &programs).err(),
            Some(LoadError::References),
            "{pointer}"
        );
    }
    let mut malformed_programs = document;
    *malformed_programs
        .pointer_mut("/identity/programs/sha256")
        .unwrap() = json!(fixture::hex(b"["));
    assert_eq!(
        PreparedBattle::new(&malformed_programs.to_string(), "[").err(),
        Some(LoadError::Selection)
    );
    assert_eq!(
        [
            LoadError::Format,
            LoadError::Selection,
            LoadError::Identity,
            LoadError::References,
            LoadError::Match
        ]
        .map(LoadError::code),
        ["format", "selection", "identity", "references", "match"]
    );
}
