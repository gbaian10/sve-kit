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
    if keywords.is_empty() || keywords.iter().any(|keyword| keyword_id(keyword).is_none()) {
        return Err(unsupported(&format!("{path}/keywords")));
    }
    keywords
        .iter()
        .enumerate()
        .map(|(index, keyword)| {
            let mut source = source_span(key, path, entry)?;
            source.author_path = format!("{path}/keywords/{index}");
            let scope = allocator.scope(path)?;
            let keyword = keyword_id(keyword).ok_or_else(|| unsupported(path))?;
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

fn keyword_id(keyword: &str) -> Option<KeywordId> {
    match keyword {
        "ward" => Some(KeywordId::Ward),
        "aura" => Some(KeywordId::Aura),
        "storm" => Some(KeywordId::Storm),
        "rush" => Some(KeywordId::Rush),
        "bane" => Some(KeywordId::Bane),
        "intimidate" => Some(KeywordId::Intimidate),
        "assail" => Some(KeywordId::Assail),
        _ => None,
    }
}
