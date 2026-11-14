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
fn turn_scheduling_rejects_other_players_or_an_unbound_anchor() {
    WellFormedProgram::check(body(1, vec![instruction(1, insertion())])).unwrap();
    for variant in 0_u8..2 {
        let mut action = insertion();
        let Action::ScheduleTurn {
            player, operation, ..
        } = &mut action
        else {
            panic!("schedule fixture")
        };
        match variant {
            0 => *player = PlayerRef::Active {},
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
