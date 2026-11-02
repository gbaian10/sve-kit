use super::{
    Ability, AbilityId, Allocator, CardCoordinates, KeywordId, Located, LoweringError, ast,
    source_span, unsupported,
};
use crate::ir::{DecisionId, FaceId, InstructionId, Nat, printed_keywords};

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
            let node = if keyword == KeywordId::Stack {
                Some(printed_keywords::stack_definition(
                    AbilityId(scope.0),
                    scope,
                    DecisionId(allocator.id(path)?),
                    InstructionId(allocator.id(path)?),
                    InstructionId(allocator.id(path)?),
                    &source,
                ))
            } else if matches!(keyword, KeywordId::SingleDrive | KeywordId::TwinDrive) {
                printed_keywords::drive_definition(
                    keyword,
                    AbilityId(scope.0),
                    scope,
                    InstructionId(allocator.id(path)?),
                    &source,
                )
            } else if keyword == KeywordId::Drain {
                Some(printed_keywords::drain_definition(
                    AbilityId(scope.0),
                    scope,
                    InstructionId(allocator.id(path)?),
                    &source,
                ))
            } else {
                printed_keywords::definition(
                    keyword,
                    FaceId(source.face.clone()),
                    Nat(u64::from(source.line)),
                    AbilityId(scope.0),
                    scope,
                )
            }
            .ok_or_else(|| unsupported(path))?;
            Ok(Located { node, source })
        })
        .collect()
}

pub(super) fn keyword_id(keyword: &str) -> Option<KeywordId> {
    match keyword {
        "stack" => Some(KeywordId::Stack),
        "ward" => Some(KeywordId::Ward),
        "aura" => Some(KeywordId::Aura),
        "storm" => Some(KeywordId::Storm),
        "rush" => Some(KeywordId::Rush),
        "bane" => Some(KeywordId::Bane),
        "drain" => Some(KeywordId::Drain),
        "intimidate" => Some(KeywordId::Intimidate),
        "assail" => Some(KeywordId::Assail),
        "single_drive" => Some(KeywordId::SingleDrive),
        "twin_drive" => Some(KeywordId::TwinDrive),
        _ => None,
    }
}

pub(super) fn grantable_keyword_id(name: &str) -> Option<KeywordId> {
    if name == "drain" {
        Some(KeywordId::Drain)
    } else {
        basic_keyword_id(name)
    }
}

pub(super) fn basic_keyword_id(name: &str) -> Option<KeywordId> {
    let keyword = keyword_id(name)?;
    matches!(
        keyword,
        KeywordId::Ward
            | KeywordId::Aura
            | KeywordId::Storm
            | KeywordId::Rush
            | KeywordId::Bane
            | KeywordId::Intimidate
            | KeywordId::Assail
    )
    .then_some(keyword)
}
