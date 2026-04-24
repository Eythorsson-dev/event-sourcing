use event_sourcing::{Event as _, EventSchemaDef, FieldDef, FieldType};
use event_sourcing_macros::Event;

#[derive(Event)]
struct OrderPlaced {
    order_id: String,
    amount: f64,
    item_count: i32,
}

#[test]
fn derive_event_basic_struct() {
    assert_eq!(OrderPlaced::event_type(), "OrderPlaced");
    let schema: EventSchemaDef = OrderPlaced::schema();
    assert_eq!(schema.event_type.as_str(), "OrderPlaced");
    assert_eq!(schema.fields.len(), 3);

    let order_id_field = schema.fields.iter().find(|f| f.name == "order_id").unwrap();
    assert_eq!(order_id_field.field_type, FieldType::String);
    assert!(!order_id_field.optional);

    let amount_field = schema.fields.iter().find(|f| f.name == "amount").unwrap();
    assert_eq!(amount_field.field_type, FieldType::Decimal);
    assert!(!amount_field.optional);

    let item_count_field = schema
        .fields
        .iter()
        .find(|f| f.name == "item_count")
        .unwrap();
    assert_eq!(item_count_field.field_type, FieldType::Integer);
    assert!(!item_count_field.optional);
}

#[derive(Event)]
struct CustomerRegistered {
    name: String,
    middle_name: Option<String>,
}

#[test]
fn derive_event_option_field() {
    assert_eq!(CustomerRegistered::event_type(), "CustomerRegistered");
    let schema: EventSchemaDef = CustomerRegistered::schema();
    assert_eq!(schema.fields.len(), 2);

    let name_field = schema.fields.iter().find(|f| f.name == "name").unwrap();
    assert_eq!(name_field.field_type, FieldType::String);
    assert!(!name_field.optional);

    let middle_field = schema
        .fields
        .iter()
        .find(|f| f.name == "middle_name")
        .unwrap();
    assert_eq!(middle_field.field_type, FieldType::String);
    assert!(middle_field.optional);
}

#[derive(Event)]
struct AllTypesEvent {
    str_field: String,
    int8_field: i8,
    int16_field: i16,
    int32_field: i32,
    int64_field: i64,
    uint32_field: u32,
    float32_field: f32,
    float64_field: f64,
    opt_str_field: Option<String>,
}

#[test]
fn derive_event_all_supported_types() {
    let schema: EventSchemaDef = AllTypesEvent::schema();
    assert_eq!(schema.fields.len(), 9);

    let get_field =
        |name: &str| -> &FieldDef { schema.fields.iter().find(|f| f.name == name).unwrap() };

    assert_eq!(get_field("str_field").field_type, FieldType::String);
    assert_eq!(get_field("int8_field").field_type, FieldType::Integer);
    assert_eq!(get_field("int16_field").field_type, FieldType::Integer);
    assert_eq!(get_field("int32_field").field_type, FieldType::Integer);
    assert_eq!(get_field("int64_field").field_type, FieldType::Integer);
    assert_eq!(get_field("uint32_field").field_type, FieldType::Integer);
    assert_eq!(get_field("float32_field").field_type, FieldType::Decimal);
    assert_eq!(get_field("float64_field").field_type, FieldType::Decimal);
    assert_eq!(get_field("opt_str_field").field_type, FieldType::String);
    assert!(get_field("opt_str_field").optional);
}
