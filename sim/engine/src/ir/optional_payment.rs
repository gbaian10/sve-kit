//! One optional payment declared before targets and committed before play.
use super::{
    Ability, AbilityOrigin, Bool, Comparison, Control, CostSpec, Expr, Int, IntRead, KeywordId,
    Located, Node, PaymentOptionality, PaymentTime, PlaySpec, PlayerRef, PlayerResource, Predicate,
    Program, ReceiptId,
};

pub(crate) fn declaration(body: &Program) -> Option<(&CostSpec, ReceiptId)> {
    let Node::Control {
        control:
            Control::Pay {
                at: PaymentTime::PlayParameter,
                costs,
                on_paid,
                receipt,
                ..
            },
    } = &body.steps.first()?.node
    else {
        return None;
    };
    on_paid.steps.is_empty().then_some((costs, *receipt))
}

pub(crate) fn required(body: &Program) -> bool {
    matches!(
        body.steps.first().map(|step| &step.node),
        Some(Node::Control {
            control: Control::Pay {
                optional: PaymentOptionality::Required,
                ..
            }
        })
    )
}

pub(crate) fn trigger_play(ability: &Ability) -> bool {
    ability.play.as_ref().is_none_or(|play| {
        if super::events::played::filter(ability).is_some() {
            return matches!(play, PlaySpec { limit: None, captures, parameters, costs: CostSpec::Ordered { steps } }
                if captures.is_empty() && parameters.is_empty() && steps.is_empty())
                && ability.body.as_ref().is_some_and(|body| required(body) && declaration(body).is_some());
        }
        let AbilityOrigin::Keyword { keyword, .. } = &ability.source.origin else {
            return false;
        };
        matches!(play, PlaySpec { limit: None, captures, costs: CostSpec::Ordered { steps }, .. }
            if captures.is_empty() && steps.is_empty())
            && ability.body.as_ref().is_some_and(|body| {
                if required(body) {
                    declaration(body).is_some()
                        && matches!(
                            keyword,
                            KeywordId::Fanfare
                                | KeywordId::Lastword
                                | KeywordId::OnEvolve
                                | KeywordId::OnSuperEvolve
                                | KeywordId::OnAttack
                                | KeywordId::OnRace
                                | KeywordId::OnDriveGain
                        )
                        && (*keyword != KeywordId::Lastword || play.parameters.is_empty())
                } else {
                    match declaration(body) {
                        Some(_) => matches!(keyword, KeywordId::Fanfare),
                        None if !play.parameters.is_empty() => matches!(
                            keyword,
                            KeywordId::Fanfare
                                | KeywordId::Lastword
                                | KeywordId::OnEvolve
                                | KeywordId::OnSuperEvolve
                                | KeywordId::OnAttack
                        ),
                        None => {
                            matches!(keyword, KeywordId::Fanfare | KeywordId::OnEvolve)
                                && declared_choice(body, matches!(keyword, KeywordId::Fanfare))
                        }
                    }
                }
            })
    })
}

fn declared_choice(body: &Program, conditional: bool) -> bool {
    match body.steps.as_slice() {
        [
            Located {
                node:
                    Node::Control {
                        control: Control::Mode { .. },
                    },
                ..
            },
        ] => true,
        [
            Located {
                node:
                    Node::Control {
                        control:
                            Control::If {
                                then_body,
                                else_body: None,
                                ..
                            },
                    },
                ..
            },
        ] if conditional => declared_choice(then_body, conditional),
        _ => false,
    }
}

pub(crate) fn public_mode_guard(condition: &Expr<Bool>) -> bool {
    matches!(condition, Expr::Compute(predicate)
    if matches!(predicate.as_ref(), Predicate::CompareInt {
        left: Expr::Live(read), comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(minimum)),
    } if *minimum >= 0 && matches!(read.as_ref(), IntRead::PlayerResource {
        player: PlayerRef::Controller {}, property: PlayerResource::PpMaximum,
    })))
}
