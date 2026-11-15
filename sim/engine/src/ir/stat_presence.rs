//! Presence checks lend no authority beyond an existing source, public pick or event number.
use super::{CharacteristicView, NumberRead, ObjectOperand, ObjectView};

pub(crate) fn supported(information: &NumberRead) -> bool {
    matches!(
        information,
        NumberRead::Stats {
            object: ObjectView::SourceInformation {}
                | ObjectView::EventSnapshot { .. }
                | ObjectView::LastKnown { .. },
            view: CharacteristicView::Effective | CharacteristicView::Original,
            ..
        }
    ) || matches!(information, NumberRead::Stats {
            object: ObjectView::Current { object }, view: CharacteristicView::Effective, ..
        } if matches!(object.as_ref(), ObjectOperand::SourceHolder {} | ObjectOperand::Selected { .. }))
}
