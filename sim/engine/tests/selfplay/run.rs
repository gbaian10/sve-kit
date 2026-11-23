//! Opt-in exercise of the strict player pipeline against the current repository selection.
//!
//! From the repository root:
//!
//! ```bash
//! SVE_SELFPLAY_GAMES=100 SVE_SELFPLAY_SEED=0 python3 .github/ci/rust_tests.py \
//!   test --locked -p sve-engine --lib selfplay::run -- --ignored --nocapture
//! ```
//!
//! Set `SVE_SELFPLAY_REPRINTS` to sample alternate printings. Each seat uses base construction
//! rules, independent of any tournament format or ban list.

#![expect(
    clippy::unwrap_used,
    reason = "A missing fixture or malformed run configuration must fail this diagnostic test."
)]

use super::player::{DeckConstruction, Input, PlayerDeck, Question, Viewer};
use super::{Game, repository};
use crate::catalog::SelectedEffects;
use crate::ir::{Cardinality, KeywordId, printed_keywords};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use core::num::NonZeroUsize;
use core::ops::ControlFlow;
use core::panic::AssertUnwindSafe;
use core::sync::atomic::{AtomicU64, Ordering};
use rand::seq::{IndexedRandom as _, SliceRandom as _};
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha8Rng;
use serde_json::{Value, json};
use std::env;
use std::panic::catch_unwind;
use std::{thread, time::Instant};

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
        let face = selected.prototypes.face(number, 0).unwrap();
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
            name: selected.prototypes.rules_label(number, 0).unwrap(),
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
            && let Ok(Some(code)) = selected.prototypes.rule_bindings.title_code("", title)
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
                .prototypes
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
    seen: BTreeSet<String>,
    rollbacks: usize,
    cancellations: usize,
}

impl Trace {
    fn record_visible(&mut self, game: &Game) {
        for object in game.state.objects.values() {
            if !matches!(object.zone.as_str(), "deck" | "evolve_deck" | "void") {
                self.seen.insert(object.card.clone());
            }
        }
    }
}

