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
use sve_battle_contract::{Answer, ClientMessage, Command, HostMessage, Seat};
use sve_battle_host::load::{LoadError, PreparedBattle};
use sve_battle_host::wire::{BattleInput, BattleUpdate, RequestId, Submit};
use sve_battle_host::{Delivery, LocalSession};

fn setup(card: &str) -> String {
    let deck = json!({"construction":{"kind":"class","value":"ニュートラル"},
        "main":vec![card; 8],"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p1","seed":"load","control":["player","policy"],
        "max_submissions":10_u8})
    .to_string()
}

fn published() -> (Value, String) {
    serde_json::from_str(include_str!("fixtures/published-battle.json")).unwrap()
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
    assert_eq!(prepared.open("{}").unwrap_err().code(), "format");
    for unavailable in ["test-t-unused", "t-anchor"] {
        assert_eq!(
            prepared.open(&setup(unavailable)).unwrap_err().code(),
            "match"
        );
    }
}

#[test]
fn jp_battle_binds_its_programs_in_a_two_region_package() {
    let (mut document, programs) = published();
    let mut programs: Value = serde_json::from_str(&programs).unwrap();
    programs["entries"].as_array_mut().unwrap().push(json!({
        "id":"english-program","dsl_version":"1.0",
        "ast":{"card_id":"english-card","scope":"shared","abilities":[]}
    }));
    programs["printings"]["english-card"] =
        json!({"representative":"english-printing","tokens":[]});
    let programs = programs.to_string();
    document["identity"]["programs"]["sha256"] = json!(fixture::hex(programs.as_bytes()));

    let prepared = PreparedBattle::new(&document.to_string(), &programs).unwrap();
    assert!(prepared.open(&setup("test-t-anchor")).is_ok());
    assert_eq!(
        prepared
            .open(&setup("english-printing"))
            .unwrap_err()
            .code(),
        "match"
    );
}

