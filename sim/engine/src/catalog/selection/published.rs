//! Prepare one verified snapshot's programs for reuse by matches.

use super::{CandidateAdmission, Catalog, Owner, SelectedEffects, accept, register_printing};
use crate::catalog::{EngineIdentityInput, PrintedTriggerIcon};
use crate::ir::lowering::{CurrentCardProgram, ProgramFace, lower_program};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;

/// Permanent identity and rules facts for one native printing in the same snapshot.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramPrinting {
    /// Permanent identifier of this printing.
    pub printing_id: String,
    /// The unchanged native card number.
    pub card_no: String,
    /// Permanent card that owns this printing.
    pub card_id: String,
    /// Adopted rules facts for this card's faces, in native order.
    pub faces: Vec<ProgramFace>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    format_version: String,
    entries: Vec<Entry>,
    printings: BTreeMap<String, Printings>,
    trigger_icons: BTreeMap<String, BTreeMap<String, PrintedTriggerIcon>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    dsl_version: String,
    ast: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Printings {
    representative: String,
    tokens: Vec<String>,
}

#[derive(Debug)]
struct CardProgram {
    ast: Value,
    representative: String,
    faces: Vec<ProgramFace>,
    tokens: Vec<String>,
}

/// Programmes and catalogue from a single checked manifest. Only requested cards are lowered.
#[derive(Debug)]
pub struct SnapshotPrograms {
    catalogue: Arc<Catalog>,
    programs: BTreeMap<String, Arc<CardProgram>>,
    numbers: BTreeMap<String, String>,
    candidates: Mutex<BTreeMap<String, Arc<CandidateAdmission>>>,
}

impl SnapshotPrograms {
    /// Resolve the program and identity closures against the native catalogue.
    ///
    /// # Errors
    /// Refuses malformed, incompatible or cross-snapshot projections and rules.
    pub fn new(
        snapshot: &str,
        keywords: &str,
        programs: &str,
        printings: &[ProgramPrinting],
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<Self> {
        let mut package: Package = serde_json::from_str(programs).map_err(invalid)?;
        if package.format_version != "4.2.0"
            || !matches!(identity, EngineIdentityInput::Resolved { region, .. } if region == "jp")
        {
            return Err(invalid("unsupported battle program profile or region"));
        }
        let mut catalogue = Catalog::from_snapshot(snapshot, keywords)?;
        let verified = PrintingIndex::new(&catalogue, printings)?;
        let icons = core::mem::take(&mut package.trigger_icons);
        let bound = verified.bind(package, &mut catalogue)?;
        catalogue.validate_token_templates()?;
        catalogue.bind_rules(snapshot, keywords, &[], rules, identity)?;
        verified.bind_trigger_icons(&mut catalogue, icons)?;
        Ok(Self {
            catalogue: Arc::new(catalogue),
            programs: bound,
            numbers: verified
                .by_id
                .into_iter()
                .map(|(id, row)| (id.to_owned(), row.card_no.clone()))
                .collect(),
            candidates: Mutex::default(),
        })
    }

    /// Resolve a permanent printing ID before passing a deck to the native rules.
    #[must_use]
    pub fn native_printing(&self, printing_id: &str) -> Option<&str> {
        self.numbers.get(printing_id).map(String::as_str)
    }

    /// The full native catalogue used to bind battle references.
    #[must_use]
    pub fn catalogue(&self) -> &Catalog {
        &self.catalogue
    }

    pub(crate) fn select(
        &self,
        collect: impl FnOnce(
            &Catalog,
            &mut dyn FnMut(&str) -> Result<Option<Arc<CurrentCardProgram>>>,
        ) -> Result<()>,
    ) -> Result<SelectedEffects> {
        let mut admitted = BTreeMap::new();
        collect(&self.catalogue, &mut |number| {
            let Some(candidate) = self.candidate(number)? else {
                return Ok(None);
            };
            let program = Arc::clone(candidate.candidate());
            admitted.insert(number.to_owned(), candidate);
            Ok(Some(program))
        })?;
        Ok(SelectedEffects::from_shared(
            Arc::clone(&self.catalogue),
            admitted,
        ))
    }

    /// Lower the entire package for a scenario library that can start at any position.
    ///
    /// # Errors
    /// Refuses unsupported nodes in any program.
    pub fn select_all(&self) -> Result<SelectedEffects> {
        self.select(|_, lower| {
            for number in self.programs.keys() {
                lower(number)?;
            }
            Ok(())
        })
    }

    fn candidate(&self, number: &str) -> Result<Option<Arc<CandidateAdmission>>> {
        let Some(program) = self.programs.get(number) else {
            return Ok(None);
        };
        if let Some(candidate) = self
            .candidates
            .lock()
            .map_err(invalid)?
            .get(&program.representative)
        {
            return Ok(Some(Arc::clone(candidate)));
        }
        let compiled = lower_program(
            &program.ast,
            &program.representative,
            &program.faces,
            &program.tokens,
        )
        .map(accept)
        .map_err(invalid)?;
        let mut candidates = self.candidates.lock().map_err(invalid)?;
        Ok(Some(Arc::clone(
            candidates
                .entry(program.representative.clone())
                .or_insert(compiled),
        )))
    }
}

struct PrintingIndex<'input> {
    by_id: BTreeMap<&'input str, &'input ProgramPrinting>,
    by_card: BTreeMap<&'input str, Vec<&'input ProgramPrinting>>,
}

