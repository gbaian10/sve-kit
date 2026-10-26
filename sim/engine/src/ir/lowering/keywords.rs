use super::{
    Ability, AbilityId, Allocator, CardCoordinates, KeywordId, Located, LoweringError, ast,
    source_span, unsupported,
};
use crate::ir::{FaceId, Nat, printed_keywords};

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    keywords: &[String],
) -> Result<Vec<Located<Ability>>, LoweringError> {
    // Validate the whole ordered list before allocating or producing any definition.
    if keywords.is_empty()
        || keywords
            .iter()
            .any(|keyword| !matches!(keyword.as_str(), "ward" | "aura"))
    {
        return Err(unsupported(&format!("{path}/keywords")));
    }
    keywords
        .iter()
        .enumerate()
        .map(|(index, keyword)| {
            let mut source = source_span(key, path, entry)?;
            source.author_path = format!("{path}/keywords/{index}");
            let scope = allocator.scope(path)?;
            let keyword = if keyword == "ward" {
                KeywordId::Ward
            } else {
                KeywordId::Aura
            };
            let node = printed_keywords::definition(
                keyword,
                FaceId(source.face.clone()),
                Nat(u64::from(source.line)),
                AbilityId(scope.0),
                scope,
            )
            .ok_or_else(|| unsupported(path))?;
            Ok(Located { node, source })
        })
        .collect()
}
