use super::super::player_pipeline::{answer, question};
use super::nested_play::{Scenario, build};
use super::*;
use sve_engine::game::player::events::{
    ActionKind, EventKind, PlayerEvent, PresentationOp, ProvisionalBatch,
};
use sve_engine::game::player::{DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};

const SEATS: [Viewer; 2] = [Viewer::P1, Viewer::P2];

/// Start the parent ability whose resolution plays a card from a cemetery.
fn started(child: &str, enemy: bool, scenario: Scenario) -> Game {
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

    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":3,"max":3});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"parent","card":"unit-amulet"})];
    field.extend((0..4).map(|slot| json!({"id":format!("filler{slot}"),"card":"unit-follower"})));
    initial["players"]["P1"]["zones"]["field"] = json!(field);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"child","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
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