impl<'input> PrintingIndex<'input> {
    fn new(catalogue: &Catalog, printings: &'input [ProgramPrinting]) -> Result<Self> {
        let mut by_id = BTreeMap::new();
        let mut by_card = BTreeMap::<_, Vec<&ProgramPrinting>>::new();
        let mut native = BTreeSet::new();
        for row in printings {
            let card = catalogue
                .cards
                .get(&row.card_no)
                .ok_or_else(|| invalid("projected printing missing from snapshot"))?;
            let group = by_card.entry(row.card_id.as_str()).or_default();
            if row.faces.is_empty()
                || row.faces.len() != card.faces.len()
                || row.printing_id.is_empty()
                || !native.insert(row.card_no.as_str())
                || by_id.insert(row.printing_id.as_str(), row).is_some()
                || group.first().is_some_and(|first| {
                    !row.faces
                        .iter()
                        .map(|face| &face.face_id)
                        .eq(first.faces.iter().map(|face| &face.face_id))
                })
            {
                return Err(invalid("invalid projected printing closure"));
            }
            group.push(row);
        }
        if native.len() != catalogue.cards.len() {
            return Err(invalid("projected printing closure is incomplete"));
        }
        Ok(Self { by_id, by_card })
    }

    fn bind_trigger_icons(
        &self,
        catalogue: &mut Catalog,
        icons: BTreeMap<String, BTreeMap<String, PrintedTriggerIcon>>,
    ) -> Result<()> {
        for (printing_id, faces) in icons {
            // The verified public attachment also includes EN printings.
            let Some(printing) = self.by_id.get(printing_id.as_str()) else {
                continue;
            };
            if faces.is_empty() {
                return Err(invalid("empty Trigger icon printing"));
            }
            let mut bound = BTreeMap::new();
            for (face_id, icon) in faces {
                let ordinal = printing
                    .faces
                    .iter()
                    .position(|face| face.face_id == face_id)
                    .ok_or_else(|| invalid("Trigger icon face belongs to another printing"))?;
                let label = catalogue.face(&printing.card_no, ordinal)?["title"]
                    .as_str()
                    .unwrap_or_default();
                if catalogue.rule_bindings.title_code("", label)?.as_deref()
                    != Some("cardfight_vanguard")
                {
                    return Err(invalid("Trigger icon face is outside Vanguard"));
                }
                bound.insert(ordinal, icon);
            }
            catalogue
                .trigger_icons
                .insert(printing.card_no.clone(), bound);
        }
        Ok(())
    }

    fn bind(
        &self,
        package: Package,
        catalogue: &mut Catalog,
    ) -> Result<BTreeMap<String, Arc<CardProgram>>> {
        let mut programs = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut owner_ids = BTreeSet::new();
        let mut dependencies = BTreeSet::new();
        for entry in package.entries {
            let card_id = entry
                .ast
                .get("card_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("invalid program card identity"))?;
            if entry.dsl_version != "1.0"
                || !ids.insert(entry.id)
                || !owner_ids.insert(card_id.to_owned())
            {
                return Err(invalid("invalid or repeated DSL program"));
            }
            let bindings = package
                .printings
                .get(card_id)
                .ok_or_else(|| invalid("program printing references missing"))?;
            // The verified package covers both regions, while this catalogue is JP.
            if !self.by_card.contains_key(card_id)
                && !self.by_id.contains_key(bindings.representative.as_str())
            {
                continue;
            }
            let representative = self
                .by_id
                .get(bindings.representative.as_str())
                .filter(|row| row.card_id == card_id)
                .ok_or_else(|| invalid("invalid program representative printing"))?;
            let tokens = bindings
                .tokens
                .iter()
                .map(|id| {
                    self.by_id
                        .get(id.as_str())
                        .map(|row| row.card_no.clone())
                        .ok_or_else(|| invalid("program token printing missing"))
                })
                .collect::<Result<Vec<_>>>()?;
            dependencies.extend(tokens.iter().cloned());
            let owner = Owner {
                card_id,
                card_no: &representative.card_no,
                token: entry.ast.get("token"),
                faces: representative.faces.len(),
            };
            let printings = self
                .by_card
                .get(card_id)
                .ok_or_else(|| invalid("missing program owner printings"))?;
            for row in printings {
                let names = row
                    .faces
                    .iter()
                    .map(|face| face.rules_name.clone())
                    .collect::<Vec<_>>();
                register_printing(catalogue, &row.card_no, &owner, &names)?;
            }
            let program = Arc::new(CardProgram {
                ast: entry.ast,
                representative: representative.card_no.clone(),
                faces: representative.faces.clone(),
                tokens,
            });
            for row in printings {
                programs.insert(row.card_no.clone(), Arc::clone(&program));
            }
        }
        if owner_ids.len() != package.printings.len()
            || dependencies.into_iter().any(|number| {
                !programs.contains_key(&number)
                    || !catalogue.token_definitions.contains_key(&number)
            })
        {
            return Err(invalid("program printing references are incomplete"));
        }
        Ok(programs)
    }
}
