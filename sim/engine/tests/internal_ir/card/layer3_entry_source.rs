#![expect(
    clippy::panic,
    reason = "Entry-source fixtures require the explicit pending and mode questions."
)]
use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn tree(before: &[Value], after: &[Value]) -> Value {
    let mut tree = evolution_linked_movement::tree(&[(
        "fanfare",
        json!({"do":[{"choice":{
            "n":1,"instead":[{"if":{"entered_by":{"ref":"this","by":"ability","source":{"trait":"ウマ娘"}}},"upto":2}],
            "options":[[{"life":{"who":"self","delta":1}}],[{"life":{"who":"self","delta":2}}]]
        }}]}),
    )]);
    let mut actions = before.to_vec();
    actions.push(json!({"move":{"ref":"incoming","to":"field","result":"entered"}}));
    actions.extend_from_slice(after);
    let producer = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"pp":0},"targets":[{"bind":"incoming","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],
            "do":actions
        }),
    )]);
    tree["cards"]["unit-source"] = producer["cards"]["unit-follower"].clone();
    tree
}

fn source_catalog(source_trait: bool, entrant_trait: bool) -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let mut rows: Vec<Value> = snapshot
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut producer = rows
        .iter()
        .find(|row| row["number"] == "unit-follower")
        .unwrap()
        .clone();
    producer["number"] = json!("unit-source");
    producer["faces"][0]["name"] = json!("unit-source");
    if source_trait {
        producer["faces"][0]["traits"] = json!(["ウマ娘"]);
    }
    rows.push(producer);
    if entrant_trait {
        rows.iter_mut()
            .find(|row| row["number"] == "unit-follower")
            .unwrap()["faces"][0]["traits"] = json!(["ウマ娘"]);
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-source"] =
        json!({"status":"complete","review":"synthetic","abilities":[]});
    Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("unit.yaml".into(), document.to_string())],
        )
        .unwrap(),
    )
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected entry procedure question");
    };
    question
}

fn configured(
    source_trait: bool,
    entrant_trait: bool,
    before: &[Value],
    after: &[Value],
    play: bool,
) -> (Game, AbilityId) {
    let programs = lowerer().lower(&tree(before, after)).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-source"}]);
    initial["players"]["P1"]["zones"][if play { "hand" } else { "cemetery" }] = json!([
        {"id":"incoming","card":"unit-follower","state":{"entered_by":"ability","entry_source_traits":["ウマ娘"]}}
    ]);
    let mut game = Game::new(
        source_catalog(source_trait, entrant_trait),
        &initial,
        &Value::Null,
        &Value::Null,
        "entry-source",
    )
    .unwrap();
    for number in ["unit-source", "unit-follower"] {
        game.load_internal_card(number, &programs[number]).unwrap();
    }
    (game, programs["unit-source"].card().abilities[0].node.id)
}

