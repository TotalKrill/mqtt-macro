#![allow(unused)]
use enum_impl::impl_for_enum;
use proc_macro2::{Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use proc_macro_error::{abort, proc_macro_error};
use syn::{
    punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Ident, Item, Lit, Meta,
    MetaList, NestedMeta,
};

mod enum_impl;

pub(crate) type TokenVec = Vec<TokenStream>;

/// A part of a topic
#[derive(Eq, Debug)]
enum TopicPart {
    /// Can be an ident (+)
    Ident(String),
    /// A literal string
    Literal(String),
}

impl PartialEq for TopicPart {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(selfstring), Self::Literal(otherstring)) => selfstring == otherstring,
            // all other cases means that we are matching an ident (+) layer to a fixed layer, which means collision
            _ => true,
        }
    }
}

/// Internal topic type used to build topic (de/ser)ialization and topic tree
#[derive(Debug)]
struct Topic {
    /// All the parts of this topic
    parts: Vec<TopicPart>,
}

impl PartialEq for Topic {
    fn eq(&self, other: &Self) -> bool {
        if self.parts.len() != other.parts.len() {
            false
        } else {
            let zip = self.parts.iter().zip(other.parts.iter());
            for (selfpart, otherpart) in zip {
                if selfpart != otherpart {
                    return false;
                }
            }
            true
        }
    }
}

impl Topic {
    fn push(&mut self, value: TopicPart) {
        self.parts.push(value)
    }

    fn from_string<T: Spanned>(input: &str, on: &T) -> Self {
        let mut me = Topic { parts: Vec::new() };

        if input != "" {
            let mut parts = input.split("/");

            while let Some(part) = parts.next() {
                if part.len() == 0 {
                    abort!(on.span(), "Empty topic layers are not allowed");
                }
                if part.starts_with('<') && part.ends_with('>') {
                    let value = &part[1..part.len() - 1];
                    me.parts.push(TopicPart::Ident(value.to_string()))
                } else {
                    me.parts.push(TopicPart::Literal(part.to_string()))
                }
            }
        }

        me
    }

    fn contains(&self, ident: &String) -> bool {
        for part in &self.parts {
            if let TopicPart::Ident(part) = part {
                if part == ident {
                    return true;
                }
            }
        }
        false
    }

    fn iter(&self) -> impl Iterator<Item = &TopicPart> {
        self.parts.iter()
    }

    fn filter_string(&self) -> String {
        let mut filter = String::new();
        for part in self.iter() {
            match part {
                TopicPart::Ident(_) => filter.push_str("+/"),
                TopicPart::Literal(lit) => {
                    filter.push_str(&lit);
                    filter.push_str("/")
                }
            }
        }

        filter.pop();
        filter
    }
}

/// A derive macro for producing MQTT topic + payloads, which implements the `MqttItem` trait
/// for the derived item.
///
/// It supports deriving on enums that only have variants with a single, unnamed, field. Additionally,
/// one of the following must be true for the variant:
/// * The field must implement [`serde::Serialize`] and [`serde::Deserialize`]
/// * The variant is marked with `#[mqtt_item(extend_into)]` and the field implements `MqttItem`
///
/// The `#[mqtt_item]` attribute is used for modifying the derive macro
/// Currently supported struct/enum attributes:
/// * `deserialize_error_type = "Type"`
/// * `serialize_error_type = "Type"`
///
/// Currently supported enum variant attributes:
/// * `#[mqtt_item(extend_into)]`. This attribute causes the MQTT item's topic to be extended with that of the field. Generation
///    of the payload of this `MqttItem` is delegated to the first field of this variant.
///
/// Currently supported struct field attributes:
/// * `#[mqtt_item(layer])`. Requires all non-layer items to be part of the item's topic.
///    This attribute causes the first layer of the field's topic to be placed at the identifier `<field_name>` in the topic
///    of this `MqttItem`. Further generation of the topic and payload of this `MqttItem` is delegated to this field.
///
///    Construction of this item's payload is delegated to the field marked with this attribute.
/// * `#[mqtt_item(extend_into)]`. This behaves exactly as if the field is marked with `#[mqtt_item(layer)]`
///    and `<field_name>` is appended to the end of the topic of this `MqttItem`
#[proc_macro_derive(MqttItem, attributes(mqtt_item))]
#[proc_macro_error]
pub fn mqtt_layer_ident(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item: syn::Item = syn::parse(item).expect("Failed to parse input!");

    let attr_list = match &item {
        Item::Enum(en) => Some(&en.attrs),
        Item::Struct(struct_def) => Some(&struct_def.attrs),
        _ => None,
    };

    let attr_list = attr_list
        .map(|list| list.iter().find_map(|attr| get_attribute_list(attr)))
        .flatten();

    let crate_name = match crate_name("mqtt_macro") {
        Ok(FoundCrate::Itself) => "crate".to_string(),
        Ok(FoundCrate::Name(name)) => format!("::{}", name),
        Err(_) => abort!(item, "Could not find crate name for mqtt-macro"),
    };

    let deserialize_error_type = get_kv_path_or_default(
        attr_list.as_ref(),
        "deserialize_error_type",
        &format!("{}::MqttDeserializeError", crate_name),
    );

    let serialize_error_type = get_kv_path_or_default(
        attr_list.as_ref(),
        "serialize_error_type",
        "::serde_json::Error",
    );

    let crate_name = crate_name.parse().unwrap();

    match &item {
        Item::Enum(en) => impl_for_enum(&crate_name, en).into(),
        _ => abort!(item, "Only enums are supported."),
    }
}

