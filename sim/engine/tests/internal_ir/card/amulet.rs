use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn bp01_041_candidate_amulet_pays_pp_and_act_after_card_entry() {
    let cards = lowerer().lower(fanfare::expanded::tree()).unwrap();
    let program = &cards["BP01-041"];
    let mut game =
        fanfare::fragment_position(1, "BP01-041", 9, json!([{"id":"d0","card":"BP03-009"}]));
    game.load_internal_card("BP01-041", program).unwrap();
    game.load_internal_card("BP03-009", &card(vec![])).unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    game.resume_internal_card().unwrap();
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert_eq!(result.source.holder.generation, Generation(2));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 4);
    assert_eq!(view["objects"]["s"]["acted"], true);
    assert_eq!(view["P1"]["hand"], json!(["d0"]));
    game.resume_internal_card().unwrap();
    let before = exact_authority(&game);
    game.activate_internal_card(
        "s",
        program.card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn bp20_100_fragments_select_before_act_sacrifice_and_damage() {
    // usable_if is deliberately excluded; these are fragments, not a complete card.
    let mut tree = fanfare::expanded::tree().clone();
    let template = tree["cards"]["BP01-041"].clone();
    let mut entry = template["abilities"][0].clone();
    entry["line"] = json!(1);
    entry.as_object_mut().unwrap().remove("activate");
    entry["fanfare"] = json!({"do":[{"draw":{"n":1}}]});
    let mut activation = template["abilities"][0].clone();
    activation["line"] = json!(2);
    activation["activate"] = json!({
        "cost":{"steps":[{"act_self":true},{"self_to_cemetery":true}]},
        "targets":[{"bind":"t","from":{"who":"opp","zone":"field"},"n":1}],
        "do":[{"damage":{"to":"t","n":4}}]
    });
    tree["cards"]["BP20-100"] = template;
    tree["cards"]["BP20-100"]["abilities"] = json!([entry, activation]);
    let lowerer = lowerer();
    let cards = lowerer.lower(&tree).unwrap();
    let program = &cards["BP20-100"];
    let (catalog, mut initial) =
        fanfare::fragment_setup(1, "BP20-100", 2, json!([{"id":"d0","card":"BP03-009"}]));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"t","card":"BP03-009"}]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "amulet-fragment",
    )
    .unwrap();
    game.load_internal_card("BP20-100", program).unwrap();
    game.load_internal_card("BP03-009", &card(vec![])).unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::P1).unwrap()["P1"]["hand"],
        json!(["d0"])
    );
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("target question")
    };
    assert!(matches!(question.domain, InputDomain::Targets(_)));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["field"], json!(["s"]));
    assert_eq!(view["objects"]["s"]["acted"], false);
    let result = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(result.source.holder.generation, Generation(2));
    assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Movement(m) if m.method == MoveMethod::Put && m.movement.before == result.source.holder)));
    assert!(result.progress.facts.iter().any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 4 && hit.source.object == result.source.holder)));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["s"])
    );
    tree["cards"]["BP20-100"]["abilities"][1]["activate"]["usable_if"] = json!({"cmp":{"left":{"count":{"who":"self","zone":"ex","kind":"crest"}},"op":"ge","right":3}});
    assert_eq!(
        lowerer.lower(&tree).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn amulet_capacity_combat_and_lastword_keep_source_information() {
    let information = ObjectView::SourceInformation {};
    let read = |property| {
        source_information::source_read(&information, CharacteristicView::Effective, property)
    };
    let sacrifice = payment::paid_ability(
        vec![payment::pp(1), payment::act(), payment::self_to_cemetery()],
        vec![
            draw_value(1, read(Some(Stat::Attack))),
            draw_value(2, read(Some(Stat::Life))),
            draw_value(3, read(None)),
        ],
    );
    let lastword = triggers::lastword(ability(
        2,
        vec![source_information::source_hit(4, read(None))],
    ));
    let mut abilities = vec![sacrifice, lastword];
    for ability in &mut abilities {
        ability.source.card_no = "unit-amulet".into();
        ability.source.card_id = "unit-amulet".into();
        locate_steps(
            &mut ability.node.body.as_mut().unwrap().steps,
            &ability.source,
            &ability.node.source,
        );
    }
    let program = card(abilities);
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0..5)
            .map(|n| json!({"id":format!("a{n}"),"card":"unit-amulet"}))
            .collect::<Vec<_>>()
    );
    let build = |initial: &Value| {
        let mut game =
            Game::new(catalog(), initial, &Value::Null, &Value::Null, "amulet-lki").unwrap();
        game.load_internal_card("unit-amulet", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game
    };
    let mut full = build(&initial);
    let before = exact_authority(&full);
    full.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(exact_authority(&full), before);
    let mut combat = initial.clone();
    combat["turn"]["active"] = json!("P2");
    combat["players"]["P1"]["zones"]["field"] =
        json!([{"id":"a0","card":"unit-amulet","state":{"acted":true}}]);
    combat["players"]["P2"]["zones"]["field"] =
        json!([{"id":"f","card":"unit-follower"},{"id":"b","card":"unit-amulet"}]);
    let mut combat = build(&combat);
    let before = combat.projection(View::Referee).unwrap();
    let attack = |game: &mut Game, attacker: &str, target: &str| {
        game.internal_rule_action(&json!({"do":"attack","attacker":attacker,"target":target}))
            .unwrap()
            .outcome
    };
    assert_eq!(attack(&mut combat, "f", "a0"), "cannot-attack");
    assert_eq!(attack(&mut combat, "b", "P1.leader"), "cannot-attack");
    assert_eq!(combat.projection(View::Referee).unwrap(), before);
    // The same position accepts a follower attack, so only the amulet type rejected those.
    assert_eq!(attack(&mut combat, "f", "P1.leader"), "resolved");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    let mut game = build(&initial);
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    game.resume_internal_card().unwrap();
    let activation = activate(&mut game, 1);
    cards(&activation.progress, 1, 0, 0);
    cards(&activation.progress, 2, 0, 0);
    cards(&activation.progress, 3, 1, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let lastword = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
            .unwrap(),
    );
    assert_eq!(lastword.source.holder, activation.source.holder);
    assert_ne!(lastword.source.instance, activation.source.instance);
    assert!(lastword.progress.facts.iter().any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 1 && hit.source.object == lastword.source.holder)));
    game.resume_internal_card().unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
}
