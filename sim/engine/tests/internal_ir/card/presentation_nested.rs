use super::super::player_pipeline::{answer, question};
use super::nested_play::{Scenario, build};
use super::*;
use alloc::collections::BTreeMap;
use sve_battle_contract::presentation::NoticeKind;
use sve_engine::game::player::events::{
    ActionKind, EventKind, PlayerEvent, PresentationOp, ProvisionalBatch,
};
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// Start the parent ability whose resolution plays a card from a cemetery.
pub(super) fn started(child: &str, enemy: bool, scenario: Scenario) -> Game {
    let (mut game, definition) = build("self", child, enemy, scenario);
    game.advance_player_input();
    game.activate_internal_card("parent", definition, PlayerId { seat: 1 })
        .unwrap();
    game.advance_player_input();
    game
}

/// Hand both seats a presentation and return every batch each has been shown so far.
fn shown(game: &mut Game) -> [Vec<ProvisionalBatch>; 2] {
    SEATS.map(|viewer| {
        game.present_player_events(viewer).unwrap();
        game.presentation(viewer, None)
            .unwrap()
            .filter_map(|entry| match &entry.op {
                PresentationOp::Provisional(batch) => Some(batch.clone()),
                PresentationOp::Promote { .. } | PresentationOp::Cancel { .. } => None,
            })
            .collect()
    })
}

/// How many scopes were promoted and how many were cancelled for this viewer.
fn ended(game: &Game, viewer: Viewer) -> (usize, usize) {
    let steps = game
        .presentation(viewer, None)
        .unwrap()
        .map(|entry| &entry.op)
        .collect::<Vec<_>>();
    (
        steps
            .iter()
            .filter(|step| matches!(step, PresentationOp::Promote { .. }))
            .count(),
        steps
            .iter()
            .filter(|step| matches!(step, PresentationOp::Cancel { .. }))
            .count(),
    )
}

fn plays(kinds: &[&EventKind]) -> Vec<ActionKind> {
    kinds
        .iter()
        .filter_map(|kind| match kind {
            EventKind::Action(action)
                if matches!(action.kind, ActionKind::Play | ActionKind::AbilityPlay) =>
            {
                Some(action.kind)
            }
            EventKind::Action(_)
            | EventKind::Movement { .. }
            | EventKind::Phase(_)
            | EventKind::Result { .. } => None,
        })
        .collect()
}

/// The committed batches that a board handed to this viewer had already contained.
fn covered(game: &Game, viewer: Viewer) -> Vec<Vec<EventKind>> {
    game.event_batches(viewer, None)
        .unwrap()
        .filter(|batch| batch.covered)
        .map(|batch| {
            batch
                .events
                .iter()
                .map(|event| event.kind.clone())
                .collect()
        })
        .collect()
}

fn history(game: &Game, viewer: Viewer) -> Vec<PlayerEvent> {
    game.events(viewer, None).unwrap().cloned().collect()
}