fn enter(game: &mut Game, producer: AbilityId, play: bool) -> DecisionView {
    if play {
        assert!(
            matches!(
                game.play_internal_card("incoming", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ),
            "ordinary play commits the entrant before its Fanfare is selected"
        );
    } else {
        let pick = question(
            game.activate_internal_card("source", producer, PlayerId { seat: 1 })
                .unwrap(),
        );
        resolved(
            game.answer_internal_card(pick.actor, pick.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("fanfare pending");
    };
    question(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn mode_count_reads_the_entering_ability_sources_trait_instead_of_the_entrant() {
    for (source_trait, entrant_trait, count) in [(true, false, 2), (false, true, 1)] {
        let (mut game, producer) = configured(source_trait, entrant_trait, &[], &[], false);
        let modes = enter(&mut game, producer, false);
        let InputDomain::Modes {
            count: actual,
            cardinality,
            modes: available,
        } = &modes.domain
        else {
            panic!("mode count");
        };
        assert_eq!(*actual, Nat(count));
        assert_eq!(cardinality.minimum, Nat(1));
        assert_eq!(cardinality.maximum, Some(Nat(count)));
        let before = exact_authority(&game);
        game.answer_internal_card(
            modes.actor,
            modes.token,
            Answer::Modes(vec![available[0], available[0]]),
        )
        .unwrap_err();
        assert_eq!(
            exact_authority(&game),
            before,
            "a mode cannot be selected twice"
        );
        if count == 1 {
            game.answer_internal_card(modes.actor, modes.token, Answer::Modes(available.clone()))
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        let chosen = available
            .iter()
            .take(usize::try_from(count).unwrap())
            .copied()
            .collect();
        resolved(
            game.answer_internal_card(modes.actor, modes.token, Answer::Modes(chosen))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if count == 2 { 23 } else { 21 }
        );
    }
}

#[test]
fn entry_source_traits_are_frozen_before_the_source_changes_or_leaves() {
    let gain = json!({"apply":{"on":"this","effect":{"add_trait":{"trait":"ウマ娘"}}}});
    let leave = json!({"move":{"ref":"this","to":"cemetery"}});
    for (source_trait, before, after, count) in [
        (false, vec![gain.clone()], vec![], 2),
        (false, vec![], vec![gain], 1),
        (true, vec![], vec![leave.clone()], 2),
        (true, vec![leave], vec![], 2),
    ] {
        let (mut game, producer) = configured(source_trait, false, &before, &after, false);
        let modes = enter(&mut game, producer, false);
        let InputDomain::Modes {
            count: actual,
            modes: available,
            ..
        } = &modes.domain
        else {
            panic!("frozen source count");
        };
        assert_eq!(*actual, Nat(count));
        let chosen = available
            .iter()
            .take(usize::try_from(count).unwrap())
            .copied()
            .collect();
        resolved(
            game.answer_internal_card(modes.actor, modes.token, Answer::Modes(chosen))
                .unwrap(),
        );
    }
}

#[test]
fn normal_play_ignores_entrant_traits_spoofed_flags_and_unrelated_sources() {
    let (mut game, producer) = configured(true, true, &[], &[], true);
    let modes = enter(&mut game, producer, true);
    let InputDomain::Modes {
        count,
        modes: available,
        ..
    } = &modes.domain
    else {
        panic!("normal play mode count");
    };
    assert_eq!(*count, Nat(1));
    resolved(
        game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![available[0]]))
            .unwrap(),
    );
}

#[test]
fn entry_source_condition_keeps_method_scope_and_trait_filters_closed() {
    let mut by_play = tree(&[], &[]);
    by_play["cards"]["unit-follower"]["abilities"][0]["fanfare"]["do"][0]["choice"]["instead"][0]
        ["if"]["entered_by"]["by"] = json!("play");
    lowerer().lower(&by_play).unwrap_err();
    let mut filtered = tree(&[], &[]);
    filtered["cards"]["unit-follower"]["abilities"][0]["fanfare"]["do"][0]["choice"]["instead"]
        [0]["if"]["entered_by"]["source"]["kind"] = json!("follower");
    lowerer().lower(&filtered).unwrap_err();
    let mut outside = tree(&[], &[]);
    let ability = &mut outside["cards"]["unit-follower"]["abilities"][0];
    let body = ability.as_object_mut().unwrap().remove("fanfare").unwrap();
    ability["activate"] = body;
    ability["activate"]["cost"] = json!({"pp":0});
    lowerer().lower(&outside).unwrap_err();
}

#[test]
fn a_prototype_cause_without_ability_source_evidence_cannot_claim_the_current_cards_trait() {
    let (mut game, _) = configured(true, false, &[], &[], false);
    let program = checked(vec![entry::enter(
        90,
        entry::own_zone(ZoneKind::Cemetery, 9),
    )]);
    let result = game.execute_internal(
        &program,
        &source("synthetic"),
        Context {
            source: "source".into(),
            generation: Generation(0),
            controller: PlayerId { seat: 1 },
        },
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => game.internal_pending_choice().unwrap_err(),
    };
    assert!(
        matches!(error, EngineFailure::Unsupported(_)),
        "missing producer evidence remains unsupported: {error}"
    );
}
