use super::{BodyContext, LoweringError, RecipientSet, ast, numeric, unsupported};

fn contains_source(recipients: &ast::Recipients) -> bool {
    match recipients {
        ast::Recipients::Target(name) => name == "this",
        ast::Recipients::Union { any_of } => any_of.iter().any(contains_source),
        ast::Recipients::Role { .. }
        | ast::Recipients::Field(_)
        | ast::Recipients::Leader { .. }
        | ast::Recipients::LeaderOf { .. }
        | ast::Recipients::Leaders { .. } => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Source recipient unions share the formal target binding environment."
)]
impl BodyContext<'_> {
    pub(super) fn recipient_union(
        &mut self,
        limbs: &[ast::Recipients],
        path: &str,
    ) -> Result<RecipientSet, LoweringError> {
        if limbs.iter().any(contains_source) {
            let [
                ast::Recipients::Target(first),
                ast::Recipients::Target(second),
            ] = limbs
            else {
                return Err(unsupported(path));
            };
            let target = match (first.as_str(), second.as_str()) {
                ("this", name) | (name, "this") if self.follower_targets.contains(name) => name,
                _ => return Err(unsupported(path)),
            };
            return Ok(RecipientSet::Union {
                sets: vec![
                    self.recipients(&ast::Recipients::Target(target.into()), path)?,
                    RecipientSet::Objects {
                        objects: numeric::holder(),
                    },
                ],
            });
        }
        Ok(RecipientSet::Union {
            sets: limbs
                .iter()
                .map(|limb| self.recipients(limb, path))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
