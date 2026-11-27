//! One match between two wire players, with its record, costs and failure.

use crate::answer::{NoAnswer, choose};
use crate::arena::{Arena, Control, Setup};
use crate::check::{Checker, printings};
use crate::pool::{Basis, Deck};
use crate::usage::{Tally, Uses};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use core::any::Any;
use core::cmp::Reverse;
use core::panic::AssertUnwindSafe;
use core::slice::Iter;
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::panic::catch_unwind;
use std::time::Instant;
use sve_battle_contract::observation::{BattleObservation, Knowledge};
use sve_battle_contract::question::{MainIntent, Question, QuestionShape};
use sve_battle_contract::{
    Answer, BattleInput, BattleUpdate, Command, HostMessage, ReceiptResult, RequestId, Seat, Submit,
};
use sve_battle_host::{Delivery, LocalSession};
use sve_engine::game::player::{Input, Viewer};

/// Count-based stops; neither is a wall-clock limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Submissions allowed in one match.
    pub decisions: usize,
    /// Committed events one submission may produce for a seat.
    pub events: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            decisions: 3_000,
            events: 2_000,
        }
    }
}

/// One submission and the receipt it got.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Seat that submitted.
    pub seat: Seat,
    /// The command as sent.
    pub command: Command,
    /// The host's receipt.
    pub result: ReceiptResult,
}

/// Everything needed to play a match again: setup and every submission in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Seed the match was generated from.
    pub seed: u64,
    /// The host's match parameters.
    pub setup: Setup,
    /// Every submission, rejected ones included.
    pub steps: Vec<Step>,
}

/// Cost of one submission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StepCost {
    /// Host time from the submission to the next stable stop, in microseconds.
    pub engine_micros: u64,
    /// Time to serialize the resulting messages, in microseconds.
    pub encode_micros: u64,
    /// Committed events the submission produced for the seat that saw most.
    pub events: usize,
}

/// A submission with what it was about and how large the board was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StepContext {
    /// Position of the submission in the record.
    pub step: usize,
    /// Seat that answered.
    pub seat: Seat,
    /// Use of the question.
    pub usage: String,
    /// Kind of the answer.
    pub answer: &'static str,
    /// Card numbers the question and the chosen option name.
    pub cards: Vec<String>,
    /// Cards on both fields.
    pub field: u64,
    /// Cards in both hands.
    pub hand: u64,
    /// Pending occurrences disclosed to the answering seat.
    pub pending: usize,
    /// Cost of the submission.
    pub cost: StepCost,
}

/// Why a match did not reach a rules result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    /// Stable class: `engine_fault`, `host_error`, `invariant`, `no_answer`, `rejected`,
    /// `decision_limit`, `event_limit`, `load`, `panic`, `replay` or `record_end`.
    pub kind: &'static str,
    /// Authority diagnostic or the broken invariant.
    pub detail: String,
    /// Card numbers of the last question and submission.
    pub cards: Vec<String>,
    /// The last submissions before the failure, oldest first.
    pub context: Vec<String>,
    /// Both seats' last complete updates, in seat order.
    pub board: Value,
    /// The authority's pending native question, for a stop the wire could not express.
    pub native: String,
}

/// How a match ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "end", rename_all = "snake_case")]
pub enum End {
    /// The rules ended the game.
    Finished {
        /// Winner; absent for a draw.
        winner: Option<Seat>,
    },
    /// The match stopped for another reason.
    Failed(Failure),
}

/// One played match.
#[derive(Clone, Debug)]
pub struct Match {
    /// Replayable record.
    pub record: Record,
    /// Result or failure.
    pub end: End,
    /// Cost of every submission, in record order, the one that ended the match included;
    /// a submission that panicked inside the host has none.
    pub costs: Vec<StepCost>,
    /// The slowest submissions, slowest first.
    pub slow: Vec<StepContext>,
    /// Card numbers that were publicly visible at some stop.
    pub seen: BTreeSet<String>,
    /// What each card number did, from both seats' event feeds.
    pub uses: BTreeMap<String, Uses>,
    /// Positions of the seats' decks in the deck list, when the decks were listed.
    pub listed: Option<[usize; 2]>,
    /// Turns begun by both seats.
    pub turns: u64,
}

