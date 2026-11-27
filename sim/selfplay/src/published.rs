//! An arena over a verified public export: the same inputs and loader as the browser Worker.

use crate::arena::{Arena, Control, Open, Setup};
use crate::lineup::{DeckList, Lineup, ListedDeck, Refused, shape};
use crate::pool::{Basis, Deck, NAME_LIMIT, NEUTRAL, PoolCard, Pools};
use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use core::iter::repeat_n;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fs::read_to_string;
use std::path::Path;
use sve_battle_contract::Seat;
use sve_battle_host::load::PreparedBattle;
use sve_engine::catalog::{EngineIdentityInput, ProgramPrinting, SnapshotPrograms};
use sve_engine::game::Game;
use sve_engine::game::player::{DeckConstruction, PlayerDeck, Viewer};

/// The keyword registry this engine build was written against.
const KEYWORDS: &str = include_str!("../../../authored/rules/keyword-registry.yaml");

fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

fn read(path: &Path) -> Result<String, String> {
    read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn parse(text: &str, name: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{name}: {error}"))
}

/// The battle inputs document and program package the Worker would load.
///
/// `reader` is the output of `sve-carddb snapshot battle-inputs --published`; `export` is the
/// public export root that holds the program blob the reader's descriptor names.
///
/// # Errors
/// A file is missing or is not the document the reader writes.
pub fn compose(reader: &Path, export: &Path) -> Result<(String, String), String> {
    let references = parse(&read(&reader.join("references.json"))?, "references.json")?;
    let projection = parse(
        &read(&reader.join("program-projection.json"))?,
        "program-projection.json",
    )?;
    let rules = read(&reader.join("rules.json"))?;
    let text = |value: &Value, pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("reader output lacks {pointer}"))
    };
    let programs = read(&export.join(text(&projection, "/descriptor/path")?))?;
    let digest = |value: String| value.trim_start_matches("sha256:").to_owned();
    let inputs = json!({
        "snapshot": read(&reader.join("snapshot.jsonl"))?,
        "keywords": KEYWORDS,
        "rules": rules,
        "references": references,
        "printings": projection.get("printings"),
        "engine_identity": projection.get("engine_identity"),
        "identity": {
            "snapshot": {"id": "public-export",
                "sha256": digest(text(&references, "/wording/manifest_sha256")?)},
            "programs": {"id": text(&projection, "/descriptor/key")?,
                "sha256": digest(text(&projection, "/descriptor/sha256")?)},
            // No WASM build is involved; the host only echoes this identity.
            "engine": {"id": "sve-selfplay", "sha256": hex("sve-selfplay")},
            "rules": {"id": "engine-rules", "sha256": hex(&rules)},
            "region": text(&references, "/region")?,
        },
    });
    Ok((inputs.to_string(), programs))
}

/// Why a printing with a program cannot enter a deck, or a deck of its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excluded {
    /// Native card number.
    pub number: String,
    /// `lowering`, the admission gap reasons, or `no construction pool`.
    pub reason: String,
}

/// A prepared export and the printings self-play had to leave out.
pub struct Published {
    arena: Arena,
    excluded: Vec<Excluded>,
    unprogrammed: usize,
    /// The programs the host prepared, held natively to probe listed decks.
    native: SnapshotPrograms,
    deck_cards: BTreeMap<String, PoolCard>,
    amulet_titles: BTreeSet<String>,
}

impl fmt::Debug for Published {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Published")
            .field("arena", &self.arena)
            .field("excluded", &self.excluded)
            .field("unprogrammed", &self.unprogrammed)
            .finish_non_exhaustive()
    }
}

struct Facts {
    class: Option<String>,
    title: Option<String>,
    kind: String,
}

fn snapshot_facts(snapshot: &str) -> BTreeMap<String, Facts> {
    snapshot
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| {
            let face = record.pointer("/faces/0")?;
            let field = |name: &str| face.get(name).and_then(Value::as_str).map(str::to_owned);
            Some((
                record.get("number")?.as_str()?.to_owned(),
                Facts {
                    class: field("card_class"),
                    title: field("title"),
                    kind: field("card_type")?,
                },
            ))
        })
        .collect()
}

