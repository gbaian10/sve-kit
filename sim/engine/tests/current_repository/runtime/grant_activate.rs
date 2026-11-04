#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the printed grant scenario."
)]
use super::event_cemetery::finish_checks;
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP11-011"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-T02"}, {"id":"second","card":"BP01-T02"},
        {"id":"other","card":"BP01-037"}, {"id":"ordinary-fairy","card":"BP22-011"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}},
        {"id":"foreign","card":"BP01-T02"}
    ]);
    initial
}

fn cast(board: &mut Game, count: usize) {
    let CardPlay::Awaiting(question) = board.play_internal_card("spell", actor()).unwrap() else {
        panic!("printed fairy selection");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("fairy targets");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "only the two own fairy tokens are eligible"
    );
    assert!(
        domain
            .members
            .iter()
            .all(|member| member.card.as_deref() == Some("BP01-T02")),
        "non-fairies and opposing fairies are excluded"
    );
    let handles = domain
        .members
        .iter()
        .take(count)
        .map(|member| member.handle)
        .collect();
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap(),
    );
    finish_checks(board);
}

#[test]
fn bramble_grants_each_fairy_a_targeted_activation_after_the_spell_has_left() {
    let mut board = game(&position());
    cast(&mut board, 2);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
    let first = board.internal_activated_sources("first").unwrap();
    let second = board.internal_activated_sources("second").unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_ne!(first[0].instance, second[0].instance);
    for holder in ["other", "ordinary-fairy", "foreign"] {
        assert_eq!(
            board.internal_activated_sources(holder).unwrap().as_slice(),
            []
        );
    }
    for (index, (holder, source)) in [("first", &first[0]), ("second", &second[0])]
        .into_iter()
        .enumerate()
    {
        let CardPlay::Awaiting(question) = board
            .activate_internal_card_instance(holder, source.instance, actor())
            .unwrap()
        else {
            panic!("granted enemy target");
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("enemy follower targets");
        };
        let enemy = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap();
        let activated = resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![enemy.handle]),
                )
                .unwrap(),
        );
        assert_eq!(activated.source, *source);
        let damage = activated
            .progress
            .facts
            .iter()
            .filter_map(|fact| {
                let Fact::Damage(event) = fact else {
                    return None;
                };
                Some(event)
            })
            .collect::<Vec<_>>();
        assert_eq!(damage.len(), 2);
        assert!(
            damage
                .iter()
                .all(|event| event.source.object == source.holder)
        );
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["objects"][holder]["acted"], true);
        assert_eq!(after["objects"]["enemy"]["damage"], (index + 1) * 3);
        assert_eq!(after["P2"]["leader"]["life"], 20 - (index + 1));
        assert_eq!(after["P1"]["pp"]["current"], 8_u8);
        let before = after;
        let CardPlay::Awaiting(retry) = board
            .activate_internal_card_instance(holder, source.instance, actor())
            .unwrap()
        else {
            panic!("retry declares its target before paying");
        };
        let InputDomain::Targets(retry_domain) = &retry.domain else {
            panic!("retry target");
        };
        let retry_enemy = retry_domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap();
        let error = board
            .answer_internal_card(
                retry.actor,
                retry.token,
                Answer::Targets(vec![retry_enemy.handle]),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("whole-card posture cost requires a standing source")
        );
        assert_eq!(board.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn bramble_accepts_zero_fairies_without_installing_any_ability() {
    let mut board = game(&position());
    cast(&mut board, 0);
    for holder in ["first", "second", "other", "ordinary-fairy", "foreign"] {
        assert_eq!(
            board.internal_activated_sources(holder).unwrap().as_slice(),
            []
        );
    }
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 8_u8);
    assert_eq!(after["P1"]["cemetery"], json!(["spell"]));
}
