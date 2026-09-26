use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Frame, Game, other, string};
use crate::{EngineFailure, Result, invalid};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnDirective {
    player: String,
    count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct TurnSchedule {
    extra: Vec<TurnDirective>,
    skipped: Vec<TurnDirective>,
    normal: Option<String>,
}

impl TurnSchedule {
    pub(super) fn load(value: &Value) -> Result<Self> {
        let schedule: Self = if value.is_null() {
            Self::default()
        } else {
            serde_json::from_value(value.clone()).map_err(invalid)?
        };
        if schedule
            .normal
            .as_ref()
            .is_some_and(|player| !matches!(player.as_str(), "P1" | "P2"))
            || schedule
                .extra
                .iter()
                .chain(&schedule.skipped)
                .any(|directive| {
                    !matches!(directive.player.as_str(), "P1" | "P2") || directive.count == 0
                })
        {
            return Err(invalid(
                "turn directives require a player and a positive count",
            ));
        }
        schedule.check_size()?;
        Ok(schedule)
    }

    fn check_size(&self) -> Result<()> {
        let count = self
            .extra
            .iter()
            .chain(&self.skipped)
            .fold(0_u64, |total, directive| {
                total.saturating_add(u64::from(directive.count))
            });
        if count > 10_000 {
            return Err(EngineFailure::Unsupported(
                "turn schedule exceeds prototype limit".into(),
            ));
        }
        Ok(())
    }

    fn pop_extra(&mut self) -> Option<String> {
        let directive = self.extra.last_mut()?;
        let player = directive.player.clone();
        directive.count = directive.count.saturating_sub(1);
        if directive.count == 0 {
            self.extra.pop();
        }
        Some(player)
    }

    fn consume_skip(&mut self, player: &str) -> bool {
        let Some(index) = self
            .skipped
            .iter()
            .rposition(|directive| directive.player == player)
        else {
            return false;
        };
        let Some(directive) = self.skipped.get_mut(index) else {
            return false;
        };
        directive.count = directive.count.saturating_sub(1);
        if directive.count == 0 {
            self.skipped.remove(index);
        }
        true
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Turn scheduling owns ordering before the common start-of-turn procedure."
)]
impl Game {
    pub(super) fn schedule_turns(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let count = u32::try_from(self.number(&node["count"], frame)?.max(0)).map_err(invalid)?;
        frame.performed = 0;
        if count == 0 {
            return Ok(());
        }
        for player in self.seats(string(&node["side"]), frame) {
            let destination = if node["op"] == "extra_turn" {
                &mut self.state.schedule.extra
            } else {
                &mut self.state.schedule.skipped
            };
            destination.push(TurnDirective { player, count });
            frame.performed = frame.performed.saturating_add(i64::from(count));
        }
        self.state.schedule.check_size()
    }

    pub(super) fn next_scheduled_player(&mut self) -> Result<String> {
        self.state.schedule.check_size()?;
        let mut normal = self
            .state
            .schedule
            .normal
            .take()
            .unwrap_or_else(|| other(self.active()).into());
        loop {
            let extra = self.state.schedule.pop_extra();
            let candidate = extra.clone().unwrap_or_else(|| normal.clone());
            if extra.is_none() {
                normal = other(&candidate).into();
            }
            if self.state.schedule.consume_skip(&candidate) {
                let group = self.group();
                self.emit(
                    json!({"kind":"ターンスキップ","player":candidate,"extra":extra.is_some()}),
                    &json!({"rule":"5.26.2"}),
                    group,
                );
            } else {
                self.state.schedule.normal = extra.map(|_| normal);
                return Ok(candidate);
            }
        }
    }
}
