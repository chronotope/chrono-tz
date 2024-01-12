use crate::{Tz, TZ_VARIANTS};

impl schemars::JsonSchema for Tz {
    fn schema_name() -> String {
        "Tz".into()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::Schema::Object(schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            enum_values: Some(TZ_VARIANTS.iter().map(|variant| variant.name().into()).collect()),
            ..Default::default()
        })
    }

    fn is_referenceable() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::Tz;

    #[test]
    fn json_schema() {
        let schema = jsonschema::JSONSchema::compile(
            &serde_json::to_value(schemars::schema_for!(Tz)).unwrap(),
        )
        .expect("a valid schema");

        assert!(schema.is_valid(&serde_json::Value::from("Europe/London")));
        assert!(schema.is_valid(&serde_json::Value::from("Africa/Abidjan")));
        assert!(schema.is_valid(&serde_json::Value::from("UTC")));
        assert!(schema.is_valid(&serde_json::Value::from("Zulu")));

        assert!(!schema.is_valid(&serde_json::Value::from("MadeUpInvalidTimezone")));
    }
}
