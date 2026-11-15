//! Selections from one loader share candidates; printings and changed programs stay checked.

use super::{author, initial, selection};
use alloc::sync::Arc;
use core::array::from_fn;
use serde_json::{Value, json};
use sve_engine::catalog::{CurrentSelectionLoader, SelectedEffects};
use sve_engine::game::internal::CardPlay;
use sve_engine::game::player::{
    AdmissionGap, AdmissionGapReason, DeckConstruction, PlayerDeck, Viewer,
};
use sve_engine::game::{Game, View};
use sve_engine::ir::PlayerId;

fn construct(selected: SelectedEffects) -> sve_engine::Result<Game> {
    Game::from_selected_effects(
        selected,
        &initial(0),
        &Value::Null,
        &Value::Null,
        "shared-admission",
    )
}

#[test]
fn a_changed_program_never_reuses_the_admission_of_the_same_card_number() {
    let loader = CurrentSelectionLoader::new().unwrap();
    let (snapshot, keywords, one) = selection(1);
    let (_, _, two) = selection(2);
    // The third selection meets the first candidate again after the changed one was admitted.
    for (input, expected) in [(&one, 1), (&two, 2), (&one, 1)] {
        let selected = loader.select(&snapshot, &keywords, input).unwrap();
        let mut game = construct(selected).unwrap();
        let played = game
            .play_internal_card("creator", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(played, CardPlay::Resolved(_)));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["ex"].as_array().map(Vec::len), Some(expected));
    }
}

#[test]
fn a_shared_candidate_is_still_checked_against_every_printing_face() {
    let (creator, mut source) = author(
        "creator",
        "11111111111111111111111111111111",
        "スペル",
        vec![json!({"spell":{"do":[{"draw":{"n":1_u8}}]}})],
    );
    source["printings"] = json!(["creator", "reprint"]);
    let reprint = |kind: &str| {
        let mut card = creator.clone();
        card["number"] = json!("reprint");
        card["faces"][0]["card_type"] = json!(kind);
        card
    };
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let input = json!({"documents":[],"authors":[source]});
    let loader = CurrentSelectionLoader::new().unwrap();
    let select = |kind: &str| {
        let snapshot = format!("{creator}\n{}\n", reprint(kind));
        loader.select(&snapshot, &keywords, &input).unwrap()
    };
    let matching = select("スペル");
    construct(matching.clone()).unwrap();
    let foreign = select("フォロワー");
    assert!(
        Arc::ptr_eq(&matching.current["reprint"], &foreign.current["reprint"]),
        "both selections share the candidate whose program-only checks already passed"
    );
    // The reprint's own face is read again, so the shared outcome cannot admit it.
    construct(foreign).unwrap_err();
    construct(matching).unwrap();
}

#[test]
fn a_program_rejection_is_identical_for_every_printing_and_selection_until_the_program_changes() {
    let (plain, keywords, mut admitted) = selection(1);
    let mut reprint: Value = serde_json::from_str(plain.lines().next().unwrap()).unwrap();
    reprint["number"] = json!("reprint");
    let snapshot = format!("{plain}{reprint}\n");
    admitted["authors"][0]["printings"] = json!(["creator", "reprint"]);
    let stages = |extra: Option<Value>| {
        let mut stages = vec![json!([{"banish":{"top":{"who":"p","n":2_u8}}}])];
        stages.extend(extra);
        json!([
            {"banish":{"ref":{"any_of":[{"who":"all","zone":"field"},{"who":"all","zone":"ex"}]}}},
            {"each_player":{"as":"p","stages":stages}}
        ])
    };
    let mut rejected = admitted.clone();
    for (input, extra) in [
        (&mut admitted, None),
        // Each player's deck top stays private, so a further stage that draws is refused at load.
        (&mut rejected, Some(json!([{"draw":{"n":1_u8}}]))),
    ] {
        input["authors"][0]["input"]["tree"]["cards"]["creator"]["faces"]["f:11111111111111111111111111111111"]
            ["abilities"][0]["spell"]["do"] = stages(extra);
    }
    let failure = |loader: &CurrentSelectionLoader, input: &Value| {
        let selected = loader.select(&snapshot, &keywords, input).unwrap();
        construct(selected).unwrap_err().to_string()
    };
    let unshared = failure(&CurrentSelectionLoader::new().unwrap(), &rejected);
    assert!(unshared.starts_with("unsupported: "), "{unshared}");
    let loader = CurrentSelectionLoader::new().unwrap();
    assert_eq!(failure(&loader, &rejected), unshared);
    assert_eq!(failure(&loader, &rejected), unshared);

    // Each printing of the rejected candidate is its own strict gap.
    let decks: [PlayerDeck; 2] = from_fn(|_| PlayerDeck {
        construction: DeckConstruction::Class("ニュートラル".into()),
        main: ["creator", "reprint"]
            .into_iter()
            .cycle()
            .take(8)
            .map(str::to_owned)
            .collect(),
        evolve: vec![],
    });
    let gap = |number: &str| AdmissionGap {
        card_no: number.into(),
        reason: AdmissionGapReason::UnsupportedRuntime,
    };
    let selected = loader.select(&snapshot, &keywords, &rejected).unwrap();
    let report = Game::check_player_decks(&selected, &decks).unwrap();
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(report.gaps(viewer), [gap("creator"), gap("reprint")]);
    }

    // The corrected program is another candidate; the rejected one stays rejected beside it.
    let corrected = loader.select(&snapshot, &keywords, &admitted).unwrap();
    assert!(
        Game::check_player_decks(&corrected, &decks)
            .unwrap()
            .is_admitted()
    );
    construct(corrected).unwrap();
    assert_eq!(failure(&loader, &rejected), unshared);
}