const SLOW_KEPT: usize = 5;
const CONTEXT_KEPT: usize = 8;
const REJECTIONS_ALLOWED: usize = 3;

enum Source<'record> {
    Random(Box<ChaCha8Rng>),
    Recorded(Iter<'record, Step>),
}

#[derive(Default)]
struct Progress {
    steps: Vec<Step>,
    costs: Vec<StepCost>,
    slow: Vec<StepContext>,
    seen: BTreeSet<String>,
    tally: Tally,
    context: VecDeque<String>,
    cards: Vec<String>,
    turns: u64,
}

impl Progress {
    fn fail(&self, kind: &'static str, detail: String) -> Box<Failure> {
        Box::new(Failure {
            kind,
            detail,
            cards: self.cards.clone(),
            context: self.context.iter().cloned().collect(),
            board: Value::Null,
            native: String::new(),
        })
    }

    fn note_slow(&mut self, context: StepContext) {
        self.slow.push(context);
        self.slow
            .sort_by_key(|step| Reverse(step.cost.engine_micros));
        self.slow.truncate(SLOW_KEPT);
    }
}

const fn answer_kind(answer: &Answer) -> &'static str {
    match answer {
        Answer::Select { .. } => "select",
        Answer::Choose { .. } => "choose",
        Answer::Mulligan { .. } => "mulligan",
        Answer::ActionWindow { .. } => "action_window",
        Answer::Quick { .. } => "quick",
        Answer::EvolutionPayment { .. } => "evolution_payment",
        Answer::ResourcePayment { .. } => "resource_payment",
        Answer::Confirm { .. } => "confirm",
        Answer::Integer { .. } => "integer",
        Answer::DiceReroll { .. } => "dice_reroll",
        Answer::Order { .. } => "order",
        Answer::DriveTrigger { .. } => "drive_trigger",
        Answer::Partition { .. } => "partition",
        Answer::Distribute { .. } => "distribute",
    }
}

fn label<T>(value: &T) -> String
where
    T: Serialize,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Card numbers the question's processing context and the chosen option name.
fn focus(
    arena: &Arena,
    observation: &BattleObservation,
    question: &Question,
    command: &Command,
) -> Vec<String> {
    let mut found = BTreeSet::new();
    if let Ok(processing) = serde_json::to_value(&question.processing) {
        printings(&processing, &mut found);
    }
    let mut scan = |value: Result<Value, serde_json::Error>| {
        if let Ok(json) = value {
            printings(&json, &mut found);
        }
    };
    if let Command::Answer { answer, .. } = command {
        match (&question.shape, answer) {
            (QuestionShape::ActionWindow { intents }, Answer::ActionWindow { intent, .. }) => {
                let handle = intents.iter().find_map(|option| match option {
                    MainIntent::Play { id, source, .. } => (id == intent).then_some(source),
                    MainIntent::Activate { id, holder, .. }
                    | MainIntent::Evolve { id, holder, .. } => (id == intent).then_some(holder),
                    MainIntent::Attack { id, attacker, .. } => (id == intent).then_some(attacker),
                    MainIntent::EndPhase { .. } => None,
                });
                if let Some(object) = handle.and_then(|handle| observation.objects.get(handle)) {
                    scan(serde_json::to_value(&object.card));
                }
            }
            (
                QuestionShape::Quick { options, .. },
                Answer::Quick {
                    option: Some(chosen),
                },
            ) => {
                if let Some(option) = options.iter().find(|option| option.id == *chosen) {
                    scan(serde_json::to_value(&option.source));
                }
            }
            (QuestionShape::Choose { options, .. }, Answer::Choose { chosen }) => {
                for option in options.iter().filter(|option| chosen.contains(&option.id)) {
                    scan(serde_json::to_value(&option.value));
                }
            }
            _ => {}
        }
    }
    found
        .iter()
        .map(|printing| arena.number(printing).to_owned())
        .collect()
}

fn zone_total(observation: &BattleObservation, field: bool) -> u64 {
    observation
        .players
        .iter()
        .map(|player| {
            if field {
                player.zones.field.count.get()
            } else {
                player.zones.hand.count.get()
            }
        })
        .fold(0, u64::saturating_add)
}

