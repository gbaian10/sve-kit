//! Stack increase expands into ordinary selection, counters and token creation.
use super::{AbilitySource, BodyContext, Located, LoweringError, SourceSpan, ast, unsupported};
use crate::ir::{
    Action, Binding, CardName, ChoiceDomain, Control, CounterKind, CounterOperation, DecisionId,
    Expr, Holder, Instruction, InstructionContext, InstructionId, Nat, Node, ObjectOperand,
    PlayerRef, Posture, Predicate, Program, ReceiptId, TokenDestination, TokenInitialState,
    TokenSpec, TokenTemplateId, ZoneKind, ZoneRef, printed_keywords,
};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The rule expansion shares the body's allocators and source."
)]
impl BodyContext<'_> {
    pub(super) fn stack_plus(
        &mut self,
        amount: &ast::Number,
        path: &str,
        source: &SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let ast::Number::Literal(amount) = amount else {
            return Err(unsupported(path));
        };
        let amount = u64::try_from((*amount).max(0)).map_err(|_private| unsupported(path))?;
        i64::try_from(amount).map_err(|_private| unsupported(path))?;
        let scope = self.card_allocator.scope(path)?;
        let mut located = source.clone();
        path.clone_into(&mut located.author_path);
        let branch = |steps| Program { scope, steps };
        if amount == 0 {
            return Ok(Located {
                source: located,
                node: Node::Control {
                    control: Control::Sequence {
                        body: branch(vec![]),
                    },
                },
            });
        }
        let decision = printed_keywords::stack_increase_choice(
            DecisionId(self.card_allocator.id(path)?),
            scope,
        );
        let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
            return Err(unsupported(path));
        };
        let selected = *binding;
        let else_scope = self.card_allocator.scope(path)?;
        let condition_scope = self.card_allocator.scope(path)?;
        let recipients = printed_keywords::stack_recipients(condition_scope, false);
        let mut instruction = |action| -> Result<Located<Node>, LoweringError> {
            let id = self.card_allocator.id(path)?;
            Ok(Located {
                source: located.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(id),
                        receipt: ReceiptId(id),
                        actor: PlayerRef::Controller {},
                        source: origin.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        action,
                    }),
                },
            })
        };
        let increase = instruction(Action::Counter {
            holder: Holder::Object {
                object: ObjectOperand::Selected {
                    selection: selected,
                },
            },
            kind: CounterKind("stack_counter".into()),
            operation: CounterOperation::Add {},
            amount: Expr::Literal(Nat(amount)),
        })?;
        let output = Binding::new(2, else_scope);
        let create = instruction(Action::Token {
            tokens: vec![TokenSpec {
                template: TokenTemplateId("大地の魔片".into()),
                name: Expr::Literal(CardName("大地の魔片".into())),
                count: Expr::Literal(Nat(1)),
            }],
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                }),
            },
            output,
            initial_state: Some(TokenInitialState::StackCounters {
                amount: Expr::Literal(Nat(amount)),
            }),
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        })?;
        Ok(Located {
            source: located.clone(),
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Compute(Box::new(Predicate::Exists {
                        objects: recipients,
                    })),
                    then_body: branch(vec![
                        Located {
                            source: located,
                            node: Node::Decision { decision },
                        },
                        increase,
                    ]),
                    else_body: Some(Program {
                        scope: else_scope,
                        steps: vec![create],
                    }),
                },
            },
        })
    }
}
