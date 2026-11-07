use super::*;

const fn insertion() -> Action {
    Action::ScheduleTurn {
        player: PlayerRef::Controller {},
        operation: TurnOperation::InsertBefore {
            anchor: ScheduleAnchor::NextTurn {},
        },
        occurrence: Binding::new(99, ScopeId(1)),
    }
}

#[test]
fn extra_turn_cannot_schedule_other_players_skip_or_an_unbound_anchor() {
    WellFormedProgram::check(body(1, vec![instruction(1, insertion())])).unwrap();
    for variant in 0_u8..3 {
        let mut action = insertion();
        let Action::ScheduleTurn {
            player, operation, ..
        } = &mut action
        else {
            panic!("schedule fixture")
        };
        match variant {
            0 => *player = PlayerRef::Active {},
            1 => *operation = TurnOperation::SkipNext {},
            _ => {
                *operation = TurnOperation::InsertBefore {
                    anchor: ScheduleAnchor::Bound {
                        binding: Binding::new(100, ScopeId(1)),
                    },
                }
            }
        }
        WellFormedProgram::check(body(1, vec![instruction(1, action)])).unwrap_err();
    }
    let mut step = instruction(1, insertion());
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("instruction fixture")
    };
    instruction.actor = PlayerRef::Owner {};
    WellFormedProgram::check(body(1, vec![step])).unwrap_err();
}
