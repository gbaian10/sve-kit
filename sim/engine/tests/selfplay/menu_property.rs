//! Random main-action checks using the self-play decks and progression.
//!
//! ```bash
//! SVE_SELFPLAY_GAMES=100 SVE_SELFPLAY_SEED=0 python3 .github/ci/rust_tests.py \
//!   test --locked -p sve-engine --lib menu_property::many_games -- --ignored --nocapture
//! ```

#![expect(
    clippy::unwrap_used,
    reason = "Missing authority state or malformed configuration should fail the test."
)]
#![expect(
    clippy::print_stderr,
    reason = "The opt-in runner reports statistics and replay details through the test harness."
)]

use super::{AttackTarget, Game, IntentKind, MainActionAnswer, MainIntentKind, other, seat};
use crate::EngineFailure;
use crate::game::internal::CardPlay;
use crate::game::player::{
    Answer, Command, DecisionShape, EvolutionQuestion, Input, MainActions, MainIntent, Question,
    QuestionShape, SelectionQuestion, SubmissionKind, Viewer,
};
use crate::game::{repository, selfplay};
use core::iter::once;
use core::ops::ControlFlow;
use rand::SeedableRng as _;
use rand_chacha::ChaCha8Rng;
use serde_json::{Value, json};
use std::{env, time::Instant};

mod menu_answers;

// Settlement can require more answers than declaration alone. Limit depth separately for loops.
const ANSWER_BUDGET: usize = 256;
const BRANCH_DEPTH: u64 = 64;
// Remove this rollback-loop signature when #2480 is fixed.
const REPEATED_PENDING: [&str; 3] = ["BP11-078", "CP04-098", "BP22-040"];

#[derive(Debug)]
enum Blocked {
    Rejected(String),
    Fault(String),
    SearchLimit,
}

fn fault(game: &Game) -> Blocked {
    Blocked::Fault(game.referee_fault().map_or_else(
        || "failed to project player input".into(),
        ToString::to_string,
    ))
}

#[derive(Clone, Copy)]
enum Goal {
    Declare,
    Resolve,
}

fn declaration_complete(game: &Game) -> bool {
    game.concealed_declaration().is_none() && game.evolution_actor().is_none()
}

fn choice_label(question: &Question, answer: &Command) -> String {
    if let (
        QuestionShape::Decision(DecisionShape::Pending(options)),
        Command::Answer {
            answer: Answer::Pending(handle),
            ..
        },
    ) = (&question.shape, answer)
        && let Some(pending) = options.iter().find(|option| option.handle == *handle)
    {
        return format!("pending {} line {}", pending.card, pending.line);
    }
    format!("{answer:?}")
}

fn choice_domain(question: &Question) -> String {
    let selection: &SelectionQuestion = match &question.shape {
        QuestionShape::Decision(
            DecisionShape::Targets(selection)
            | DecisionShape::CostMaterials(selection)
            | DecisionShape::Discard {
                cards: selection, ..
            }
            | DecisionShape::ResourceEntryPayment {
                materials: selection,
                ..
            },
        )
        | QuestionShape::Evolution(
            EvolutionQuestion::Materials(selection) | EvolutionQuestion::Discard(selection),
        ) => selection,
        QuestionShape::Decision(_)
        | QuestionShape::Evolution(_)
        | QuestionShape::Opening(_)
        | QuestionShape::Closing(_)
        | QuestionShape::ActionWindow(_) => return format!("{:?}", question.purpose),
    };
    let candidates: Vec<_> = selection
        .members
        .iter()
        .map(|member| {
            (
                member.handle,
                member.card.as_deref(),
                member.player,
                member.original_cost,
            )
        })
        .collect();
    format!(
        "{:?} {:?}, candidates {candidates:?}",
        question.purpose, selection.cardinality
    )
}

fn rejected_answer(game: &Game, input: Input, kind: SubmissionKind) -> Blocked {
    if kind == SubmissionKind::Reverted
        && let Input::Question(question) = selfplay::active_input(game, input)
        && let QuestionShape::Decision(DecisionShape::Pending(pending)) = &question.shape
        && let Some(reopened) = pending
            .iter()
            .find(|choice| REPEATED_PENDING.contains(&choice.card.as_str()))
    {
        return Blocked::Rejected(format!("reopened pending {}", reopened.card));
    }
    Blocked::Rejected(format!("{kind:?}"))
}

