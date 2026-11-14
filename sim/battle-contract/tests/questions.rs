//! Every native question source has a wire use, one answer meaning and a fixture.

#![expect(
    clippy::unwrap_used,
    reason = "A missing or malformed fixture must fail the test."
)]

extern crate alloc;

mod support;

use alloc::collections::BTreeSet;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sve_battle_contract::question::{AutoAnswer, Question, QuestionShape, QuestionUse, ShapeKind};
use sve_battle_contract::{Answer, ClientMessage, Command, HostError, decode, encode, schema};

fn fixtures<T: DeserializeOwned + Serialize>(group: &str) -> Vec<T> {
    support::fixtures(group)
        .into_iter()
        .map(|(_, text)| decode(&text).unwrap())
        .collect()
}

/// Every variant of one definition in the generated schema, by the string at `pointer`.
fn variants(definition: &str, pointer: &str) -> BTreeSet<String> {
    let document = serde_json::to_value(schema()).unwrap();
    let listed = document
        .pointer(&format!("/$defs/{definition}/oneOf"))
        .unwrap()
        .as_array()
        .unwrap();
    listed
        .iter()
        .map(|variant| {
            variant
                .pointer(pointer)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn wire_name<T: Serialize>(value: &T) -> String {
    serde_json::from_str(&encode(value).unwrap()).unwrap()
}

fn tag(value: &Value, pointer: &str) -> String {
    value.pointer(pointer).unwrap().as_str().unwrap().to_owned()
}

fn documents(group: &str) -> Vec<Value> {
    support::fixtures(group)
        .into_iter()
        .map(|(_, text)| serde_json::from_str(&text).unwrap())
        .collect()
}

#[test]
fn every_use_has_exactly_one_coherent_fixture() {
    let mut seen = BTreeSet::new();
    for question in fixtures::<Question>("question") {
        assert!(question.is_coherent(), "{:?}", question.usage);
        assert!(
            seen.insert(wire_name(&question.usage)),
            "{:?}",
            question.usage
        );
    }
    assert_eq!(seen, variants("QuestionUse", "/const"));
}

#[test]
fn every_answer_meaning_is_asked_and_has_a_submission_fixture() {
    let shapes = variants("QuestionShape", "/properties/kind/const");
    // An answer carries the tag of the shape it answers.
    assert_eq!(variants("Answer", "/properties/kind/const"), shapes);

    let asked: BTreeSet<_> = documents("question")
        .iter()
        .map(|question| tag(question, "/shape/kind"))
        .collect();
    assert_eq!(asked, shapes);

    let submissions = documents("client");
    let answered: BTreeSet<_> = submissions
        .iter()
        .filter(|submit| tag(submit, "/command/kind") == "answer")
        .map(|submit| tag(submit, "/command/answer/kind"))
        .collect();
    assert_eq!(answered, shapes);
    assert!(
        submissions
            .iter()
            .any(|submit| tag(submit, "/command/kind") == "concede")
    );
}

#[test]
fn a_shape_and_its_answer_report_the_same_meaning() {
    let questions = fixtures::<Question>("question");
    for ClientMessage::Submit(submit) in fixtures::<ClientMessage>("client") {
        let Command::Answer { answer, .. } = submit.command else {
            continue;
        };
        let kind = answer.kind();
        assert!(
            questions
                .iter()
                .any(|question| question.shape.kind() == kind),
            "{kind:?}"
        );
    }
}

#[test]
fn shapes_that_split_by_purpose_keep_one_answer_meaning() {
    for (first, second) in [
        (QuestionUse::CostDiscard, QuestionUse::EffectDiscard),
        (QuestionUse::Targets, QuestionUse::ResolutionSelection),
        (QuestionUse::Modes, QuestionUse::ResolutionAlternative),
        (QuestionUse::OptionalCost, QuestionUse::MayEffect),
    ] {
        assert_eq!(first.shape(), second.shape());
    }
    assert_eq!(
        QuestionUse::EvolutionPayment.shape(),
        ShapeKind::EvolutionPayment
    );
    assert_eq!(QuestionUse::DriveTrigger.shape(), ShapeKind::DriveTrigger);
}

#[test]
fn a_shape_or_answer_of_another_meaning_is_incoherent() {
    let mut question = fixtures::<Question>("question")
        .into_iter()
        .find(|question| question.usage == QuestionUse::ResolutionPayment)
        .unwrap();
    assert!(question.is_coherent());

    question.auto_answer = AutoAnswer::Unique {
        answer: Answer::Integer {
            value: 1_u32.into(),
        },
        public: true,
    };
    assert!(!question.is_coherent());

    question.auto_answer = AutoAnswer::Unknown;
    assert!(question.is_coherent());
    question.shape = QuestionShape::EvolutionPayment {
        payments: Vec::new(),
    };
    assert!(!question.is_coherent());
}

#[test]
fn the_assistance_slot_only_holds_null() {
    let (_, text) = support::fixtures("question").swap_remove(0);
    let mut question: Value = serde_json::from_str(&text).unwrap();
    question["assistance"] = Value::Bool(false);
    assert_eq!(
        decode::<Question>(&question.to_string()).err(),
        Some(HostError::FORMAT)
    );
}

#[test]
fn occurrences_of_one_source_keep_the_public_details_that_tell_them_apart() {
    let [pending] = documents("question")
        .into_iter()
        .filter(|question| tag(question, "/use") == "pending_order")
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let options = pending
        .pointer("/shape/options")
        .unwrap()
        .as_array()
        .unwrap();
    let detail = |index: usize, pointer: &str| options[index].pointer(pointer).unwrap().clone();
    // Same source, same controller, same kind of event: only the public detail differs.
    for (first, second, pointer) in [
        (0, 1, "/value/event/value"),
        (2, 3, "/value/event/amount"),
        (4, 5, "/value/event/delta"),
        (6, 7, "/value/event/turn"),
    ] {
        assert_eq!(
            detail(first, "/value/source"),
            detail(second, "/value/source")
        );
        assert_eq!(
            detail(first, "/value/event/kind"),
            detail(second, "/value/event/kind")
        );
        assert_ne!(detail(first, pointer), detail(second, pointer));
    }
    let listed: BTreeSet<_> = options
        .iter()
        .map(|option| tag(option, "/value/event/kind"))
        .collect();
    assert_eq!(listed, variants("TriggerEvent", "/properties/kind/const"));
}

#[test]
fn earlier_quota_choices_carry_their_details_but_no_answerable_code() {
    let [quota] = documents("question")
        .into_iter()
        .filter(|question| tag(question, "/use") == "trigger_quota")
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let earlier = quota.pointer("/shape/earlier").unwrap().as_array().unwrap();
    let amounts: Vec<_> = earlier
        .iter()
        .map(|choice| choice.pointer("/value/event/amount").unwrap().clone())
        .collect();
    assert_eq!(amounts, [Value::from(1_u8), Value::from(3_u8)]);
    for choice in earlier {
        assert_eq!(choice.get("id"), None);
    }
}
