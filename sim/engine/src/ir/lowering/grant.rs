use super::{BodyContext, LoweringError, SourceSpan, ast, keywords, numeric, unsupported};
use crate::ir::{Action, FaceId, GrantedAbility, ObjectOperand, ObjectView, TextProvider};
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Keyword grants use the body's object bindings."
)]
impl BodyContext<'_> {
    pub(super) fn grant(
        &mut self,
        value: &ast::Grant,
        source: &SourceSpan,
        path: &str,
    ) -> Result<Action, LoweringError> {
        if value.keywords.is_empty()
            || !matches!(
                value.until,
                None | Some(ast::Until::Game | ast::Until::Turn)
            )
        {
            return Err(unsupported(path));
        }
        let abilities = value
            .keywords
            .iter()
            .map(|id| {
                keywords::keyword_id(id)
                    .map(|keyword| GrantedAbility::Keyword { keyword })
                    .ok_or_else(|| unsupported(path))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Action::Grant {
            subjects: self.objects(&value.on, path)?,
            abilities,
            duration: numeric::duration(value.until.as_ref()),
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId(source.face.clone()),
                ability: self.definition,
            },
        })
    }
}
