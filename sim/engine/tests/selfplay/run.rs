//! Seeded matches on the current repository selection through the native player pipeline.
//!
//! This is the driver of the main-action property checks, which need the authority's own
//! state. Batch self-play, its invariants and its reports live in `sim/selfplay`, above the
//! battle host. Each seat uses base construction rules, independent of any format or ban list.

#![expect(
    clippy::unwrap_used,
    reason = "A missing fixture must fail the tests that drive these matches."
)]

use super::Game;
use super::player::{DeckConstruction, Input, PlayerDeck, Question, Viewer};
use crate::catalog::SelectedEffects;
use crate::ir::{Cardinality, KeywordId, printed_keywords};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use core::num::NonZeroUsize;
use core::ops::ControlFlow;
use core::panic::AssertUnwindSafe;
use rand::seq::{IndexedRandom as _, SliceRandom as _};
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha8Rng;
use serde_json::{Value, json};
use std::panic::catch_unwind;
use std::thread;

pub(super) mod answers;

#[derive(Clone, Default)]
struct Pool {
    main: Vec<Printing>,
    evolve: Vec<Printing>,
}

#[derive(Clone)]
struct Printing {
    number: String,
    name: String,
    start_amulet: bool,
}

pub(super) struct DeckPool {
    basis: DeckConstruction,
    cards: Pool,
    mandatory_amulet: bool,
}

pub(super) fn pools(selected: &SelectedEffects, reprints: bool) -> Vec<DeckPool> {
    let mut neutral = Pool::default();
    let mut classes = BTreeMap::<String, Pool>::new();
    let mut titles = BTreeMap::<String, Pool>::new();
    for (number, candidate) in &selected.current {
        if !reprints && candidate.card_no != *number {
            continue;
        }
        let face = selected.catalogue.face(number, 0).unwrap();
        let Some(kind) = face.get("card_type").and_then(Value::as_str) else {
            continue;
        };
        if !["フォロワー", "スペル", "アミュレット"]
            .iter()
            .any(|base| kind.contains(base))
            || kind.contains("トークン")
        {
            continue;
        }
        let printing = Printing {
            number: number.clone(),
            name: selected.catalogue.rules_label(number, 0).unwrap(),
            start_amulet: candidate.program.card().abilities.iter().any(|ability| {
                printed_keywords::card_declaration(&ability.node) == Some(KeywordId::StartAmulet)
            }),
        };
        let evolve = kind.contains("エボルヴ") || kind.contains("アドバンス");
        let add = |pool: &mut Pool| {
            if evolve {
                &mut pool.evolve
            } else {
                &mut pool.main
            }
            .push(printing.clone());
        };
        if let Some(class) = face.get("card_class").and_then(Value::as_str) {
            if class == "ニュートラル" {
                add(&mut neutral);
            } else {
                add(classes.entry(class.into()).or_default());
            }
        }
        if let Some(title) = face.get("title").and_then(Value::as_str)
            && let Ok(Some(code)) = selected.catalogue.rule_bindings.title_code("", title)
        {
            add(titles.entry(code).or_default());
        }
    }
    classes
        .into_iter()
        .map(|(class, mut cards)| {
            cards.main.extend(neutral.main.iter().cloned());
            cards.evolve.extend(neutral.evolve.iter().cloned());
            DeckPool {
                basis: DeckConstruction::Class(class),
                cards,
                mandatory_amulet: false,
            }
        })
        .chain(titles.into_iter().map(|(title, cards)| {
            let mandatory_amulet = selected
                .catalogue
                .rule_bindings
                .capability(&title, "opening_start_amulet");
            DeckPool {
                basis: DeckConstruction::Title(title),
                cards,
                mandatory_amulet,
            }
        }))
        .filter(|pool| {
            pool.cards
                .main
                .iter()
                .map(|card| &card.name)
                .collect::<BTreeSet<_>>()
                .len()
                >= 40
                && (!pool.mandatory_amulet || pool.cards.main.iter().any(|card| card.start_amulet))
        })
        .collect()
}