#[test]
fn rejected_opponent_decks_share_one_status_with_the_other_seat() {
    let (mut document, programs) = published();
    let mut programs: Value = serde_json::from_str(&programs).unwrap();
    let card = document
        .pointer("/printings/1/card_id")
        .unwrap()
        .as_str()
        .unwrap();
    let face = document
        .pointer("/printings/1/faces/0/face_id")
        .unwrap()
        .as_str()
        .unwrap();
    programs
        .pointer_mut("/entries")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"rejected","dsl_version":"1.0",
            "ast":{"card_id":card,"scope":"shared","faces":{(face):{"abilities":"invalid"}}}
        }));
    programs
        .pointer_mut("/printings")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(
            card.to_owned(),
            json!({"representative":"test-t-unused","tokens":[]}),
        );
    let programs = programs.to_string();
    *document.pointer_mut("/identity/programs/sha256").unwrap() =
        json!(fixture::hex(programs.as_bytes()));
    let prepared = PreparedBattle::new(&document.to_string(), &programs).unwrap();

    let mut setup: Value = serde_json::from_str(&setup("test-t-anchor")).unwrap();
    *setup.pointer_mut("/control").unwrap() = json!(["player", "player"]);
    let legal = setup.pointer("/decks/0").unwrap().clone();
    prepared.open(&setup.to_string()).unwrap();
    let with_main = |main| {
        let mut deck = legal.clone();
        *deck.pointer_mut("/main").unwrap() = main;
        deck
    };
    for (rejected, detail) in [
        (Value::Null, "deck_format"),
        (with_main(json!(["unavailable"])), "deck_printing"),
        (with_main(json!(["test-t-unused"])), "selection"),
        (with_main(json!([])), "deck_admission"),
    ] {
        for (submitter, other, decks) in [
            (Seat::P1, Seat::P2, [&rejected, &legal]),
            (Seat::P2, Seat::P1, [&legal, &rejected]),
        ] {
            *setup.pointer_mut("/decks").unwrap() = json!(decks);
            let refusal = prepared.open(&setup.to_string()).unwrap_err();
            assert_eq!(refusal.code_for(submitter), detail);
            assert_eq!(refusal.code_for(other), "match");
            assert_eq!(refusal.code(), "match");
        }
    }
    let duplicate = legal.to_string().replacen('{', r#"{"main":[],"#, 1);
    *setup.pointer_mut("/decks").unwrap() = json!([legal, "duplicate-deck"]);
    let setup = setup.to_string().replace(r#""duplicate-deck""#, &duplicate);
    let refusal = prepared.open(&setup).unwrap_err();
    assert_eq!(refusal.code_for(Seat::P1), "match");
    assert_eq!(refusal.code_for(Seat::P2), "deck_format");
}

fn published_start_amulet() -> PreparedBattle {
    let (mut document, programs) = published();
    let (anchor, unused) = document
        .pointer("/snapshot")
        .unwrap()
        .as_str()
        .unwrap()
        .split_once('\n')
        .unwrap();
    let mut anchor: Value = serde_json::from_str(anchor).unwrap();
    *anchor.pointer_mut("/faces/0/card_type").unwrap() = json!("アミュレット");
    *anchor.pointer_mut("/faces/0/power").unwrap() = json!("-");
    *anchor.pointer_mut("/faces/0/hp").unwrap() = json!("-");
    *anchor.pointer_mut("/faces/0/text").unwrap() = json!("Start amulet.");
    let snapshot = format!("{anchor}\n{unused}");
    let digest = fixture::hex(snapshot.as_bytes());
    *document.pointer_mut("/snapshot").unwrap() = json!(snapshot);
    for path in [
        "/references/snapshot_sha256",
        "/engine_identity/Resolved/snapshot_sha256",
    ] {
        *document.pointer_mut(path).unwrap() = json!(digest);
    }
    let mut rules: Value =
        serde_json::from_str(document.pointer("/rules").unwrap().as_str().unwrap()).unwrap();
    *rules.pointer_mut("/input/snapshot_sha256").unwrap() = json!(digest);
    *rules.pointer_mut("/titles/0/capabilities").unwrap() =
        json!([{"kind":"opening_start_amulet"}]);
    let rules = rules.to_string();
    *document.pointer_mut("/identity/rules/sha256").unwrap() =
        json!(fixture::hex(rules.as_bytes()));
    *document.pointer_mut("/rules").unwrap() = json!(rules);

    let mut programs: Value = serde_json::from_str(&programs).unwrap();
    let face = document
        .pointer("/printings/0/faces/0/face_id")
        .unwrap()
        .as_str()
        .unwrap();
    let abilities = format!("/entries/0/ast/faces/{face}/abilities");
    *programs.pointer_mut(&abilities).unwrap() = json!([{
        "line":1_u8,"keywords":["start_amulet"],"ports":{"exports":{},"imports":{}}
    }]);
    let programs = programs.to_string();
    *document.pointer_mut("/identity/programs/sha256").unwrap() =
        json!(fixture::hex(programs.as_bytes()));
    PreparedBattle::new(&document.to_string(), &programs).unwrap()
}

#[test]
fn reserving_a_start_amulet_leaves_the_submitting_deck_too_short() {
    let prepared = published_start_amulet();
    let mut setup: Value = serde_json::from_str(&setup("test-t-anchor")).unwrap();
    *setup.pointer_mut("/decks/1/main").unwrap() = json!(vec!["test-t-anchor"; 4]);
    prepared.open(&setup.to_string()).unwrap();
    for (title, main, expected) in [
        (
            "synthetic_empty",
            vec!["test-t-anchor"; 4],
            "deck_admission",
        ),
        ("unregistered", vec![], "match"),
    ] {
        *setup.pointer_mut("/decks/1").unwrap() = json!({
            "construction":{"kind":"title","value":title},
            "main":main,"evolve":[]
        });
        let refused = prepared.open(&setup.to_string()).unwrap_err();
        assert_eq!(refused.code_for(Seat::P1), "match");
        assert_eq!(refused.code_for(Seat::P2), expected);
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
