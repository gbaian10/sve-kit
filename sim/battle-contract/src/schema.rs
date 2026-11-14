//! JSON Schema of the contract: the single source of the TypeScript types and validator.

use crate::answer::ClientMessage;
use crate::message::HostMessage;
use crate::observation::BattleObservation;
use crate::question::Question;
use schemars::generate::SchemaSettings;
use schemars::transform::transform_subschemas;
use schemars::{Schema, json_schema};
use serde_json::Value;

/// The schema of every message, with shared definitions under `$defs`.
#[must_use]
pub fn schema() -> Schema {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    let roots = [
        generator.subschema_for::<HostMessage>(),
        generator.subschema_for::<ClientMessage>(),
        generator.subschema_for::<BattleObservation>(),
        generator.subschema_for::<Question>(),
    ];
    let mut schema = json_schema!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "BattleContract",
        "anyOf": roots,
        "$defs": generator.take_definitions(true),
    });
    require_every_property(&mut schema);
    schema
}

/// The generator leaves nullable fields optional, but the wire has no optional field:
/// an inapplicable value is an explicit `null`.
fn require_every_property(schema: &mut Schema) {
    if let Some(object) = schema.as_object_mut() {
        let names = object
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.keys().cloned().map(Value::String).collect());
        if let Some(names) = names {
            object.insert("required".to_owned(), Value::Array(names));
        }
    }
    transform_subschemas(&mut require_every_property, schema);
}