fn deck(pool: &DeckPool, rng: &mut ChaCha8Rng) -> PlayerDeck {
    let mut main = Vec::new();
    let mut names = BTreeSet::new();
    let amulet = pool.mandatory_amulet.then(|| {
        let candidates: Vec<_> = pool
            .cards
            .main
            .iter()
            .filter(|card| card.start_amulet)
            .collect();
        *candidates.choose(rng).unwrap()
    });
    if let Some(amulet) = amulet {
        main.push(amulet.number.clone());
        names.insert(&amulet.name);
    }
    let mut candidates: Vec<_> = pool.cards.main.iter().collect();
    candidates.shuffle(rng);
    // One printing per construction name also satisfies printed singleton restrictions.
    for card in candidates {
        if main.len() == 40 {
            break;
        }
        if card.start_amulet
            && main.iter().any(|number| {
                pool.cards
                    .main
                    .iter()
                    .any(|previous| previous.start_amulet && previous.number == *number)
            })
        {
            continue;
        }
        if names.insert(&card.name) {
            main.push(card.number.clone());
        }
    }
    let mut forms: Vec<_> = pool.cards.evolve.iter().collect();
    forms.shuffle(rng);
    forms.sort_by_key(|card| !names.contains(&card.name));
    let mut evolve_names = BTreeSet::new();
    let evolve = forms
        .into_iter()
        .filter(|card| evolve_names.insert(&card.name))
        .take(10)
        .map(|card| card.number.clone())
        .collect();
    PlayerDeck {
        construction: pool.basis.clone(),
        main,
        evolve,
    }
}

fn sample<'items, T>(items: &'items [T], rng: &mut ChaCha8Rng) -> Result<&'items T, &'static str> {
    items.choose(rng).ok_or("empty choice domain")
}

fn choice_count(
    bounds: Cardinality,
    available: usize,
    rng: &mut ChaCha8Rng,
) -> Result<usize, &'static str> {
    let minimum = usize::try_from(bounds.minimum.0).map_err(|_error| "minimum out of range")?;
    let maximum = bounds.maximum.map_or(available, |max| {
        usize::try_from(max.0).unwrap_or(usize::MAX).min(available)
    });
    if minimum > maximum {
        return Err("infeasible choice count");
    }
    Ok(rng.random_range(minimum..=maximum))
}

const STEP_LIMIT: usize = 2_000;

#[derive(Default)]
struct Trace {
    step: usize,
    turn: Value,
    context: VecDeque<String>,
}

/// How a match stopped without a failure.
enum MatchEnd {
    /// The rules ended it, or the inspector had seen enough.
    Completed,
    /// The decks need verified Trigger icons that the current facts lack.
    MissingTriggerIcons,
}

