use alloc::collections::BTreeMap;
use alloc::sync::Arc;

use serde_json::{Value, json};
use sve_scenario_runner::ai::{AiEngine, AiReport};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

use super::{core_view, error};
use crate::ai::{self, Profile, Report};
use crate::catalog::Catalog;
use crate::game::{Game, View as CoreView};

/// Format-only bridge; the AI receives the core's exact outgoing player packet.
#[derive(Debug, Clone)]
pub struct AiAdapter {
    catalog: Arc<Catalog>,
    profiles: BTreeMap<String, Profile>,
    game: Option<Game>,
    /// Most recent core search report, retained verbatim for the external audit.
    report: Option<Report>,
}
impl AiAdapter {
    /// Uses caller-selected profiles and the same immutable validated catalogue as matches.
    #[must_use]
    pub const fn new(catalog: Arc<Catalog>, profiles: BTreeMap<String, Profile>) -> Self {
        Self {
            catalog,
            profiles,
            game: None,
            report: None,
        }
    }
    /// Last unchanged core report, including the engine-written search forest.
    #[must_use]
    pub const fn report(&self) -> Option<&Report> {
        self.report.as_ref()
    }

    fn game(&self) -> Result<&Game, EngineError> {
        self.game
            .as_ref()
            .ok_or_else(|| EngineError::Adapter("AI match not loaded".into()))
    }
}
impl AiEngine for AiAdapter {
    fn load(&mut self, fixture: &Fixture, seed: &str) -> Result<(), EngineError> {
        self.game = Some(
            Game::new(
                Arc::clone(&self.catalog),
                &fixture.setup,
                &fixture.card_facts,
                &fixture.random,
                seed,
            )
            .map_err(error)?,
        );
        self.report = None;
        Ok(())
    }
    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        let step = self
            .game
            .as_mut()
            .ok_or_else(|| EngineError::Adapter("AI match not loaded".into()))?
            .submit(decision)
            .map_err(error)?;
        Ok(Step {
            outcome: step.outcome,
            events: step.events,
        })
    }
    fn legal(&self) -> Result<Vec<Value>, EngineError> {
        self.game()?.legal().map_err(error)
    }
    fn projection(&self, view: View) -> Result<Value, EngineError> {
        self.game()?.projection(core_view(view)).map_err(error)
    }
    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.game()?.query(core_view(view), path).map_err(error)
    }
    fn awaiting(&self) -> Result<Option<Value>, EngineError> {
        let packet = self.game()?.projection(CoreView::Referee).map_err(error)?;
        Ok(packet
            .get("awaiting")
            .filter(|a| !a.is_null())
            .map(|a| json!({"by":a["by"],"at":a["at"]})))
    }
    fn think(
        &mut self,
        seat: View,
        profile: &str,
        budget: u64,
        seed: &str,
    ) -> Result<AiReport, EngineError> {
        let profile = self
            .profiles
            .get(profile)
            .ok_or_else(|| EngineError::Unsupported("unknown deck profile".into()))?;
        let packet = self.game()?.projection(core_view(seat)).map_err(error)?;
        let report = ai::think(
            &self.catalog,
            &packet,
            core_view(seat),
            profile,
            budget,
            seed,
        )
        .map_err(error)?;
        let output = AiReport {
            decision: report.decision.clone(),
            candidates: report.candidates.clone(),
            trace: report.trace.clone(),
            edges: u64::try_from(report.log.entries.len()).unwrap_or(u64::MAX),
            engine_steps: report.log.engine_steps,
            millis: report.millis,
        };
        self.report = Some(report);
        Ok(output)
    }
}
