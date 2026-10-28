#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the scenario."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            assert_eq!(
                domain.members.len(),
                1,
                "only the opposing follower is legal"
            );
            let handle = domain.members[0].handle;
            let before = game.projection(View::Referee).unwrap();
            let wrong = PlayerId {
                seat: if question.actor.seat == 1 { 2 } else { 1 },
            };
            game.answer_internal_card(wrong, question.token, Answer::Targets(vec![handle]))
                .unwrap_err();
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                before,
                "wrong actors preserve authority"
            );
            Answer::Targets(vec![handle])
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("expected a formal target or Ward entry decision");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_) | CardPlay::Entered(_) | CardPlay::Continued(_)
        ),
        "the printed effect or entry completes"
    );
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected one printed trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "one printed trigger belongs to this event"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play);
}

#[test]
fn bp16_035_assails_a_standing_follower_then_declares_four_damage() {
    let mut initial = position("BP16-035", "field");
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P1"]["pp"]["current"], 10_u8);
}

#[test]
fn bp16_053_fanfare_and_paid_activation_each_deal_four_after_targeting() {
    let mut game = game(&position("BP16-053", "hand"));
    let entry = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, entry);
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 6_u8);
    assert_eq!(before["objects"]["enemy"]["damage"], 4_u8);
    let ability = selection().current["BP16-053"]
        .program
        .card()
        .abilities
        .iter()
        .find(|candidate| candidate.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let activation = game
        .activate_internal_card("source", ability, actor())
        .unwrap();
    assert!(matches!(&activation, CardPlay::Awaiting(_)));
    let declared = game.projection(View::Referee).unwrap();
    assert_eq!(declared["P1"]["pp"]["current"], 6_u8);
    assert_eq!(declared["objects"]["source"]["acted"], false);
    finish(&mut game, activation);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp16_p34_created_on_field_chooses_ward_then_fanfare_deals_six() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let action = Action::Token {
        tokens: vec![TokenSpec {
            template: TokenTemplateId("辺獄の悪鬼".into()),
            name: Expr::Literal(CardName("辺獄の悪鬼".into())),
            count: Expr::Literal(Nat(1)),
        }],
        destination: TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Field,
            }),
        },
        output: Binding::new(50, ScopeId(1)),
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects: vec![],
    };
    let mut game = with_producer(&initial, action);
    let creation = game
        .activate_internal_card("producer", AbilityId(1), actor())
        .unwrap();
    assert!(
        matches!(&creation, CardPlay::Awaiting(question) if matches!(&question.domain, InputDomain::WardEntry { .. }))
    );
    finish(&mut game, creation);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| state["objects"][id.as_str().unwrap()]["name"] == "辺獄の悪鬼")
    );
}

#[test]
fn ebd04_t04_alias_retains_the_ward_token_keyword() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"ward","card":"EBD04-T04","state":{"acted":true}}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":"P2.leader"})
        )
        .unwrap()
        .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"ward"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp17_088_fanfare_destroys_the_declared_opposing_follower() {
    let mut game = game(&position("BP17-088", "hand"));
    let entry = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, entry);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn bp17_088_lastword_damages_only_opposing_leaders() {
    let mut game = with_producer(
        &position("BP17-088", "field"),
        Action::Destroy {
            objects: subjects(),
        },
    );
    activate(&mut game, "P1");
    game.resume_internal_card().unwrap();
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
