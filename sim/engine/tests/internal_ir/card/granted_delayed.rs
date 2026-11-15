use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::game::player::Viewer;
use sve_engine::game::player::observation::{InstalledChange, NumberChange, ObservedZone};

fn program(depart: bool) -> WellFormedCardProgram {
    lowerer()
        .lower(&document(depart))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn document(depart: bool) -> Value {
    let mut document = equipment_played::tree(
        &json!({"line":1_u8,"activate":{
            "cost":{"pp":0},"do":[{"grant":{
                "on":{"who":"self","zone":"field","kind":"follower"},"until":"game",
                "abilities":[{"activate":{"cost":{"pp":0},"do":[{"delayed":{
                    "on":{"life_changed":{"who":"self","direction":"down"}},
                    "until":"turn","next_only":false,
                    "do":[{"buff":{"on":"this","power":2,"until":"turn"}}]
                }}]}}]
            }}]
        }}),
        "unit-amulet",
    );
    let mut steps = vec![json!({"life":{"who":"self","delta":-1}})];
    if depart {
        steps.push(json!({"destroy":{"ref":"this"}}));
    }
    let trigger = equipment_played::tree(
        &json!({"line":2_u8,"activate":{
            "cost":{"pp":0},"do":steps
        }}),
        "unit-amulet",
    );
    document["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(trigger["cards"]["unit-amulet"]["abilities"][0].clone());
    document
}

#[test]
fn granted_delayed_callbacks_refuse_nested_installation_at_the_closed_boundary() {
    let mut nested = document(false);
    let path = "/cards/unit-amulet/abilities/0/activate/do/0/grant/abilities/0/activate/do/0";
    let callback = nested.pointer(path).unwrap().clone();
    *nested.pointer_mut(&format!("{path}/delayed/do")).unwrap() = json!([callback]);
    let error = lowerer().lower(&nested).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
    assert_eq!(
        error.path,
        "/cards/unit-amulet/abilities/0/activate/do/0/abilities/0/activate/do/0"
    );
}

fn position(actor: Viewer, depart: bool) -> (Game, [AbilityId; 2]) {
    let own = if actor == Viewer::P1 { "P1" } else { "P2" };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!([]);
    initial["players"][own]["zones"]["field"] = json!([
        {"id":"giver","card":"unit-amulet"},
        {"id":"receiver","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "granted-delayed",
    )
    .unwrap();
    let program = program(depart);
    let definitions = [
        program.card().abilities[0].node.id,
        program.card().abilities[1].node.id,
    ];
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    (game, definitions)
}

#[test]
fn granted_delayed_callback_keeps_the_original_text_provider_after_its_departure() {
    for actor in [Viewer::P1, Viewer::P2] {
        for depart in [false, true] {
            let (mut game, definitions) = position(actor, depart);
            resolved(
                game.activate_internal_card("giver", definitions[0], actor.player())
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let granted = game.internal_activated_sources("receiver").unwrap();
            assert_eq!(
                granted.len(),
                1,
                "the receiver owns one real granted activation"
            );
            resolved(
                game.activate_internal_card_instance(
                    "receiver",
                    granted[0].instance,
                    actor.player(),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            resolved(
                game.activate_internal_card("giver", definitions[1], actor.player())
                    .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("the installed life callback is pending")
            };
            assert_eq!(
                options.len(),
                1,
                "one real life decrease produces one callback"
            );
            assert_eq!(
                options[0].card, "unit-follower",
                "the delayed choice belongs to the receiving follower"
            );
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_eq!(
                    game.observe(viewer).unwrap().pending[0].source.card,
                    "unit-amulet",
                    "the frozen pending origin already names the text provider"
                );
            }
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_provider(&game, actor, viewer, depart)
                    .expect("the public callback keeps its text provider and current receiver");
            }
        }
    }
}

fn assert_provider(game: &Game, actor: Viewer, viewer: Viewer, depart: bool) -> Option<()> {
    let before = format!("{game:?}");
    let observation = game.observe(viewer).unwrap();
    let player = observation
        .players
        .iter()
        .find(|player| player.player == actor)?;
    assert_eq!(
        player
            .zones
            .iter()
            .filter(|zone| zone.kind == ObservedZone::Cemetery)
            .flat_map(|zone| &zone.cards)
            .any(|member| member.card.properties.card == "unit-amulet"),
        depart,
        "the departure case really moved the giver into the cemetery"
    );
    let receiver = player
        .zones
        .iter()
        .flat_map(|zone| &zone.cards)
        .find(|member| member.card.properties.card == "unit-follower")?;
    let state = receiver.card.properties.state.as_ref()?;
    let effects = state
        .effects
        .iter()
        .filter(|effect| {
            matches!(
                &effect.change,
                InstalledChange::Numeric {
                    attack: Some(NumberChange::Add(2)),
                    ..
                }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        effects.len(),
        1,
        "the callback really installed its numeric change"
    );
    let source = effects[0].source.as_ref()?;
    assert_eq!(
        source.card, "unit-amulet",
        "text comes from the giver, not the receiving follower"
    );
    assert_eq!(
        source.face,
        Some(0),
        "the provider's admitted face remains exact"
    );
    assert_eq!(
        source.line,
        Some(1),
        "the text remains on the original granting line"
    );
    assert_eq!(
        source.holder.as_ref(),
        Some(&receiver.card.handle),
        "the effect's producing holder is the receiver"
    );
    assert!(
        format!("{game:?}") == before,
        "source observation must not change authority"
    );
    Some(())
}