#[expect(
    clippy::indexing_slicing,
    reason = "The synthetic catalog and setup have fixed coordinates."
)]
fn source_reprint(ability: Value, zone: &str) -> Game {
    let (creator, mut source) = author(
        "creator",
        "11111111111111111111111111111111",
        "フォロワー",
        vec![ability],
    );
    source["printings"] = json!(["creator", "reprint"]);
    let mut reprint = creator.clone();
    reprint["number"] = json!("reprint");
    let snapshot = format!("{creator}\n{reprint}\n");
    let keywords = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let selected = CurrentSelectionLoader::new()
        .unwrap()
        .select(
            &snapshot,
            &keywords,
            &json!({"documents":[],"authors":[source]}),
        )
        .unwrap();
    let mut initial = initial(0);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"reprint"}]);
    Game::from_selected_effects(
        selected,
        &initial,
        &Value::Null,
        &Value::Null,
        "source-reprint",
    )
    .unwrap()
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only activation and action-event origins belong to this source assertion."
)]
fn reprints_keep_their_own_source_printing_and_resolve_named_faces() {
    use sve_engine::game::player::events::EventKind;
    use sve_engine::game::player::observation::{EffectSource, ObservedZone};
    use sve_engine::game::player::{
        Command, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind,
    };
    let mut game = source_reprint(
        json!({"activate":{"cost":{"pp":0_u8},"per_turn":1_u8,"do":[{"buff":{"on":"this","power":2_u8}},{"life":{"who":"self","delta":1_u8}}]}}),
        "field",
    );
    game.advance_player_input();
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("main action")
    };
    let QuestionShape::ActionWindow(actions) = question.shape else {
        panic!("action window")
    };
    let activation_origin = actions
        .intents
        .iter()
        .find_map(|intent| match &intent.kind {
            MainIntentKind::Activate { source, .. } => source.as_ref(),
            _ => None,
        })
        .unwrap();
    assert_eq!(activation_origin.card, "reprint");
    assert_eq!(activation_origin.face, Some(0));
    assert_eq!(activation_origin.line, Some(1));
    assert!(activation_origin.holder.is_some());
    let intent = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Activate { .. }))
        .unwrap();
    let submitted = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: question.token,
            intent: intent.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
    assert_eq!(submitted.kind, SubmissionKind::Applied);
    for viewer in [Viewer::P1, Viewer::P2] {
        let observation = game.observe(viewer).unwrap();
        let card = &observation.players[0]
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap()
            .cards[0]
            .card;
        let state = card.properties.state.as_ref().unwrap();
        let quota = state.quotas.as_ref().unwrap().first().unwrap();
        assert_eq!(quota.source.card, "reprint");
        assert_eq!(quota.source.face, Some(0));
        assert_eq!(quota.source.line, Some(1));
        assert_eq!(quota.remaining, 0);
        let origin = state.effects[0].source.as_ref().unwrap();
        assert_eq!(origin.card, "reprint");
        assert_eq!(origin.face, Some(0));
        assert_eq!(origin.line, Some(1));
        assert_eq!(origin.holder.as_ref(), Some(&card.handle));
        let sources = game
            .events(viewer, None)
            .unwrap()
            .filter_map(|event| match &event.kind {
                EventKind::Action(action) => action.source.as_ref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_ne!(sources, Vec::<&EffectSource>::new());
        for source in sources {
            assert_eq!(source.card, "reprint");
            assert_eq!(source.face, Some(0));
            assert_eq!(source.line, Some(1));
        }
    }
}

#[test]
fn pending_fanfare_keeps_the_actual_reprint_and_named_face_coordinates() {
    let mut game = source_reprint(
        json!({"fanfare":{"do":[{"buff":{"on":"this","power":2_u8}}]}}),
        "hand",
    );
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    for viewer in [Viewer::P1, Viewer::P2] {
        let observation = game.observe(viewer).unwrap();
        assert_eq!(observation.pending.len(), 1);
        let source = &observation.pending.first().unwrap().source;
        assert_eq!(source.card, "reprint");
        assert_eq!(source.face, Some(0));
        assert_eq!(source.line, Some(1));
        assert!(source.holder.is_some());
    }
}