fn follow(
    game: Game,
    input: Input,
    goal: Goal,
    budget: &mut usize,
    depth: u64,
) -> Result<(), Blocked> {
    let question = match selfplay::active_input(&game, input) {
        Input::Finished { .. } => return Ok(()),
        Input::Question(question) => question,
        Input::EngineFault | Input::Waiting(_) => return Err(fault(&game)),
    };
    if matches!(question.shape, QuestionShape::ActionWindow(_))
        || (matches!(goal, Goal::Declare) && declaration_complete(&game))
    {
        return Ok(());
    }
    if depth == BRANCH_DEPTH {
        return Err(Blocked::SearchLimit);
    }

    let mut rng = ChaCha8Rng::seed_from_u64(depth);
    let preferred = if matches!(
        question.shape,
        QuestionShape::Decision(DecisionShape::Quick { .. })
    ) {
        Some(Command::Answer {
            token: question.token,
            answer: Answer::Quick(None),
        })
    } else {
        selfplay::answers::choose(&question, &mut rng)
            .ok()
            .map(|(command, _)| command)
    };
    let mut answers = preferred
        .iter()
        .cloned()
        .chain(
            menu_answers::alternatives(&question)
                .filter(|answer| preferred.as_ref() != Some(answer)),
        )
        .peekable();
    let mut failure =
        Blocked::Rejected(format!("{} has no usable answer", choice_domain(&question)));
    let mut game = Some(game);
    while let Some(answer) = answers.next() {
        let Some(remaining) = budget.checked_sub(1) else {
            return Err(Blocked::SearchLimit);
        };
        *budget = remaining;
        let mut branch = if answers.peek().is_none() {
            game.take().unwrap()
        } else {
            game.as_ref().unwrap().clone()
        };
        let setup_complete = declaration_complete(&branch);
        let context = choice_label(&question, &answer);
        let submission = branch.submit_player(question.actor, answer);
        let result = match submission.kind {
            // A separate automatic occurrence can be cancelled after the main action was played.
            kind @ (SubmissionKind::Applied | SubmissionKind::Cancelled)
                if kind == SubmissionKind::Applied || setup_complete =>
            {
                follow(
                    branch,
                    submission.input,
                    goal,
                    budget,
                    depth.saturating_add(1),
                )
            }
            SubmissionKind::EngineFault => Err(fault(&branch)),
            other @ (SubmissionKind::Applied
            | SubmissionKind::Cancelled
            | SubmissionKind::Rejected
            | SubmissionKind::Reverted) => Err(rejected_answer(&branch, submission.input, other)),
        };
        match result {
            Ok(()) => return Ok(()),
            Err(Blocked::SearchLimit) => failure = Blocked::SearchLimit,
            Err(Blocked::Fault(message)) if known_engine_issue(&message).is_some() => {
                return Err(Blocked::Fault(format!("{context}: {message}")));
            }
            Err(Blocked::Fault(message)) if !matches!(failure, Blocked::SearchLimit) => {
                failure = Blocked::Fault(format!("{context}: {message}"));
            }
            Err(Blocked::Rejected(message)) if matches!(failure, Blocked::Rejected(_)) => {
                failure = Blocked::Rejected(format!("{context}: {message}"));
            }
            Err(Blocked::Rejected(_) | Blocked::Fault(_)) => {}
        }
    }
    Err(failure)
}

fn option_name(game: &Game, actor: Viewer, intent: &MainIntentKind) -> String {
    let describe = |card: &super::CardObservation| {
        let Ok(id) = game.resolve_player_object(actor, &card.handle) else {
            return format!("{} (invalid handle)", card.physical.card);
        };
        let object = game.object(id).unwrap();
        format!("{} ({id}, {})", card.physical.card, object.zone)
    };
    match intent {
        MainIntentKind::Play { card } => format!("play {}", describe(card)),
        MainIntentKind::Activate {
            holder,
            line,
            source,
        } => format!(
            "activate {} line {line} origin {:?}",
            describe(holder),
            source.as_ref().map(|origin| &origin.card)
        ),
        MainIntentKind::Evolve { holder, line, .. } => {
            format!("evolve {} line {line}", describe(holder))
        }
        MainIntentKind::Attack { attacker, .. } => format!("attack {}", describe(attacker)),
        MainIntentKind::EndPhase => "end main".into(),
    }
}