fn deck_kind(kind: &str) -> Option<bool> {
    let base = ["フォロワー", "スペル", "アミュレット"]
        .iter()
        .any(|base| kind.contains(base));
    (base && !kind.contains("トークン"))
        .then(|| kind.contains("エボルヴ") || kind.contains("アドバンス"))
}

fn start_amulets(programs: &Value) -> BTreeSet<String> {
    programs
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let ast = entry.get("ast")?;
            let declared = ast.get("faces")?.as_object()?.values().any(|face| {
                face.get("abilities")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|ability| ability.get("keywords")?.as_array())
                    .flatten()
                    .any(|keyword| keyword == "start_amulet")
            });
            declared.then(|| ast.get("card_id")?.as_str().map(str::to_owned))?
        })
        .collect()
}

/// Whether one printing alone passes lowering and whole-card admission on this basis.
fn probe(
    programs: &SnapshotPrograms,
    number: &str,
    evolve: bool,
    basis: &DeckConstruction,
) -> Result<(), String> {
    let deck = |cards: Vec<String>| PlayerDeck {
        construction: basis.clone(),
        main: if evolve { Vec::new() } else { cards.clone() },
        evolve: if evolve { cards } else { Vec::new() },
    };
    let decks = [deck(vec![number.to_owned()]), deck(Vec::new())];
    admission_gaps(programs, &decks)?.map_or(Ok(()), Err)
}

fn admission_gaps(
    programs: &SnapshotPrograms,
    decks: &[PlayerDeck; 2],
) -> Result<Option<String>, String> {
    let selected = Game::select_snapshot_decks(programs, decks)
        .map_err(|error| format!("lowering: {error}"))?;
    let report = Game::check_player_decks(&selected, decks)
        .map_err(|error| format!("admission: {error}"))?;
    if report.is_admitted() {
        return Ok(None);
    }
    let reasons: BTreeSet<String> = report
        .gaps(Viewer::P1)
        .iter()
        .map(|gap| format!("{:?}", gap.reason))
        .collect();
    Ok(Some(reasons.into_iter().collect::<Vec<_>>().join("+")))
}

fn opener(prepared: PreparedBattle) -> Open {
    Box::new(move |setup: &Setup| {
        let encoded = serde_json::to_string(setup).map_err(|error| error.to_string())?;
        prepared
            .open(&encoded)
            .map_err(|error| error.code().to_owned())
    })
}

