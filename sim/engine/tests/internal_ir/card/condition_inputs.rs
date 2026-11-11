use super::*;

pub(super) fn unprepared_cost(binding: Binding<ObjectRef>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Cost {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Original,
            },
        })),
        comparison: Comparison::LessEqual,
        right: Expr::Live(Box::new(IntRead::PlayerResource {
            player: PlayerRef::Controller {},
            property: PlayerResource::PpMaximum,
        })),
    }))
}