fn resolve_option(game: &Game, question: &Question, intent: &MainIntent) -> Result<(), Blocked> {
    let answers = match &intent.kind {
        MainIntentKind::Attack { targets, .. } => targets
            .iter()
            .cloned()
            .map(MainActionAnswer::Attack)
            .collect(),
        MainIntentKind::Play { .. }
        | MainIntentKind::Activate { .. }
        | MainIntentKind::Evolve { .. }
        | MainIntentKind::EndPhase => vec![MainActionAnswer::None],
    };
    let mut failure = Blocked::Rejected("empty action parameter domain".into());
    let mut budget = ANSWER_BUDGET;
    for answer in answers {
        let mut branch = game.clone();
        let submission = branch.submit_player(
            question.actor,
            Command::MainAction {
                token: question.token,
                intent: intent.id.clone(),
                answer,
            },
        );
        failure = match submission.kind {
            SubmissionKind::Applied => {
                match follow(branch, submission.input, Goal::Resolve, &mut budget, 0) {
                    Ok(()) => return Ok(()),
                    Err(error) => error,
                }
            }
            SubmissionKind::EngineFault => fault(&branch),
            other @ (SubmissionKind::Rejected
            | SubmissionKind::Reverted
            | SubmissionKind::Cancelled) => Blocked::Rejected(format!("declaration {other:?}")),
        };
    }
    Err(failure)
}

fn check_attack_omissions(
    game: &Game,
    actor: Viewer,
    actions: &MainActions,
) -> Result<(), (String, Blocked)> {
    let mine = seat(actor);
    let opponent = other(mine);
    let enemy = if actor == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    };
    let attacks = once((
        format!("{opponent}.leader"),
        Some(AttackTarget::Leader(enemy)),
    ))
    .chain(game.zone_ids(opponent, "field").into_iter().map(|id| {
        let target = game
            .observed_object_handle(actor, &id)
            .ok()
            .map(AttackTarget::Card);
        (id, target)
    }))
    .collect::<Vec<_>>();
    for id in game.zone_ids(mine, "field") {
        let source = game.observed_object_handle(actor, &id).ok();
        let card = &game
            .object(&id)
            .map_err(|error| (id.clone(), Blocked::Fault(error.to_string())))?
            .card;
        for (target, answer) in &attacks {
            let listed = actions.intents.iter().any(|intent| {
                matches!(
                    &intent.kind,
                    MainIntentKind::Attack { attacker, targets }
                        if source.as_ref() == Some(&attacker.handle)
                        && answer.as_ref().is_some_and(|answer| targets.contains(answer))
                )
            });
            if !listed {
                let name = format!("attack {card} ({id}) -> {target}");
                verify_absent(game, actor, name, |branch| {
                    branch.apply_attack(&id, target, None)
                })?;
            }
        }
    }
    Ok(())
}

