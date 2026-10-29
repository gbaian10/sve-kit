#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these real Lastword scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, zone: &str, enemy: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"third","card":"BP01-037"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":enemy}]);
    initial
}

fn destructive_game(initial: &Value) -> Game {
    let mut own_field = subjects();
    let Selector::Zone { players, .. } = &mut own_field else {
        panic!("field selector");
    };
    players.relation = PlayerRelation::SelfPlayer;
    players.text_quantifier = TextQuantifier::Implicit;
    players.cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    with_producer(initial, Action::Destroy { objects: own_field })
}

fn trigger_target(game: &mut Game, card: &str) -> Activation {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed trigger");
    };
    assert_eq!(
        options.len(),
        1_usize,
        "exactly one printed trigger is pending"
    );
    assert_eq!(
        options[0].card, card,
        "the actual printing supplies the trigger"
    );
    assert_eq!(
        pending.actor,
        actor(),
        "the source controller declares targets"
    );
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap()
    else {
        panic!("formal target declaration");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal targets");
    };
    assert_eq!(
        domain.cardinality.minimum,
        Nat(1),
        "formal targets keep the printed minimum"
    );
    assert_eq!(
        domain.members.len(),
        1_usize,
        "only the opposing follower is eligible"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1_usize,
        "one trigger commits exactly one AbilityPlayed"
    );
    game.resume_internal_card().unwrap();
    result
}

fn enter(game: &mut Game) {
    assert!(
        matches!(
            game.play_internal_card("source", actor()).unwrap(),
            CardPlay::Entered(_)
        ),
        "the real follower enters"
    );
}

fn depart(game: &mut Game) {
    activate(game, "P1");
    game.resume_internal_card().unwrap();
    assert!(
        !game.projection(View::Referee).unwrap()["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "Lastword starts after its holder has left the field"
    );
}

#[test]
fn bp08_078_same_line_fanfare_and_lastword_each_damage_and_heal() {
    let mut game = destructive_game(&position("BP08-078", "hand", "BP01-037"));
    enter(&mut game);
    let entry = trigger_target(&mut game, "BP08-078");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    depart(&mut game);
    let exit = trigger_target(&mut game, "BP08-078");
    assert_ne!(
        entry.source.definition, exit.source.definition,
        "same printed line keeps separate trigger scopes"
    );
    assert_eq!(entry.source.holder, exit.source.holder);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(after["P1"]["leader"]["life"], 22_u8);
    assert_eq!(after["P2"]["leader"]["life"], 20_u8);
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp12_083_alias_pays_self_to_cemetery_mills_two_then_lastword_destroys() {
    let initial = position("PR-335", "field", "BP04-117");
    let selected = selection_with_rules();
    let ability = selected.current["PR-335"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let mut game = Game::from_selected_effects(
        selected.clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "lastword-activated",
    )
    .unwrap();
    resolved(
        game.activate_internal_card("source", ability, actor())
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 9_u8);
    assert_eq!(before["P1"]["field"], json!([]));
    assert_eq!(before["P1"]["deck"], json!(["third"]));
    assert_eq!(
        before["P1"]["cemetery"],
        json!(["source", "first", "second"])
    );
    assert_eq!(before["P2"]["field"], json!(["enemy"]));
    trigger_target(&mut game, "PR-335");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["field"], json!([]));
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P1"]["pp"]["current"], 9_u8);
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp16_p30_token_alias_keeps_lastword_after_ceasing_to_exist() {
    let mut game = destructive_game(&position("EBD03-T01", "field", "BP01-037"));
    depart(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert!(
        !before["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "departed tokens do not remain in cemetery"
    );
    trigger_target(&mut game, "EBD03-T01");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(after["P1"]["deck"], json!(["second", "third"]));
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("first"))
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
}

#[test]
fn bp17_016_same_line_triggers_apply_permanent_shrink_and_lethal_cleanup() {
    let mut game = destructive_game(&position("BP17-016", "hand", "BP04-117"));
    enter(&mut game);
    let entry = trigger_target(&mut game, "BP17-016");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 3_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 3_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
    depart(&mut game);
    let exit = trigger_target(&mut game, "BP17-016");
    assert_ne!(entry.source.definition, exit.source.definition);
    assert_eq!(entry.source.holder, exit.source.holder);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["field"], json!([]));
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert!(game.internal_pending_choice().unwrap().is_none());
}
