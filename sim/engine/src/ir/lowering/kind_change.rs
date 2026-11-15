use super::{
    Ability, AbilityId, Allocator, BodyContext, Located, LoweringError, ScopeId, ast, unsupported,
};
use crate::ir::{
    AbilityKind, AbilityQualification, Action, CardKind, CharacteristicPatch, EffectDefinitionId,
    EffectReference, ZoneKind, kind_change,
};
use core::mem::swap;

const fn kind(value: ast::FieldKind) -> CardKind {
    match value {
        ast::FieldKind::Follower => CardKind::Follower,
        ast::FieldKind::Amulet => CardKind::Amulet,
    }
}

pub(super) fn lower_at_source(
    allocator: &mut Allocator,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
    source: super::SourceSpan,
    effects: &[ast::StaticEffect],
    bindings: &mut Allocator,
) -> Result<Located<Ability>, LoweringError> {
    let head = entry.r#static.as_ref().ok_or_else(|| unsupported(path))?;
    if head
        .zone
        .as_ref()
        .is_some_and(|zones| zones.single() != Some(ZoneKind::Field))
    {
        return Err(unsupported(path));
    }
    let origin = super::ability_source(&source, None);
    let empty = ast::Body::default();
    let mut context = BodyContext::new(entry, scope, &empty, None, allocator);
    swap(&mut context.allocator, bindings);
    let condition = head
        .r#while
        .as_ref()
        .map(|value| context.qualification(value, path))
        .transpose()?;
    let mut declarations = vec![];
    for effect in effects {
        let ast::StaticEffect::MakeType(value) = effect else {
            return Err(unsupported(path));
        };
        if !matches!(value.on, None | Some(ast::Objects::This(_))) {
            return Err(unsupported(path));
        }
        let declaration = kind_change::declaration(
            origin.clone(),
            kind(value.kind),
            condition.clone(),
            EffectDefinitionId(context.card_allocator.id(path)?),
        );
        if kind_change::projection(&declaration).is_none() {
            return Err(unsupported(path));
        }
        declarations.push(declaration);
    }
    swap(&mut context.allocator, bindings);
    Ok(Located {
        source,
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Static,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            play: None,
            trigger: None,
            body: None,
            declarations,
        },
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Kind changes share the body's selected object bindings."
)]
impl BodyContext<'_> {
    pub(super) fn make_type(
        &mut self,
        value: &ast::MakeType,
        path: &str,
    ) -> Result<Action, LoweringError> {
        Ok(Action::Modify {
            subjects: self.objects(&value.on, path)?,
            patch: CharacteristicPatch::SetKind {
                kind: kind(value.kind),
            },
            expires: super::numeric::duration(value.until.as_ref(), path)?,
            source: EffectReference::CurrentInstallation {},
        })
    }
}
