use super::*;
use sve_battle_contract::{
    presentation::NoticeKind,
    question::{AutoAnswer, MainIntent},
};
use sve_engine::ir::lowering::ExpandedLowerer;
use sve_engine::ir::{CardProgram, WellFormedCardProgram};

fn local_look(hidden: &str) -> LocalSession {
    let snapshot = [card("t-small", "1", "1"), card("t-large", "3", "3")].join("\n");
    let registry = json!({"format":1_i32,"kind":"keyword_registry","keywords":{}}).to_string();
    let effects = json!({"format":1_i32,"kind":"effect_set","cards":{
        "t-small":{"status":"complete","review":"synthetic","abilities":[]},
        "t-large":{"status":"complete","review":"synthetic","abilities":[]}
    }});
    let catalog = Catalog::from_documents_with_rules(
        &snapshot,
        &registry,
        &[("synthetic.yaml".into(), effects.to_string())],
        &fixture::rules(&snapshot, "t-small"),
        &EngineIdentityInput::LegacyJp,
    )
    .unwrap();
    let references = BattleReferences::new(&catalog, fixture::references(&snapshot)).unwrap();
    let setup = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main",
        "elapsed_turns":{"P1":3_i32,"P2":2_i32}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_i32},"pp":{"current":2_i32,"max":2_i32},
            "construction":"class","zones":{"field":[{"id":"source","card":"t-small"}],
                "deck":[{"id":"hidden","card":hidden},{"id":"tail","card":"t-small"}]}},
        "P2":{"leader":{"class":"ニュートラル","life":20_i32},"pp":{"current":2_i32,"max":2_i32},
            "construction":"class","zones":{"deck":[{"id":"other","card":"t-small"}]}}
    }});
    let mut game = Game::new(
        Arc::new(catalog),
        &setup,
        &Value::Null,
        &Value::Null,
        "no-stop",
    )
    .unwrap();
    let program = ExpandedLowerer::new()
        .unwrap()
        .lower(&json!({
            "format":1_i32,"kind":"author_effect_set","scope":"shared","cards":{"t-small":{
                "meta":{"dsl":"1.0","source":format!("effect-source-v1:sha256:{}", "3".repeat(64)),
                    "written_by":[{"actor":{"who":"synthetic","model":null},"date":"2026-10-22"}],
                    "reviewed_by":[],"status":"draft","verified_by_exam":[],"rulings":[]},
                "abilities":[{"line":1_i32,"sources":[{"binding_id":"binding:notice",
                    "ability_scope":"ability_0","occurrence":{
                        "owner":{"kind":"face_revision","revision_id":"revision:notice"},
                        "field":"effect","ordinal":null,"source_hash":"0".repeat(64),
                        "line_ordinal":0_i32,"role":"body","segments":[{"start":0_i32,"end":1_i32}]
                    }}],"ports":{"imports":{},"exports":{}},
                    "activate":{"cost":{"pp":1_i32},"do":[
                        {"look":{"top":{"who":"self","n":1_i32},"bind":"seen"}},
                        {"shuffle":{"who":"self"}}]}}]
            }}
        }))
        .unwrap()
        .remove("t-small")
        .unwrap();
    game.load_internal_card("t-small", &program).unwrap();
    let vanilla = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .unwrap();
    game.load_internal_card("t-large", &vanilla).unwrap();
    game.advance_player_input();
    LocalSession::new(
        game,
        references,
        fixture::identity("", ""),
        HOT_SEAT,
        HostLimits::default(),
    )
}

#[expect(
    clippy::panic,
    reason = "The synthetic match must open the activation window."
)]
fn activate(session: &mut LocalSession) -> Vec<Delivery> {
    let mut seats = Seats::default();
    let opening = session.start();
    seats.absorb(&opening);
    let Some((seat, question)) = seats.question() else {
        panic!("no question after synthetic session start: {opening:?}");
    };
    let QuestionShape::ActionWindow { intents } = &question.shape else {
        panic!("a main action is open");
    };
    let activation = intents
        .iter()
        .find_map(|intent| {
            if let MainIntent::Activate { id, .. } = intent {
                Some(id.clone())
            } else {
                None
            }
        })
        .unwrap();
    session.submit(
        seat,
        &answer_with(
            question.id.as_str(),
            Answer::ActionWindow {
                intent: activation,
                target: None,
            },
            "look",
        ),
    )
}

fn update(deliveries: &[Delivery], seat: Seat) -> &BattleUpdate {
    deliveries
        .iter()
        .filter(|delivery| delivery.seat == seat)
        .find_map(|delivery| {
            if let HostMessage::Update(update) = &delivery.message {
                Some(update.as_ref())
            } else {
                None
            }
        })
        .unwrap()
}

#[test]
fn a_look_without_a_question_is_delivered_once_to_its_seat() {
    let mut a = local_look("t-small");
    let mut b = local_look("t-large");
    let (a_out, b_out) = (activate(&mut a), activate(&mut b));
    assert!(applied(&a_out) && applied(&b_out));
    assert_eq!(update(&a_out, Seat::P2), update(&b_out, Seat::P2));
    assert_eq!(update(&a_out, Seat::P2).notices, []);
    for (session, out, printing) in [
        (&mut a, &a_out, "test-t-small"),
        (&mut b, &b_out, "test-t-large"),
    ] {
        let seen = update(out, Seat::P1);
        assert!(
            matches!(&seen.input, BattleInput::Question { question } if question.usage == QuestionUse::MainAction)
        );
        let [notice] = seen.notices.as_slice() else {
            panic!("one Look notice")
        };
        assert_eq!(notice.kind, NoticeKind::Look);
        let [card] = notice.cards.as_slice() else {
            panic!("one looked card")
        };
        assert_eq!(card.card.printing_id, printing);
        let baseline = session.baseline(Seat::P1);
        assert_eq!(update(&baseline, Seat::P1).notices, []);
        assert_eq!(update(&baseline, Seat::P1).update_seq, seen.update_seq);
    }
}

#[test]
fn a_publicly_empty_quick_window_requires_an_explicit_pass() {
    let mut session = local_look("t-small");
    let mut seats = Seats::default();
    seats.absorb(&session.start());
    let (turn_player, main) = seats.question().unwrap();
    assert_eq!(main.auto_answer, AutoAnswer::Unknown);
    seats.absorb(&session.submit(turn_player, &answer(&main, "end-main")));
    let (actor, quick) = seats.question().unwrap();
    assert_eq!(actor, Seat::P2);
    assert_eq!(quick.usage, QuestionUse::Quick);
    let AutoAnswer::Unique {
        answer: pass,
        public: true,
    } = &quick.auto_answer
    else {
        panic!("all public candidate zones are empty");
    };
    assert_eq!(pass, &Answer::Quick { option: None });
    assert!(applied(&session.submit(
        actor,
        &answer_with(quick.id.as_str(), pass.clone(), "pass")
    )));
}