/// The same programs the host prepares, held separately to probe single printings.
fn native_programs(document: &Value, programs: &str) -> Result<SnapshotPrograms, String> {
    let text = |name: &str| {
        document
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let printings: Vec<ProgramPrinting> =
        serde_json::from_value(document.get("printings").cloned().unwrap_or_default())
            .map_err(|error| format!("printings: {error}"))?;
    let identity: EngineIdentityInput =
        serde_json::from_value(document.get("engine_identity").cloned().unwrap_or_default())
            .map_err(|error| format!("engine identity: {error}"))?;
    SnapshotPrograms::new(
        text("snapshot"),
        text("keywords"),
        programs,
        &printings,
        text("rules"),
        &identity,
    )
    .map_err(|error| error.to_string())
}

fn deck_card(row: &Value, fact: &Facts, evolve: bool, start_amulet: bool) -> PoolCard {
    let field = |name: &str| row.get(name).and_then(Value::as_str).unwrap_or_default();
    let number = field("card_no");
    PoolCard {
        reference: field("printing_id").to_owned(),
        number: number.to_owned(),
        name: row
            .pointer("/faces/0/rules_name")
            .and_then(Value::as_str)
            .unwrap_or(number)
            .to_owned(),
        class: fact.class.clone().filter(|class| class != "-"),
        title: None,
        evolve,
        start_amulet,
    }
}

impl Published {
    /// Prepare the export and probe every deck-type printing for admission.
    ///
    /// # Errors
    /// The documents are not a consistent battle input, with the host's stage code.
    pub fn new(inputs: &str, programs: &str) -> Result<Self, String> {
        let prepared =
            PreparedBattle::new(inputs, programs).map_err(|error| error.code().to_owned())?;
        let document = parse(inputs, "inputs")?;
        let package = parse(programs, "programs")?;
        let native = native_programs(&document, programs)?;
        let text = |name: &str| {
            document
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let facts = snapshot_facts(text("snapshot"));
        let rules = parse(text("rules"), "rules")?;
        let (codes, amulet_titles) = titles(&document, &rules, &facts);
        let programmed = package.get("printings").and_then(Value::as_object);
        let amulets = start_amulets(&package);
        let mut cards = Vec::new();
        let mut excluded = Vec::new();
        let mut unprogrammed = 0_usize;
        let mut deck_cards = BTreeMap::new();
        let mut numbers = BTreeMap::new();
        for row in document
            .get("printings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let field = |name: &str| row.get(name).and_then(Value::as_str).unwrap_or_default();
            let (printing, number, card) =
                (field("printing_id"), field("card_no"), field("card_id"));
            numbers.insert(printing.to_owned(), number.to_owned());
            let Some(fact) = facts.get(number) else {
                continue;
            };
            let Some(evolve) = deck_kind(&fact.kind) else {
                continue;
            };
            let card_title = fact.title.as_ref().and_then(|title| codes.get(title));
            let mut candidate = deck_card(row, fact, evolve, amulets.contains(card));
            candidate.title = card_title.cloned();
            deck_cards.insert(number.to_owned(), candidate.clone());
            if !programmed.is_some_and(|known| known.contains_key(card)) {
                unprogrammed = unprogrammed.saturating_add(1);
                continue;
            }
            let neutral = DeckConstruction::Class(NEUTRAL.into());
            if let Err(reason) = probe(&native, number, evolve, &neutral) {
                excluded.push(Excluded {
                    number: number.to_owned(),
                    reason,
                });
                continue;
            }
            // A title basis can ask more of a card than a class basis does (CR 14.4.5).
            candidate.title = card_title
                .filter(|code| {
                    let basis = DeckConstruction::Title((*code).clone());
                    probe(&native, number, evolve, &basis)
                        .map_err(|reason| {
                            excluded.push(Excluded {
                                number: number.to_owned(),
                                reason: format!("title {code}: {reason}"),
                            });
                        })
                        .is_ok()
                })
                .cloned();
            cards.push(candidate);
        }
        let pools = Pools::new(cards, &amulet_titles);
        let arena_printings = numbers
            .keys()
            .map(|printing| (printing.clone(), printing.clone()))
            .collect();
        excluded.extend(pools.unpooled().into_iter().map(|number| Excluded {
            number: number.to_owned(),
            reason: "no construction pool".into(),
        }));
        Ok(Self {
            arena: Arena {
                open: opener(prepared),
                pools,
                lineup: None,
                printings: arena_printings,
                numbers,
            },
            excluded,
            unprogrammed,
            native,
            deck_cards,
            amulet_titles,
        })
    }

    /// The arena ready to play on this export.
    #[must_use]
    pub const fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Programmed printings excluded from random construction.
    #[must_use]
    pub const fn excluded(&self) -> &[Excluded] {
        self.excluded.as_slice()
    }

    /// Deck-type printings without any program.
    #[must_use]
    pub const fn unprogrammed(&self) -> usize {
        self.unprogrammed
    }

    /// Play the decks of a list file instead of random ones; returns the decks left out.
    ///
    /// Installs the admitted lineup even when every deck was refused.
    ///
    /// # Errors
    /// The file is not a deck list.
    pub fn list(&mut self, text: &str) -> Result<Vec<Refused>, String> {
        let list: DeckList =
            serde_json::from_str(text).map_err(|error| format!("deck list: {error}"))?;
        let mut lineup = Lineup::default();
        let mut refused = Vec::new();
        let checked: Vec<_> = list
            .decks
            .into_iter()
            .enumerate()
            .map(|(position, listed)| {
                let admitted = if listed.weight == 0 {
                    Err("weight is zero".to_owned())
                } else {
                    self.admit(&listed)
                };
                let label = listed.label.unwrap_or_else(|| position.to_string());
                (label, listed.weight, admitted)
            })
            .collect();
        let title = checked.iter().find_map(|(_, _, admitted)| {
            let deck = admitted.as_ref().ok()?;
            matches!(deck.construction, Basis::Title(_)).then(|| deck.clone())
        });
        for (label, weight, admitted) in checked {
            let admitted = admitted.and_then(|deck| {
                // CR 14.4.5 can require extra facts from a class deck against any title.
                if let Some(title) = &title
                    && matches!(deck.construction, Basis::Class(_))
                {
                    self.check_open([deck.clone(), title.clone()])
                        .map_err(|stage| format!("load against a title deck: {stage}"))?;
                }
                Ok(deck)
            });
            match admitted {
                Ok(deck) => lineup.push(label, weight, deck),
                Err(reason) => refused.push(Refused { label, reason }),
            }
        }
        self.arena.lineup = Some(lineup);
        Ok(refused)
    }

    fn admit(&self, listed: &ListedDeck) -> Result<Deck, String> {
        shape(listed)?;
        let expand = |cards: &BTreeMap<String, usize>| {
            cards
                .iter()
                .flat_map(|(number, count)| repeat_n(number.clone(), *count))
                .collect::<Vec<_>>()
        };
        let (main, evolve) = (expand(&listed.main), expand(&listed.evolve));
        let basis = match &listed.construction {
            Basis::Class(class) => DeckConstruction::Class(class.clone()),
            Basis::Title(title) => DeckConstruction::Title(title.clone()),
        };
        let native = PlayerDeck {
            construction: basis,
            main,
            evolve,
        };
        if let Some(gaps) = admission_gaps(&self.native, &[native.clone(), native])? {
            return Err(format!("admission: {gaps}"));
        }
        let deck = self.construct(listed)?;
        self.check_open([deck.clone(), deck.clone()])
            .map_err(|stage| format!("load: {stage}"))?;
        Ok(deck)
    }

    fn construct(&self, listed: &ListedDeck) -> Result<Deck, String> {
        let basis = &listed.construction;
        let mut amulet = None;
        let mut section = |entries: &BTreeMap<String, usize>, evolve: bool| {
            let mut cards = Vec::new();
            let mut names = BTreeMap::<&str, usize>::new();
            for (number, count) in entries {
                let card = self
                    .deck_cards
                    .get(number)
                    .ok_or_else(|| format!("{number} is not a deck card"))?;
                if card.evolve != evolve {
                    return Err(format!("{number} is in the wrong deck section"));
                }
                let belongs = match basis {
                    Basis::Class(class) => card
                        .class
                        .as_ref()
                        .is_some_and(|printed| printed == class || printed == NEUTRAL),
                    Basis::Title(title) => card.title.as_ref() == Some(title),
                };
                if !belongs {
                    return Err(format!("{number} does not match the construction basis"));
                }
                let copies = names.entry(card.name.as_str()).or_default();
                *copies = copies.saturating_add(*count);
                if *copies > NAME_LIMIT {
                    return Err(format!(
                        "{number} exceeds {NAME_LIMIT} copies of one rules name"
                    ));
                }
                if card.start_amulet
                    && !evolve
                    && amulet
                        .replace(card.name.as_str())
                        .is_some_and(|name| name != card.name)
                {
                    return Err("Start Amulets have different rules names".into());
                }
                cards.extend(repeat_n(card.reference.clone(), *count));
            }
            Ok(cards)
        };
        let main = section(&listed.main, false)?;
        let evolve = section(&listed.evolve, true)?;
        if let Basis::Title(title) = basis
            && self.amulet_titles.contains(title)
            && amulet.is_none()
        {
            return Err("title requires a Start Amulet".into());
        }
        Ok(Deck {
            construction: basis.clone(),
            main,
            evolve,
        })
    }

    fn check_open(&self, decks: [Deck; 2]) -> Result<(), String> {
        let setup = Setup {
            decks,
            first_chooser: Seat::P1,
            seed: "0".into(),
            control: [Control::Player, Control::Player],
            max_submissions: 0,
        };
        (self.arena.open)(&setup).map(drop)
    }
}

/// Title label to registered code, and the codes whose opening needs a Start Amulet.
fn titles(
    document: &Value,
    rules: &Value,
    facts: &BTreeMap<String, Facts>,
) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let codes = document
        .pointer("/engine_identity/Resolved/faces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|face| {
            let code = face.get("title_code")?.as_str()?;
            let label = facts.get(face.get("card_no")?.as_str()?)?.title.clone()?;
            Some((label, code.to_owned()))
        })
        .collect();
    let amulets = rules
        .get("titles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|title| {
            title
                .get("capabilities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|capability| capability.get("kind") == Some(&json!("opening_start_amulet")))
        })
        .filter_map(|title| title.get("title_code")?.as_str().map(str::to_owned))
        .collect();
    (codes, amulets)
}