enum MatchEnd {
    Finished(Option<Viewer>),
    Sampled,
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
    use super::player::{StrictStartError, SubmissionKind};
    let first = if rng.random() { Viewer::P1 } else { Viewer::P2 };
    let mut game = Game::start_player_game(selected.clone(), decks, first, &seed.to_string())
        .map_err(|error| match error {
            StrictStartError::Initialization(fault) => diagnostic(&fault),
            StrictStartError::Admission(report) => (
                "admission",
                format!(
                    "{:?} {:?}",
                    report.gaps(Viewer::P1),
                    report.gaps(Viewer::P2)
                ),
            ),
        })?;
    let mut input = game.input(Viewer::P1);
    for step in 0..=STEP_LIMIT {
        trace.step = step;
        trace.record_visible(&game);
        trace.turn = game.state.turn.clone();
        let question = match active_input(&game, input) {
            Input::Finished { winner } => return Ok(MatchEnd::Finished(winner)),
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
            return Ok(MatchEnd::Sampled);
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
            SubmissionKind::Applied => {}
            SubmissionKind::Reverted => trace.rollbacks = trace.rollbacks.saturating_add(1),
            SubmissionKind::Cancelled => {
                trace.cancellations = trace.cancellations.saturating_add(1);
            }
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

pub(super) struct MatchReport {
    outcome: &'static str,
    included: BTreeSet<String>,
    seen: BTreeSet<String>,
    pub(super) failure: Option<Value>,
    steps: usize,
    rollbacks: usize,
    cancellations: usize,
}

pub(super) fn run_match(
    selected: &SelectedEffects,
    pools: &[DeckPool],
    seed: u64,
    inspect: impl FnMut(&Game, &Question, usize) -> Inspect,
) -> MatchReport {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let decks = [
        deck(pools.choose(&mut rng).unwrap(), &mut rng),
        deck(pools.choose(&mut rng).unwrap(), &mut rng),
    ];
    let included = decks
        .iter()
        .flat_map(|deck| deck.main.iter().chain(&deck.evolve))
        .cloned()
        .collect();
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
    let (outcome, failure) = match result {
        Ok(MatchEnd::Finished(Some(Viewer::P1))) => ("P1", None),
        Ok(MatchEnd::Finished(Some(Viewer::P2))) => ("P2", None),
        Ok(MatchEnd::Finished(None)) => ("draw", None),
        Ok(MatchEnd::Sampled) => ("sampled", None),
        Err((kind, message)) => (
            kind,
            Some(json!({
                "seed": seed, "kind": kind, "error": message, "step": trace.step,
                "turn": trace.turn, "context": trace.context,
                "main": decks.each_ref().map(|deck| &deck.main),
                "evolve": decks.each_ref().map(|deck| &deck.evolve),
                "basis": decks.each_ref().map(|deck| format!("{:?}", deck.construction)),
            })),
        ),
    };
    MatchReport {
        outcome,
        included,
        seen: trace.seen,
        failure,
        steps: trace.step,
        rollbacks: trace.rollbacks,
        cancellations: trace.cancellations,
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

#[test]
#[ignore = "opt-in multi-game diagnostic using the current repository selection"]
fn run() {
    let setup = Instant::now();
    let selected = repository::selection_with_rules();
    let reprints = env::var_os("SVE_SELFPLAY_REPRINTS").is_some();
    let pools = pools(selected, reprints);
    assert!(!pools.is_empty());
    let count: u64 = env::var("SVE_SELFPLAY_GAMES")
        .map_or(Ok(1), |value| value.parse())
        .unwrap();
    let first_seed: u64 = env::var("SVE_SELFPLAY_SEED")
        .map_or(Ok(0), |value| value.parse())
        .unwrap();
    assert!(count > 0);
    let last_seed = first_seed.checked_add(count.saturating_sub(1)).unwrap();
    let start = Instant::now();
    let completed = AtomicU64::new(0);
    let reports = run_seeds(&(first_seed..=last_seed), |seed| {
        let report = run_match(selected, &pools, seed, |_, _, _| {
            Ok(ControlFlow::Continue(()))
        });
        let done = completed.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        if count <= 30 || done.is_multiple_of(64) {
            eprintln!(
                "selfplay {done}/{count} games complete (seed {seed}, {} submissions)",
                report.steps
            );
        }
        report
    });
    let mut included = BTreeMap::<String, u64>::new();
    let mut seen = BTreeMap::<String, u64>::new();
    let mut outcomes = BTreeMap::<&str, u64>::new();
    let mut submissions = 0_usize;
    let mut rollbacks = 0_usize;
    let mut cancellations = 0_usize;
    for (_, report) in reports {
        submissions = submissions.saturating_add(report.steps);
        rollbacks = rollbacks.saturating_add(report.rollbacks);
        cancellations = cancellations.saturating_add(report.cancellations);
        for number in report.included {
            *included.entry(number).or_default() += 1;
        }
        for number in report.seen {
            *seen.entry(number).or_default() += 1;
        }
        if let Some(failure) = report.failure {
            println!("{failure}");
        }
        *outcomes.entry(report.outcome).or_default() += 1;
    }
    let eligible: BTreeSet<_> = pools
        .iter()
        .flat_map(|pool| {
            pool.cards
                .main
                .iter()
                .chain(&pool.cards.evolve)
                .map(|card| &card.number)
        })
        .collect();
    println!(
        "{}",
        json!({"first_seed":first_seed,"games":count,"elapsed_seconds":start.elapsed().as_secs_f64(),
        "total_seconds":setup.elapsed().as_secs_f64(),"reprints":reprints,"workers":worker_count(),
        "submissions":submissions,"rollbacks":rollbacks,"cancellations":cancellations,
        "selection":selected.current.len(),"eligible":eligible,
        "built":included,"seen":seen,"outcomes":outcomes})
    );
}
