pub mod dynamo;

use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

/// Provider-specific request fields flattened into a Responses API request body.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[serde(transparent)]
pub struct ProviderRequestFields(pub Map<String, Value>);

impl ProviderRequestFields {
    pub fn insert_serialized_field<T: Serialize>(
        &mut self,
        name: &str,
        value: T,
    ) -> serde_json::Result<()> {
        self.0
            .insert(name.to_string(), serde_json::to_value(value)?);
        Ok(())
    }
}