/// The authority's own word on a stop no seat can answer; never part of a player message.
fn diagnose(session: &LocalSession) -> String {
    let game = session.game();
    if let Some(fault) = game.referee_fault() {
        return fault.to_string();
    }
    [Viewer::P1, Viewer::P2]
        .into_iter()
        .find_map(|viewer| match game.input(viewer) {
            Input::Question(question) => {
                // The native variant names say which projection is missing; the payload does not.
                let shape = format!("{:?}", question.shape);
                let name: String = shape
                    .chars()
                    .take_while(|letter| !matches!(letter, '{' | '['))
                    .take(80)
                    .collect();
                Some(format!("{:?} {}", question.purpose, name.trim()))
            }
            Input::Waiting(_) | Input::Finished { .. } | Input::EngineFault => None,
        })
        .unwrap_or_else(|| "no native question".into())
}

/// The native question nobody could be asked, cut to a readable length.
fn native_question(session: &LocalSession) -> String {
    [Viewer::P1, Viewer::P2]
        .into_iter()
        .find_map(|viewer| match session.game().input(viewer) {
            Input::Question(question) => {
                Some(format!("{question:?}").chars().take(6_000).collect())
            }
            Input::Waiting(_) | Input::Finished { .. } | Input::EngineFault => None,
        })
        .unwrap_or_default()
}

struct Table<'arena> {
    arena: &'arena Arena,
    session: LocalSession,
    checker: Checker,
    latest: [Option<BattleUpdate>; 2],
    limits: Limits,
}

struct Absorbed {
    receipt: Option<ReceiptResult>,
    events: usize,
    encode_micros: u64,
    /// The first message that ended the match; what came before it is still counted.
    failure: Option<Box<Failure>>,
}