#[test]
fn a_nested_play_is_shown_only_after_the_child_and_its_parent_are_both_played() {
    let mut game = started(
        "unit-spell",
        true,
        Scenario::MandatoryReveal { material: true },
    );
    for (stop, reply) in [
        ("parent target", Answer::Targets(vec![0])),
        ("child cost material", Answer::CostMaterials(vec![0])),
        ("child target", Answer::Targets(vec![0])),
    ] {
        assert_eq!(
            shown(&mut game),
            [vec![], vec![]],
            "at the {stop} question the innermost play can still be undone by rules, \
             so not even its played parent's results are shown"
        );
        assert_eq!(
            answer(&mut game, Viewer::P1, reply).kind,
            SubmissionKind::Applied,
            "the {stop} answer is legal"
        );
    }
    assert!(
        matches!(
            question(&game, Viewer::P1).shape,
            QuestionShape::Decision(DecisionShape::Targets(_))
        ),
        "the child spell asks its resolution choice after it was played and dealt damage"
    );
    let batches = shown(&mut game);
    for seen in &batches {
        let kinds = seen
            .iter()
            .flat_map(|batch| batch.events.iter().map(|event| &event.kind))
            .collect::<Vec<_>>();
        assert_eq!(
            plays(&kinds),
            vec![ActionKind::AbilityPlay, ActionKind::Play],
            "the parent ability and then the child card were played, in rules order: {kinds:?}"
        );
        let child = kinds
            .iter()
            .position(
                |kind| matches!(kind, EventKind::Action(action) if action.kind == ActionKind::Play),
            )
            .unwrap();
        assert!(
            kinds[child..]
                .iter()
                .any(|kind| matches!(kind, EventKind::Action(action) if action.kind == ActionKind::Damage)),
            "the child's completed damage follows its play: {kinds:?}"
        );
        assert!(
            !kinds.iter().any(
                |kind| matches!(kind, EventKind::Action(action) if action.kind == ActionKind::Life)
            ),
            "the parent's life gain was already on the boards handed at the earlier stops, \
             so it is not shown now: {kinds:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the resolution choice completes the child and then the parent"
    );
    assert!(
        matches!(game.input(Viewer::P1), Input::Question(question)
            if matches!(question.shape, QuestionShape::ActionWindow(_))),
        "the whole nested play has finished"
    );
    for (viewer, seen) in SEATS.into_iter().zip(&batches) {
        assert_eq!(
            ended(&game, viewer),
            (seen.len(), 0),
            "every shown scope of both layers is promoted and none is cancelled"
        );
        let linked = history(&game, viewer)
            .iter()
            .filter(|event| event.presentation.is_some())
            .count();
        assert_eq!(
            linked,
            seen.iter().map(|batch| batch.events.len()).sum::<usize>(),
            "committed events refer to exactly the shown occurrences"
        );
        let covered = covered(&game, viewer);
        assert!(
            matches!(covered.as_slice(), [events]
                if matches!(events.as_slice(), [EventKind::Action(action)]
                    if action.kind == ActionKind::Life
                        && action.numbers.first().is_some_and(|number| number.amount == 1))),
            "only the parent's first life gain commits as something an earlier board already \
             contained; costs and plays keep their first presentation: {covered:?}"
        );
    }
}

/// A parent ability that pauses for a choice after it was played, then plays a follower from
/// the cemetery into a field that is already full.
fn paused_parent_with_a_full_field() -> Game {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP01-041"].clone();
    let entry = template["abilities"][0].clone();
    let mut parent = entry.clone();
    parent["activate"] = json!({
        "cost":{"pp":1},
        "targets":[{"bind":"selected","from":{"who":"self","zone":"cemetery","name":"unit-follower"},"n":1}],
        "do":[
            {"life":{"who":"self","delta":1}},
            {"choose":{"bind":"pause","from":{"who":"self","zone":"field","kind":"follower"},"n":1}},
            {"look":{"top":{"who":"self","n":1},"bind":"seen"}},
            {"play_card":{"ref":"selected","cost":0}},
            {"life":{"who":"self","delta":10}}
        ]
    });
    template["abilities"] = json!([parent]);
    let mut follower = template.clone();
    let mut passive = entry;
    passive.as_object_mut().unwrap().remove("activate");
    passive["fanfare"] = json!({"do":[{"life":{"who":"self","delta":5}}]});
    follower["abilities"] = json!([passive]);
    tree["cards"] = json!({"unit-amulet":template,"unit-follower":follower});
    let programs = lowerer().lower(&tree).unwrap();

    let mut initial = setup(1, json!([{"id":"deck-top","card":"unit-spell"}]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"parent","card":"unit-amulet"})];
    field.extend((0..4).map(|slot| json!({"id":format!("filler{slot}"),"card":"unit-follower"})));
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"child","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "nested-card",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.advance_player_input();
    game.activate_internal_card(
        "parent",
        programs["unit-amulet"].card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn a_rules_cancellation_of_the_child_never_withdraws_what_the_parent_already_showed() {
    let mut game = paused_parent_with_a_full_field();
    assert_eq!(
        shown(&mut game),
        [vec![], vec![]],
        "the parent has not been played at its target question"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the cemetery follower is a legal target"
    );
    let batches = shown(&mut game);
    for seen in &batches {
        let kinds = seen
            .iter()
            .flat_map(|batch| batch.events.iter().map(|event| &event.kind))
            .collect::<Vec<_>>();
        assert_eq!(
            plays(&kinds),
            vec![ActionKind::AbilityPlay],
            "the played parent and its completed life gain are shown before the pause: {kinds:?}"
        );
    }
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "rules cancel only the child entry into the full field; the parent still resolves"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["child"]),
        "the child returned to where it was before its cancelled entry"
    );
    for (viewer, seen) in SEATS.into_iter().zip(&batches) {
        assert_eq!(
            ended(&game, viewer),
            (seen.len(), 0),
            "what the parent showed is promoted; the child's rules cancellation is not a cancel"
        );
        let events = history(&game, viewer);
        let kinds = events.iter().map(|event| &event.kind).collect::<Vec<_>>();
        assert_eq!(
            plays(&kinds),
            vec![ActionKind::AbilityPlay],
            "the undone child entry left no history: {kinds:?}"
        );
    }
}

