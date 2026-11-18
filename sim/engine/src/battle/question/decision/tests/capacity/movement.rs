use super::*;
use crate::game::player::Input;
use sve_battle_contract::Subject;

fn private(actor: Viewer, cards: &[&str], other_secret: &str) -> Game {
    let body = json!({"cost":{"pp":1_u32},"do":[
        {"look":{"bind":"seen","top":{"who":"self","n":2_u32}}},
        {"choose":{"bind":"picked","from":{"in":"seen"},"upto":2_u32}},
        {"move":{"ref":"picked","to":"ex","result":"moved"}},
        {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"moved","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}},
        {"life":{"who":"self","delta":1_u32}}
    ]});
    let zones = json!({"field":[{"id":"source","card":"unit-follower"}],
        "ex":(0_u32..4).map(|index| json!({"id":format!("old-{index}"),"card":"unit-follower"})).collect::<Vec<_>>(),
        "deck":cards.iter().enumerate().map(|(index, card)| json!({"id":format!("private-{index}"),"card":card})).collect::<Vec<_>>()});
    let mut game = make(actor, &body, zones, other_secret);
    choose_all(&mut game, actor, "look-choice");
    game
}

fn projected(game: &Game, actor: Viewer) -> (player::Question, wire::Selection, Binding) {
    let native = question(game, actor).unwrap();
    assert!(
        matches!(domain(&native), Some(DecisionShape::MovementCapacity(_))),
        "real movement capacity"
    );
    let before = board_fixtures::exact_authority(game);
    let (usage, shape, binding) = project_question(
        &native,
        &components(game, actor),
        &QuestionId::new("capacity".into()),
    )
    .unwrap();
    assert_eq!(usage, wire::QuestionUse::MovementCapacity);
    assert!(
        board_fixtures::exact_authority(game) == before,
        "projection preserves complete authority"
    );
    let wire::QuestionShape::Select { selection } = shape else {
        panic!("capacity");
    };
    (native, selection, binding)
}

#[test]
fn private_look_capacity_keeps_both_directions_of_complete_seat_views_private() {
    let refs = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let a = private(
            actor,
            &["unit-follower", "unit-spell", "unit-follower"],
            "unit-spell",
        );
        let b = private(
            actor,
            &["unit-follower", "unit-amulet", "unit-follower"],
            "unit-spell",
        );
        let native_a = question(&a, actor).unwrap();
        let native_b = question(&b, actor).unwrap();
        let (
            Some(DecisionShape::MovementCapacity(capacity_a)),
            Some(DecisionShape::MovementCapacity(capacity_b)),
        ) = (domain(&native_a), domain(&native_b))
        else {
            panic!("real private domains");
        };
        assert_eq!(capacity_a.selection.cards.len(), 2);
        assert_eq!(capacity_b.selection.cards.len(), 2);
        assert_ne!(
            capacity_a.selection.cards, capacity_b.selection.cards,
            "actual candidate identities differ before projection"
        );
        let before_a = board_fixtures::exact_authority(&a);
        let before_b = board_fixtures::exact_authority(&b);
        assert!(
            a.observe(other).unwrap() == b.observe(other).unwrap(),
            "complete waiting boards hide private arrivals"
        );
        assert!(
            a.input(other) == b.input(other),
            "complete waiting summaries hide candidate identities"
        );
        assert!(matches!(a.input(other), Input::Waiting(_)));
        assert_eq!(components(&a, other), components(&b, other));
        let id = QuestionId::new("private".into());
        assert!(matches!(a.battle_question(other, &id, refs), Ok(None)));
        assert!(matches!(b.battle_question(other, &id, refs), Ok(None)));
        assert!(matches!(a.battle_question(actor, &id, refs), Ok(Some(_))));
        assert_ne!(projected(&a, actor).1, projected(&b, actor).1);
        assert!(
            board_fixtures::exact_authority(&a) == before_a,
            "read-only authority"
        );
        assert!(
            board_fixtures::exact_authority(&b) == before_b,
            "read-only authority"
        );
        let changed_other = private(
            actor,
            &["unit-follower", "unit-spell", "unit-follower"],
            "unit-amulet",
        );
        assert!(
            a.observe(actor).unwrap() == changed_other.observe(actor).unwrap(),
            "opponent secrets do not change the actor board"
        );
        assert!(
            a.input(actor) == changed_other.input(actor),
            "opponent secrets do not change the complete native question"
        );
        assert_eq!(components(&a, actor), components(&changed_other, actor));
        assert_eq!(projected(&a, actor).1, projected(&changed_other, actor).1);
    }
}

#[test]
fn private_ex_arrivals_and_the_real_remainder_order_commit_through_the_original_channels() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = private(
            actor,
            &["unit-follower", "unit-spell", "unit-follower"],
            "unit-spell",
        );
        let (native, selection, binding) = projected(&game, actor);
        assert_eq!(selection.capacity.unwrap().destination.zone, Zone::Ex);
        assert!(selection.candidates.iter().all(|candidate| matches!(
            &candidate.subject,
            Some(Subject::Card {
                object: Some(_),
                ..
            })
        )));
        submit_exact(
            &mut game,
            &native,
            &binding,
            &Answer::Select {
                selected: vec![selection.candidates.last().unwrap().id.clone()],
                groups: None,
                roles: vec![],
            },
        );
        let order = question(&game, actor).unwrap();
        let (_, shape, order_binding) = project_question(
            &order,
            &components(&game, actor),
            &QuestionId::new("remainder".into()),
        )
        .unwrap();
        let wire::QuestionShape::Order { items, .. } = shape else {
            panic!("real printed remainder order");
        };
        submit_exact(
            &mut game,
            &order,
            &order_binding,
            &Answer::Order {
                order: items.iter().map(|item| item.id.clone()).collect(),
            },
        );
        let state = game.projection(View::Referee).unwrap();
        assert!(
            state[seats(actor).0]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("private-1"))
        );
        assert_eq!(
            state[seats(actor).0]["deck"],
            json!(["private-2", "private-0"])
        );
        assert_eq!(state[seats(actor).0]["leader"]["life"], 21_u32);
    }
}
