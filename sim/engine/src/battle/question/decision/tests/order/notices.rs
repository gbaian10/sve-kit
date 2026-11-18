use super::*;
use sve_battle_contract::{BattleInput, presentation::NoticeKind};

#[test]
fn one_private_order_answer_stays_unknown_and_only_the_looker_sees_the_cards() {
    let refs = &board_fixture::fixture().unwrap().references;
    let id = QuestionId::new("private-order".into());
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let mut a = make(actor, "bottom", &["unit-spell"], "unit-spell", 1);
        let mut b = make(actor, "bottom", &["unit-amulet"], "unit-spell", 1);
        let (a_wait, b_wait) = (
            a.battle_input(other, &id, refs).unwrap(),
            b.battle_input(other, &id, refs).unwrap(),
        );
        assert_eq!(a_wait.observation(), b_wait.observation());
        assert_eq!(a_wait.input(), b_wait.input());
        assert_eq!(a.take_battle_notices(other, refs).unwrap(), []);
        assert_eq!(b.take_battle_notices(other, refs).unwrap(), []);
        for (game, number) in [(&mut a, "unit-spell"), (&mut b, "unit-amulet")] {
            let input = game.battle_input(actor, &id, refs).unwrap();
            let BattleInput::Question { question } = input.input() else {
                panic!("the singleton private ordering stop stays open");
            };
            assert!(
                matches!(&question.shape, wire::QuestionShape::Order { items, .. } if items.len() == 1)
            );
            assert_eq!(question.auto_answer, wire::AutoAnswer::Unknown);
            let notices = game.take_battle_notices(actor, refs).unwrap();
            let [notice] = notices.as_slice() else {
                panic!("one Look notice")
            };
            assert_eq!(notice.kind, NoticeKind::Look);
            let [card] = notice.cards.as_slice() else {
                panic!("one looked face")
            };
            let expected = refs
                .resolve(
                    &board_fixture::fixture().unwrap().catalog,
                    &player::observation::CardFace {
                        card: number.into(),
                        face: 0,
                    },
                )
                .unwrap();
            assert_eq!(card.card, expected);
            assert_eq!(card.owner, seat(actor));
            assert_eq!(card.position, Some(sve_battle_contract::Nat::ZERO));
        }
    }
}

#[test]
fn look_survives_a_shuffle_after_an_immediately_answered_public_stop() {
    let refs = &board_fixture::fixture().unwrap().references;
    let id = QuestionId::new("fixed-number".into());
    let body = program(json!({"cost":{"pp":1_i32},"do":[
        {"look":{"top":{"who":"self","n":1_i32},"bind":"seen"}},
        {"declare":{"kind":"number","bind":"one","min":1_i32,"max":1_i32}},
        {"shuffle":{"who":"self"}}
    ]}));
    let actor = Viewer::P1;
    let mut game = activate(actor, &deck(), "unit-amulet", &body);
    let native = question(&game, actor).unwrap();
    let (_, _, projection) = project_game(&game, actor, &id).unwrap();
    let wire::AutoAnswer::Unique {
        answer,
        public: true,
    } = &projection.question().auto_answer
    else {
        panic!(
            "literal singleton in public resolution: {:?}",
            projection.question().auto_answer
        );
    };
    assert_eq!(
        *answer,
        Answer::Integer {
            value: 1_u32.into()
        }
    );
    submit_projected(&mut game, &native, &projection, answer);
    let following = question(&game, actor).unwrap();
    assert!(matches!(
        following.shape,
        player::QuestionShape::ActionWindow(_)
    ));
    let notices = game.take_battle_notices(actor, refs).unwrap();
    assert!(
        matches!(notices.as_slice(), [notice] if notice.kind == NoticeKind::Look && notice.cards.len() == 1)
    );
    assert_eq!(game.take_battle_notices(actor, refs).unwrap(), []);
    assert_eq!(game.take_battle_notices(Viewer::P2, refs).unwrap(), []);
}

#[test]
fn a_literal_result_inside_a_branch_stays_unknown() {
    let body = program(json!({"cost":{"pp":1_i32},"do":[
        {"if":{"cond":{"cmp":{"left":0_i32,"op":"eq","right":0_i32}},"then":[
            {"declare":{"kind":"number","bind":"one","min":1_i32,"max":1_i32}}
        ]}}
    ]}));
    let actor = Viewer::P1;
    let game = activate(actor, &deck(), "unit-amulet", &body);
    let (_, shape, projection) =
        project_game(&game, actor, &QuestionId::new("branch".into())).unwrap();
    assert!(matches!(
        shape,
        wire::QuestionShape::Integer { bounds, .. } if bounds.max == Some(bounds.min)
    ));
    assert_eq!(projection.question().auto_answer, wire::AutoAnswer::Unknown);
}

#[test]
fn public_top_reveal_delivers_one_notice_to_both_seats_without_private_cards_or_positions() {
    let fixture = board_fixture::fixture().unwrap();
    let body = program(json!({"cost":{"pp":1_i32},"do":[
        {"reveal":{"top":{"who":"self","n":1_i32},"bind":"shown"}},
        {"shuffle":{"who":"self"}}
    ]}));
    let build = |tail, secret| activate(Viewer::P1, &["unit-spell", tail], secret, &body);
    let mut a = build("unit-follower", "unit-amulet");
    let mut b = build("unit-amulet", "unit-follower");
    let expected = fixture
        .references
        .resolve(
            &fixture.catalog,
            &player::observation::CardFace {
                card: "unit-spell".into(),
                face: 0,
            },
        )
        .unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        let notices = a.take_battle_notices(viewer, &fixture.references).unwrap();
        assert_eq!(
            notices,
            b.take_battle_notices(viewer, &fixture.references).unwrap()
        );
        let [notice] = notices.as_slice() else {
            panic!("one public notice")
        };
        assert_eq!(notice.kind, NoticeKind::TemporaryReveal);
        let [shown] = notice.cards.as_slice() else {
            panic!("one authorized face")
        };
        assert_eq!(shown.card, expected);
        assert_eq!(shown.owner, seat(Viewer::P1));
        assert_eq!(shown.position, None);
        assert!(
            notice
                .source
                .as_ref()
                .is_none_or(|source| source.holder.is_none())
        );
    }
}