#[test]
fn a_cancelled_child_preserves_the_parents_look_in_the_same_submission() {
    let mut game = paused_parent_with_a_full_field();
    let refs = &board_fixture::fixture().unwrap().references;
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied
    );
    assert_eq!(game.take_battle_notices(Viewer::P1, refs).unwrap(), []);
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["child"])
    );
    let notices = game.take_battle_notices(Viewer::P1, refs).unwrap();
    assert!(
        matches!(notices.as_slice(), [notice] if notice.kind == NoticeKind::Look
        && matches!(notice.cards.as_slice(), [shown] if shown.card.printing_id == "printing-spell"))
    );
    assert_eq!(game.take_battle_notices(Viewer::P2, refs).unwrap(), []);
}

/// A parent ability that plays a follower and then a spell from the cemetery. The spell asks
/// for its target before it is played, after the follower's whole play has completed.
fn parent_playing_a_follower_then_a_spell() -> Game {
    let mut tree = fanfare::expanded::tree().clone();
    let mut template = tree["cards"]["BP01-041"].clone();
    let entry = template["abilities"][0].clone();
    let mut parent = entry.clone();
    parent["activate"] = json!({
        "cost":{"pp":1},
        "targets":[
            {"bind":"first","from":{"who":"self","zone":"cemetery","name":"unit-follower"},"n":1},
            {"bind":"second","from":{"who":"self","zone":"cemetery","name":"unit-spell"},"n":1}
        ],
        "do":[
            {"play_card":{"ref":"first","cost":0}},
            {"play_card":{"ref":"second","cost":0}}
        ]
    });
    template["abilities"] = json!([parent]);
    let mut follower = template.clone();
    follower["abilities"] = json!([]);
    let mut spell = template.clone();
    let mut cast = entry;
    cast.as_object_mut().unwrap().remove("activate");
    cast["spell"] = json!({
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"damage":{"to":"enemy","n":1}},{"life":{"who":"self","delta":2}}]
    });
    spell["abilities"] = json!([cast]);
    tree["cards"] = json!({"unit-amulet":template,"unit-spell":spell,"unit-follower":follower});
    let programs = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap();

    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"parent","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"child","card":"unit-follower"},
        {"id":"leaf","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "nested-card",
    )
    .unwrap();
    for (number, program) in &programs {
        game.load_internal_card(number, program).unwrap();
    }
    game.advance_player_input();
    game.activate_internal_card(
        "parent",
        programs["unit-amulet"].card().abilities[0].node.id,
        PlayerId { seat: 1 },
    )
    .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn a_nested_followers_own_play_keeps_its_first_presentation_and_only_its_entry_is_history() {
    let mut game = parent_playing_a_follower_then_a_spell();
    for chosen in ["follower", "spell"] {
        assert_eq!(
            answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
            SubmissionKind::Applied,
            "the {chosen} in the cemetery is a legal target of the parent"
        );
    }
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["parent", "child"]),
        "the follower's whole play has completed before the spell asks for its target"
    );
    assert_eq!(
        shown(&mut game),
        [vec![], vec![]],
        "the spell can still be undone by rules, so nothing is shown at its question"
    );
    assert_eq!(
        answer(&mut game, Viewer::P1, Answer::Targets(vec![0])).kind,
        SubmissionKind::Applied,
        "the spell and then the parent resolve completely"
    );
    for viewer in SEATS {
        let covered = covered(&game, viewer);
        let [entered, resolved] = covered.as_slice() else {
            panic!("two batches were already on the board handed at the stop: {covered:?}");
        };
        assert!(
            matches!(entered.as_slice(), [EventKind::Movement { after, .. }]
                if after.zone == Some(ObservedZone::Field))
                && matches!(resolved.as_slice(), [EventKind::Action(action)]
                    if action.kind == ActionKind::Resolve),
            "only the follower's entry and resolution are history that the handed board \
             already contained; its move to the resolution zone and its play, like the \
             parent's play, keep their first presentation: {covered:?}"
        );
    }
}