/// Every candidate is addressed independently of the menu's filtering. A listed one
/// already satisfies this direction, so only missing candidates need a formal attempt.
fn check_omissions(
    game: &Game,
    actor: Viewer,
    actions: &MainActions,
) -> Result<(), (String, Blocked)> {
    check_attack_omissions(game, actor, actions)?;
    let mine = seat(actor);
    if !actions
        .intents
        .iter()
        .any(|intent| intent.id.kind == IntentKind::EndPhase)
    {
        verify_absent(game, actor, "end main".into(), |branch| {
            // The obligation is checked by the formal action dispatcher before starting end.
            if branch.attack_required()? {
                return Ok(false);
            }
            branch.start_end().map(|()| true)
        })?;
    }
    for zone in ["hand", "ex", "cemetery", "banish", "field"] {
        for id in game.zone_ids(mine, zone) {
            if !game.player_knows_object(actor, &id) {
                continue;
            }
            let locating = |error: EngineFailure| {
                (format!("{id} ({zone})"), Blocked::Fault(error.to_string()))
            };
            let source = game.observed_object_handle(actor, &id).map_err(locating)?;
            let card = &game.object(&id).map_err(locating)?.card;
            let listed = |kind| {
                actions.intents.iter().any(|intent| {
                    intent.id.source.as_ref() == Some(&source) && intent.id.kind == kind
                })
            };
            if zone != "field" && !listed(IntentKind::Play) {
                verify_absent(
                    game,
                    actor,
                    format!("play {card} ({id}, {zone})"),
                    |branch| {
                        branch
                            .play_main_card(&id, actor.player(), &mut false)
                            .map(|play| play != CardPlay::Cancelled)
                    },
                )?;
            }
            for ordinal in 0.. {
                if listed(IntentKind::Activate(ordinal)) {
                    continue;
                }
                if !verify_absent(
                    game,
                    actor,
                    format!("activate {card} ({id}, {zone}) option {ordinal}"),
                    |branch| {
                        branch
                            .activate_main_option(&id, ordinal, actor, &mut false)
                            .map(|play| play != CardPlay::Cancelled)
                    },
                )? {
                    break;
                }
            }
            if zone == "field" {
                for ordinal in 0.. {
                    if listed(IntentKind::Evolve(ordinal)) {
                        continue;
                    }
                    if !verify_absent(
                        game,
                        actor,
                        format!("evolve {card} ({id}) option {ordinal}"),
                        |branch| branch.begin_main_evolution(&id, ordinal, actor),
                    )? {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

// The public command rejects unknown menu IDs before dispatch. Enter the same formal
// procedures directly, then use player answers. False marks an exhausted ordinal domain.
fn verify_absent(
    game: &Game,
    actor: Viewer,
    name: String,
    apply: impl FnOnce(&mut Game) -> crate::Result<bool>,
) -> Result<bool, (String, Blocked)> {
    let mut branch = game.clone();
    branch.node = branch.rule_node("player-rule-");
    branch.state.next_decision = branch.state.next_decision.saturating_add(1);
    match apply(&mut branch) {
        Err(EngineFailure::Invalid(message))
            if matches!(
                message.as_str(),
                "retired main activation option" | "retired evolution option"
            ) =>
        {
            Ok(false)
        }
        Ok(false) | Err(EngineFailure::IllegalPlay(_)) => Ok(true),
        Err(EngineFailure::Unsupported(message))
            if matches!(
                message.as_str(),
                "internal D1 does not support card play holder, permission or timing"
                    | "internal D1 does not support field token play origin"
            ) =>
        {
            Ok(true)
        }
        Err(error) => Err((name, Blocked::Fault(error.to_string()))),
        Ok(true) => {
            let result = if declaration_complete(&branch) {
                Ok(())
            } else {
                branch.advance_player_input();
                let input = branch.input(actor);
                let mut budget = ANSWER_BUDGET;
                follow(branch, input, Goal::Declare, &mut budget, 0)
            };
            match result {
                Err(Blocked::Rejected(_)) => Ok(true),
                Err(error) => Err((name, error)),
                Ok(()) => Err((
                    name,
                    Blocked::Rejected(
                        "formal declaration accepted but absent from the menu".into(),
                    ),
                )),
            }
        }
    }
}

fn board(game: &Game) -> Value {
    let cards = game.state.objects.values()
        .filter(|object| !matches!(object.zone.as_str(), "deck" | "void"))
        .map(|object| json!({
            "id":object.id, "card":object.card, "zone":object.zone, "seat":object.controller,
            "acted":object.state["acted"] == true, "entered":object.state["entered_this_turn"] == true,
            "face":object.state["face"], "counters":object.state["counters"],
        })).collect::<Vec<_>>();
    let resources = ["P1", "P2"].map(|seat| {
        let player = game.player(seat).unwrap();
        json!({"seat":seat,"life":player.leader["life"],"pp":player.pp,"ep":player.ep,"sep":player.sep,"deck":game.zone_ids(seat, "deck").len()})
    });
    json!({"cards":cards, "resources":resources})
}

fn check_window(game: &Game, question: &Question) -> Result<usize, (String, Blocked)> {
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        return Ok(0);
    };
    for intent in &actions.intents {
        resolve_option(game, question, intent)
            .map_err(|failure| (option_name(game, question.actor, &intent.kind), failure))?;
    }
    check_omissions(game, question.actor, actions)?;
    Ok(actions.intents.len())
}

fn known_engine_issue(error: &str) -> Option<u16> {
    // Remove each temporary diagnostic exception when the corresponding issue is fixed.
    [
        (2474, "Trigger icon requires verified facts"),
        (2475, "name selection exceeds both fields"),
        (
            2476,
            "search optional replacement needs two executable destinations",
        ),
        (2477, "card source mapping mismatch"),
        (2478, "evolution outside fixed Add numeric layers"),
        (
            2479,
            "exchange material has unsupported movement or permission",
        ),
        (
            2481,
            "prototype continuation cannot enter the typed player interface",
        ),
        (2482, "event card kind"),
    ]
    .into_iter()
    .find_map(|(issue, diagnostic)| error.contains(diagnostic).then_some(issue))
}

fn known_issue(failure: &Value) -> Option<u16> {
    let error = failure.get("property").unwrap_or(failure)["error"]
        .as_str()
        .unwrap_or_default();
    known_engine_issue(error).or_else(|| {
        if REPEATED_PENDING.iter().any(|card| {
            error.contains(&format!("pending {card} line"))
                && error.contains(&format!("reopened pending {card}"))
        }) {
            return Some(2480);
        }
        let repeats = failure["context"].as_array().is_some_and(|context| {
            context.len() == 8
                && context
                    .iter()
                    .zip(context.iter().skip(2))
                    .all(|(first, second)| first == second)
                && REPEATED_PENDING.iter().any(|card| {
                    context
                        .iter()
                        .any(|step| step.as_str().is_some_and(|step| step.contains(card)))
                })
        });
        (failure["kind"] == "step_limit" && repeats).then_some(2480)
    })
}

#[derive(Clone, Copy)]
enum GameResult {
    Checked,
    Skipped,
    Failed,
}

fn report_game(failure: Option<Value>, seed: u64, reprints: bool) -> GameResult {
    let Some(mut failure) = failure else {
        return GameResult::Checked;
    };
    failure["reprints"] = json!(reprints);
    if failure["kind"] == "menu_property" {
        let property = serde_json::from_str::<Value>(failure["error"].as_str().unwrap()).unwrap();
        failure.as_object_mut().unwrap().remove("error");
        failure["property"] = property;
    }
    if let Some(issue) = known_issue(&failure) {
        eprintln!("menu property seed {seed} skipped: #{issue}");
        return GameResult::Skipped;
    }
    eprintln!("menu property failure: {failure}");
    GameResult::Failed
}

fn check_games(
    seeds: &(impl Iterator<Item = u64> + Clone + Send),
    reprints: bool,
    window_limit: Option<usize>,
) {
    let started = Instant::now();
    let selected = repository::selection_with_rules();
    let pools = selfplay::pools(selected, reprints);
    assert!(!pools.is_empty(), "no legal construction pool");
    let reports = selfplay::run_seeds(seeds, |seed| {
        let mut windows = 0_usize;
        let mut options = 0_usize;
        let report = selfplay::run_match(selected, &pools, seed, |game, question, step| {
            if !matches!(question.shape, QuestionShape::ActionWindow(_)) {
                return Ok(ControlFlow::Continue(()));
            }
            windows = windows.saturating_add(1);
            options = options.saturating_add(check_window(game, question).map_err(
                |(option, error)| {
                    let error = match error {
                        Blocked::Rejected(message) | Blocked::Fault(message) => message,
                        Blocked::SearchLimit => "answer search exhausted its budget".into(),
                    };
                    (
                        "menu_property",
                        json!({"step":step,"option":option,"error":error,"board":board(game)})
                            .to_string(),
                    )
                },
            )?);
            Ok(if window_limit == Some(windows) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            })
        });
        eprintln!("menu property seed {seed}: {windows} windows checked");
        (
            windows,
            options,
            report_game(report.failure, seed, reprints),
        )
    });
    let games = reports.len();
    let mut windows = 0_usize;
    let mut options = 0_usize;
    let mut failures = 0_usize;
    let mut skipped = 0_usize;
    for (_, (checked, witnessed, result)) in reports {
        windows = windows.saturating_add(checked);
        options = options.saturating_add(witnessed);
        match result {
            GameResult::Checked => {}
            GameResult::Skipped => skipped = skipped.saturating_add(1),
            GameResult::Failed => failures = failures.saturating_add(1),
        }
    }
    eprintln!(
        "menu property: {games} games, {windows} windows, {options} options in completed windows, {skipped} known interruptions, {failures} failures, {:.2}s",
        started.elapsed().as_secs_f64()
    );
    assert!(
        options > 0 || (games > 0 && skipped == games),
        "no main window was fully verified"
    );
    assert_eq!(failures, 0, "main-action property failures");
}

#[test]
fn fixed_seeds() {
    check_games(&[2, 3].into_iter(), false, Some(12));
}

#[test]
#[ignore = "opt-in main-action checks across many random games"]
fn many_games() {
    let first: u64 = env::var("SVE_SELFPLAY_SEED")
        .map_or(Ok(0), |value| value.parse())
        .unwrap();
    let count: u64 = env::var("SVE_SELFPLAY_GAMES")
        .map_or(Ok(100), |value| value.parse())
        .unwrap();
    assert!(count > 0, "SVE_SELFPLAY_GAMES must be positive");
    check_games(
        &(first..=first.checked_add(count.saturating_sub(1)).unwrap()),
        env::var_os("SVE_SELFPLAY_REPRINTS").is_some(),
        None,
    );
}