pub(crate) fn get_kv_path_or_default<'a>(
    punctuated: Option<&MetaList>,
    key: &str,
    default: &str,
) -> TokenStream {
    let lit = punctuated.map(|list| get_kv(&list.nested, key)).flatten();

    let ident = lit.map(|lit| match lit {
        Lit::Str(lit_str) => lit_str.value(),
        _ => abort!(punctuated, "No support for literals other than Str yet."),
    });

    let path = ident.unwrap_or(default.to_string());
    path.parse().unwrap()
}

/// Get a key-value from the nested list
pub(crate) fn get_kv<'a>(
    punctuated: &'a Punctuated<NestedMeta, Comma>,
    key: &str,
) -> Option<&'a Lit> {
    for value in punctuated.iter() {
        match value {
            NestedMeta::Meta(meta) => match meta {
                Meta::NameValue(value) => {
                    if let Some(ident) = value.path.get_ident() {
                        if ident == key {
                            return Some(&value.lit);
                        }
                    }
                }
                _ => {}
            },
            NestedMeta::Lit(_) => {}
        }
    }
    None
}

/// Get the list contained in the `mqtt_item` attribute
pub(crate) fn get_attribute_list(attribute: &Attribute) -> Option<MetaList> {
    match attribute.parse_meta().unwrap() {
        Meta::List(list) => {
            if let Some(ident) = list.path.get_ident() {
                if ident == "mqtt_item" {
                    return Some(list);
                }
            }
        }
        _ => {}
    }
    None
}

/// This checks if the `mqtt_item` attribute contains `flag`
pub(crate) fn attribute_flag_set(attribute: &Attribute, flag: &str) -> bool {
    get_attribute_list(attribute).map(|list| {
        list.nested
            .iter()
            .find(|nested| match nested {
                NestedMeta::Meta(meta) => match meta {
                    Meta::Path(path) => path.segments.first().unwrap().ident == flag,
                    _ => false,
                },
                NestedMeta::Lit(_) => false,
            })
            .is_some()
    }) == Some(true)
}

pub(crate) fn get_mqtt_attribute(attributes: &Vec<Attribute>) -> Option<Attribute> {
    attributes.clone().into_iter().find(|attr| {
        attr.path.segments.first().map(|seg| seg.ident.to_string()) == Some("mqtt_item".to_string())
    })
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn topic_can_match_detection() {
        let t1 = Topic::from_string("<hello>/world", &"");
        let t2 = Topic::from_string("hello/<world>", &"");
        assert!(t1 == t2);

        let t1 = Topic::from_string("hello/world", &"");
        let t2 = Topic::from_string("hello/<world>", &"");
        assert!(t1 == t2);

        let t1 = Topic::from_string("hello/world2", &"");
        let t2 = Topic::from_string("hello/world", &"");
        assert!(t1 != t2);

        let t1 = Topic::from_string("hello/world2", &"");
        let t2 = Topic::from_string("<hello>/world", &"");
        assert!(t1 != t2);

        let t1 = Topic::from_string("hello/world/again", &"");
        let t2 = Topic::from_string("hello/<world>", &"");
        assert!(t1 != t2);

        let t1 = Topic::from_string("hello/world/again", &"");
        let t2 = Topic::from_string("hello/<world>/again", &"");
        assert!(t1 == t2);
    }
}