type Inspect = Result<ControlFlow<()>, (&'static str, String)>;

fn play(
    selected: &SelectedEffects,
    decks: [PlayerDeck; 2],
    seed: u64,
    rng: &mut ChaCha8Rng,
    trace: &mut Trace,
    mut inspect: impl FnMut(&Game, &Question, usize) -> Inspect,
) -> Result<MatchEnd, (&'static str, String)> {
    use super::player::{AdmissionGapReason, StrictStartError, SubmissionKind};
    let first = if rng.random() { Viewer::P1 } else { Viewer::P2 };
    let mut game = match Game::start_player_game(selected.clone(), decks, first, &seed.to_string())
    {
        Ok(game) => game,
        Err(StrictStartError::Initialization(fault)) => return Err(diagnostic(&fault)),
        Err(StrictStartError::Admission(report)) => {
            let first_gaps = report.gaps(Viewer::P1);
            let second_gaps = report.gaps(Viewer::P2);
            if first_gaps
                .iter()
                .chain(second_gaps)
                .all(|gap| gap.reason == AdmissionGapReason::MissingTriggerIcons)
            {
                return Ok(MatchEnd::MissingTriggerIcons);
            }
            return Err(("admission", format!("{first_gaps:?} {second_gaps:?}")));
        }
    };
    let mut input = game.input(Viewer::P1);
    for step in 0..=STEP_LIMIT {
        trace.step = step;
        trace.turn = game.state.turn.clone();
        let question = match active_input(&game, input) {
            Input::Finished { .. } => return Ok(MatchEnd::Completed),
            Input::EngineFault => {
                return Err(game.referee_fault().map_or_else(
                    || {
                        (
                            "input",
                            "engine fault while projecting the next input".into(),
                        )
                    },
                    diagnostic,
                ));
            }
            Input::Waiting(_) => return Err(("input", "both seats waiting".into())),
            Input::Question(question) => question,
        };
        if step == STEP_LIMIT {
            return Err((
                "step_limit",
                "game did not finish within the submission limit".into(),
            ));
        }
        if inspect(&game, &question, step)?.is_break() {
            return Ok(MatchEnd::Completed);
        }
        let (command, context) = answers::choose(&question, rng)
            .map_err(|message| ("policy", format!("{:?}: {message}", question.purpose)))?;
        if trace.context.len() == 8 {
            trace.context.pop_front();
        }
        trace.context.push_back(format!(
            "{:?} {:?} {context}",
            question.actor, question.purpose
        ));
        trace.step = trace.step.saturating_add(1);
        let submission = game.submit_player(question.actor, command);
        match submission.kind {
            SubmissionKind::Rejected => {
                return Err((
                    "rejected",
                    "engine rejected an answer derived from its question".into(),
                ));
            }
            SubmissionKind::EngineFault => {
                return Err(game.referee_fault().map_or_else(
                    || ("submission", "engine fault without diagnostic".into()),
                    diagnostic,
                ));
            }
            SubmissionKind::Applied | SubmissionKind::Reverted | SubmissionKind::Cancelled => {}
        }
        input = submission.input;
    }
    Err(("step_limit", "unreachable loop exit".into()))
}

pub(super) fn active_input(game: &Game, input: Input) -> Input {
    match input {
        Input::Waiting(wait) => game.input(wait.actor),
        other @ (Input::Question(_) | Input::Finished { .. } | Input::EngineFault) => other,
    }
}

fn diagnostic(error: &crate::EngineFailure) -> (&'static str, String) {
    use crate::EngineFailure::{
        AuthorizationIntegrity, IllegalPlay, Invalid, Invariant, Unsupported,
    };
    let kind = match error {
        AuthorizationIntegrity(_) => "authorization",
        IllegalPlay(_) => "illegal_play",
        Invalid(_) => "invalid",
        Invariant(_) => "invariant",
        Unsupported(_) => "unsupported",
    };
    (kind, error.to_string())
}

/// What became of one seeded match.
pub(super) enum MatchOutcome {
    /// It ended by the rules or by the inspector's stop.
    Completed,
    /// It never opened: the decks need verified Trigger icons.
    MissingTriggerIcons,
    /// It stopped otherwise; the value is the replayable diagnostic.
    Failed(Value),
}

pub(super) fn run_match(
    selected: &SelectedEffects,
    pools: &[DeckPool],
    seed: u64,
    inspect: impl FnMut(&Game, &Question, usize) -> Inspect,
) -> MatchOutcome {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let decks = [
        deck(pools.choose(&mut rng).unwrap(), &mut rng),
        deck(pools.choose(&mut rng).unwrap(), &mut rng),
    ];
    let mut trace = Trace::default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        play(selected, decks.clone(), seed, &mut rng, &mut trace, inspect)
    }))
    .unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic payload")
            .into();
        Err(("panic", message))
    });
    match result {
        Ok(MatchEnd::Completed) => MatchOutcome::Completed,
        Ok(MatchEnd::MissingTriggerIcons) => MatchOutcome::MissingTriggerIcons,
        Err((kind, message)) => MatchOutcome::Failed(json!({
            "seed": seed, "kind": kind, "error": message, "step": trace.step,
            "turn": trace.turn, "context": trace.context,
            "main": decks.each_ref().map(|deck| &deck.main),
            "evolve": decks.each_ref().map(|deck| &deck.evolve),
            "basis": decks.each_ref().map(|deck| format!("{:?}", deck.construction)),
        })),
    }
}

fn worker_count() -> usize {
    thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(6)
}

pub(super) fn run_seeds<T>(
    seeds: &(impl Iterator<Item = u64> + Clone + Send),
    run: impl Fn(u64) -> T + Sync,
) -> Vec<(u64, T)>
where
    T: Send,
{
    let jobs = worker_count().min(seeds.size_hint().1.unwrap_or(usize::MAX));
    let mut reports = thread::scope(|scope| {
        let workers: Vec<_> = (0..jobs)
            .map(|slot| {
                let seeds = seeds.clone();
                let run = &run;
                scope.spawn(move || {
                    seeds
                        .skip(slot)
                        .step_by(jobs)
                        .map(|seed| (seed, run(seed)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    reports.sort_by_key(|(seed, _)| *seed);
    reports
}