fn micros(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

impl Table<'_> {
    /// Take in what the host sent: check every message and keep each seat's newest update.
    fn absorb(&mut self, deliveries: &[Delivery], progress: &mut Progress) -> Absorbed {
        let mut absorbed = Absorbed {
            receipt: None,
            events: 0,
            encode_micros: 0,
            failure: None,
        };
        for delivery in deliveries {
            if let Err(failure) = self.take(delivery, progress, &mut absorbed) {
                absorbed.failure = Some(failure);
                break;
            }
        }
        absorbed
    }

    fn take(
        &mut self,
        delivery: &Delivery,
        progress: &mut Progress,
        absorbed: &mut Absorbed,
    ) -> Result<(), Box<Failure>> {
        let start = Instant::now();
        let text = serde_json::to_string(&delivery.message)
            .map_err(|error| progress.fail("invariant", format!("unserializable: {error}")))?;
        absorbed.encode_micros = absorbed.encode_micros.saturating_add(micros(start));
        let update = match &delivery.message {
            HostMessage::Ready(_) => return Ok(()),
            HostMessage::Receipt(receipt) => {
                absorbed.receipt = Some(receipt.result());
                return Ok(());
            }
            HostMessage::HostError(error) => {
                return Err(progress.fail(
                    "host_error",
                    format!("{}: {}", label(&error.reason), diagnose(&self.session)),
                ));
            }
            HostMessage::Update(update) => update,
        };
        if sve_battle_contract::decode::<HostMessage>(&text)
            .ok()
            .as_ref()
            != Some(&delivery.message)
        {
            return Err(progress.fail(
                "invariant",
                "wire_round_trip: the contract does not decode what the host sent".into(),
            ));
        }
        let json: Value = serde_json::from_str(&text).unwrap_or_default();
        self.checker.update(update, &json).map_err(|broken| {
            progress.fail("invariant", format!("{}: {}", broken.name, broken.detail))
        })?;
        let events = json
            .pointer("/events/batches")
            .and_then(Value::as_array)
            .map_or(0, |batches| {
                batches
                    .iter()
                    .filter_map(|batch| batch.get("events").and_then(Value::as_array))
                    .map(Vec::len)
                    .sum()
            });
        absorbed.events = absorbed.events.max(events);
        progress.tally.update(update);
        progress.seen.extend(
            update
                .observation
                .objects
                .values()
                .filter(|object| object.knowledge == Knowledge::Public)
                .map(|object| self.arena.number(&object.card.printing_id).to_owned()),
        );
        progress.turns = update
            .observation
            .turn
            .elapsed_turns
            .iter()
            .map(|turns| turns.get())
            .fold(0, u64::saturating_add);
        let slot = match delivery.seat {
            Seat::P1 => self.latest.first_mut(),
            Seat::P2 => self.latest.last_mut(),
        };
        if let Some(slot) = slot {
            *slot = Some((**update).clone());
        }
        Ok(())
    }

    fn open_question(&self) -> Option<(Seat, &BattleUpdate, &Question)> {
        self.latest
            .iter()
            .flatten()
            .find_map(|update| match &update.input {
                BattleInput::Question { question } => Some((update.viewer, update, &**question)),
                BattleInput::Waiting { .. }
                | BattleInput::Finished { .. }
                | BattleInput::EngineFault { .. } => None,
            })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "The match loop reads top to bottom: absorb, check, ask, answer, submit."
    )]
    fn run(
        &mut self,
        source: &mut Source<'_>,
        progress: &mut Progress,
    ) -> Result<Option<Seat>, Box<Failure>> {
        let mut deliveries = self.session.start();
        let mut rejections = 0_usize;
        let mut pending: Option<(StepContext, Instant)> = None;
        loop {
            let engine_micros = pending.as_ref().map_or(0, |(_, start)| micros(*start));
            let mut absorbed = self.absorb(&deliveries, progress);
            if let Some((mut context, _)) = pending.take() {
                // The cost and receipt of a submission are kept even when it ended the match.
                context.cost = StepCost {
                    engine_micros,
                    encode_micros: absorbed.encode_micros,
                    events: absorbed.events,
                };
                let (number, events) = (context.step, context.cost.events);
                progress.costs.push(context.cost);
                progress.note_slow(context);
                let recorded = progress.steps.last().map(|step| step.result);
                if let (Some(result), Some(step)) = (absorbed.receipt, progress.steps.last_mut()) {
                    step.result = result;
                }
                if let Some(failure) = absorbed.failure.take() {
                    return Err(failure);
                }
                let result = absorbed.receipt.ok_or_else(|| {
                    progress.fail("invariant", "receipt: a submission got no receipt".into())
                })?;
                if matches!(source, Source::Recorded(_)) && recorded != Some(result) {
                    return Err(progress.fail(
                        "replay",
                        format!(
                            "step {number} got {} where the record says {}",
                            label(&result),
                            recorded.map(|kind| label(&kind)).unwrap_or_default()
                        ),
                    ));
                }
                rejections = if result == ReceiptResult::Rejected {
                    rejections.saturating_add(1)
                } else {
                    0
                };
                if events > self.limits.events {
                    return Err(progress.fail(
                        "event_limit",
                        format!("one submission produced {events} events"),
                    ));
                }
                if rejections > REJECTIONS_ALLOWED {
                    return Err(progress.fail(
                        "rejected",
                        "the host refused answers inside the question's stated domain".into(),
                    ));
                }
            }
            if let Some(failure) = absorbed.failure {
                return Err(failure);
            }
            self.checker.stop(&self.latest).map_err(|broken| {
                progress.fail("invariant", format!("{}: {}", broken.name, broken.detail))
            })?;
            self.checker.settle(&self.latest);
            let Some((seat, update, question)) = self.open_question() else {
                return match self
                    .latest
                    .iter()
                    .flatten()
                    .map(|update| &update.input)
                    .next()
                {
                    Some(BattleInput::Finished { .. }) if matches!(source, Source::Recorded(steps) if steps.len() > 0) => {
                        Err(progress.fail("replay", "the match ends before the record does".into()))
                    }
                    Some(BattleInput::Finished { winner, .. }) => Ok(*winner),
                    Some(BattleInput::EngineFault { .. }) => {
                        Err(progress.fail("engine_fault", diagnose(&self.session)))
                    }
                    Some(BattleInput::Question { .. } | BattleInput::Waiting { .. }) | None => {
                        Err(progress.fail("invariant", "routing: no seat can answer".into()))
                    }
                };
            };
            if progress.steps.len() >= self.limits.decisions {
                return Err(progress.fail(
                    "decision_limit",
                    format!("no result after {} submissions", progress.steps.len()),
                ));
            }
            let (actor, command, recorded) = match source {
                Source::Random(rng) => {
                    let command = if question.can_withdraw && rng.random_range(0..20_u8) == 0 {
                        Command::Withdraw {
                            question_id: question.id.clone(),
                        }
                    } else {
                        let answer = choose(question, rng).map_err(|missing| {
                            progress.cards =
                                focus(self.arena, &update.observation, question, &Command::Concede);
                            progress.fail(
                                "no_answer",
                                format!(
                                    "{} has no answer in its stated domain ({})",
                                    label(&question.usage),
                                    match missing {
                                        NoAnswer::Proven => "proven",
                                        NoAnswer::NotFound => "not found by sampling",
                                    }
                                ),
                            )
                        })?;
                        Command::Answer {
                            question_id: question.id.clone(),
                            answer,
                        }
                    };
                    (seat, command, ReceiptResult::Applied)
                }
                Source::Recorded(steps) => match steps.next() {
                    Some(step) => (step.seat, step.command.clone(), step.result),
                    // Where the recorded match stopped without submitting: not a divergence.
                    None => {
                        return Err(progress.fail(
                            "record_end",
                            format!(
                                "the record has no answer for this {}",
                                label(&question.usage)
                            ),
                        ));
                    }
                },
            };
            let cards = focus(self.arena, &update.observation, question, &command);
            let context = StepContext {
                step: progress.steps.len(),
                seat: actor,
                usage: label(&question.usage),
                answer: match &command {
                    Command::Answer { answer, .. } => answer_kind(answer),
                    Command::Withdraw { .. } => "withdraw",
                    Command::Concede => "concede",
                },
                cards: cards.clone(),
                field: zone_total(&update.observation, true),
                hand: zone_total(&update.observation, false),
                pending: update.observation.pending.len(),
                cost: StepCost::default(),
            };
            if progress.context.len() == CONTEXT_KEPT {
                progress.context.pop_front();
            }
            progress.context.push_back(format!(
                "{} {} {} {}",
                label(&actor),
                context.usage,
                context.answer,
                cards.join(",")
            ));
            progress.cards = cards;
            let submit = Submit {
                request_id: RequestId::new(format!("r{}", progress.steps.len())),
                command: command.clone(),
            };
            progress.steps.push(Step {
                seat: actor,
                command,
                // Replaced by the receipt once it arrives; a replay compares the two first.
                result: recorded,
            });
            let start = Instant::now();
            deliveries = self.session.submit(actor, &submit);
            pending = Some((context, start));
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("unknown panic payload")
        .to_owned()
}

fn finish(arena: &Arena, seed: u64, setup: Setup, progress: Progress, end: End) -> Match {
    let mut uses = BTreeMap::<String, Uses>::new();
    for (printing, counted) in progress.tally.merged() {
        uses.entry(arena.number(&printing).to_owned())
            .or_default()
            .add(&counted);
    }
    Match {
        record: Record {
            seed,
            setup,
            steps: progress.steps,
        },
        end,
        costs: progress.costs,
        slow: progress.slow,
        seen: progress.seen,
        uses,
        listed: None,
        turns: progress.turns,
    }
}

fn drive(arena: &Arena, seed: u64, setup: Setup, mut source: Source<'_>, limits: Limits) -> Match {
    let mut progress = Progress::default();
    let wire = |deck: &Deck| {
        deck.main
            .iter()
            .chain(&deck.evolve)
            .map(|reference| {
                arena
                    .printings
                    .get(reference)
                    .cloned()
                    .unwrap_or_else(|| reference.clone())
            })
            .collect::<Vec<_>>()
    };
    let decks = [wire(&setup.decks[0]), wire(&setup.decks[1])];
    // Opening lowers and admits the decks; a panic there must not take the whole run down.
    let opened = catch_unwind(AssertUnwindSafe(|| (arena.open)(&setup)));
    let session = match opened {
        Ok(Ok(session)) => session,
        Ok(Err(stage)) => {
            let failure = progress.fail("load", stage);
            return finish(arena, seed, setup, progress, End::Failed(*failure));
        }
        Err(payload) => {
            let failure = progress.fail("panic", panic_message(payload.as_ref()));
            return finish(arena, seed, setup, progress, End::Failed(*failure));
        }
    };
    let mut table = Table {
        arena,
        session,
        checker: Checker::new(&decks),
        latest: [None, None],
        limits,
    };
    let result = catch_unwind(AssertUnwindSafe(|| table.run(&mut source, &mut progress)));
    let failed = |mut failure: Box<Failure>| {
        failure.board = serde_json::to_value(&table.latest).unwrap_or_default();
        if failure.kind == "host_error" {
            failure.native = native_question(&table.session);
        }
        End::Failed(*failure)
    };
    let end = match result {
        Ok(Ok(winner)) => End::Finished { winner },
        Ok(Err(failure)) => failed(failure),
        Err(payload) => failed(progress.fail("panic", panic_message(payload.as_ref()))),
    };
    finish(arena, seed, setup, progress, end)
}

fn seated(seed: u64, decks: [Deck; 2], rng: &mut ChaCha8Rng) -> Setup {
    Setup {
        decks,
        first_chooser: if rng.random() { Seat::P1 } else { Seat::P2 },
        seed: seed.to_string(),
        control: [Control::Player, Control::Player],
        max_submissions: 0,
    }
}

/// The setup a seed names: two random legal decks and the first chooser.
fn setup(arena: &Arena, seed: u64, rng: &mut ChaCha8Rng, singles: bool) -> Option<Setup> {
    let copies = |source: &mut ChaCha8Rng| {
        let drawn = [1_usize, 1, 2, 3]
            .get(source.random_range(0..4_usize))
            .copied()
            .unwrap_or(1);
        if singles { 1 } else { drawn }
    };
    let first = copies(rng);
    let second = copies(rng);
    let decks = [
        arena.pools.deck(first, rng)?,
        arena.pools.deck(second, rng)?,
    ];
    Some(seated(seed, decks, rng))
}

/// Play the match a seed names: listed decks drawn by weight when the arena has a list,
/// otherwise random decks.
#[must_use]
pub fn play(arena: &Arena, seed: u64, limits: Limits) -> Match {
    let Some(lineup) = &arena.lineup else {
        return play_random(arena, seed, limits);
    };
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let drawn = lineup.draw(&mut rng).zip(lineup.draw(&mut rng));
    let Some(((first, first_deck), (second, second_deck))) = drawn else {
        return play_random(arena, seed, limits);
    };
    let setup = seated(seed, [first_deck, second_deck], &mut rng);
    let mut played = drive(arena, seed, setup, Source::Random(Box::new(rng)), limits);
    played.listed = Some([first, second]);
    played
}

/// Random decks; ones with several copies that a printed construction restriction refuses
/// are rebuilt with one copy per name.
fn play_random(arena: &Arena, seed: u64, limits: Limits) -> Match {
    let mut last = None;
    for singles in [false, true] {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let Some(setup) = setup(arena, seed, &mut rng, singles) else {
            break;
        };
        let played = drive(arena, seed, setup, Source::Random(Box::new(rng)), limits);
        let refused = matches!(&played.end, End::Failed(failure) if failure.kind == "load" && failure.detail == "match");
        last = Some(played);
        if !refused {
            break;
        }
    }
    last.unwrap_or_else(|| {
        let empty = Deck {
            construction: Basis::Class(String::new()),
            main: Vec::new(),
            evolve: Vec::new(),
        };
        let setup = Setup {
            decks: [empty.clone(), empty],
            first_chooser: Seat::P1,
            seed: seed.to_string(),
            control: [Control::Player, Control::Player],
            max_submissions: 0,
        };
        let progress = Progress::default();
        let failure = progress.fail("load", "no construction pool".into());
        finish(arena, seed, setup, progress, End::Failed(*failure))
    })
}

/// Submit a record's commands again; any receipt that differs is a `replay` failure.
#[must_use]
pub fn replay(arena: &Arena, record: &Record, limits: Limits) -> Match {
    drive(
        arena,
        record.seed,
        record.setup.clone(),
        Source::Recorded(record.steps.iter()),
        limits,
    )
}
