//! Implementation of enum derive macro for MqttItem

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use proc_macro_error::abort;
use quote::quote;
use syn::{spanned::Spanned, Ident, Index, ItemEnum, Member, Type, Variant};

use crate::{get_attribute_list, get_kv, get_mqtt_attribute, Topic, TopicPart};

#[derive(PartialEq, Clone, Debug)]
struct IdentifiedField {
    ty: Type,
    ident: Ident,
    name: String,
}

#[derive(Clone, Copy, PartialEq)]
enum FieldType {
    Unnamed,
    Named,
}

fn find_field<'b, T: Spanned>(
    field_type: FieldType,
    on: &T,
    fields: &'b [IdentifiedField],
    path: &String,
) -> &'b IdentifiedField {
    if field_type == FieldType::Unnamed {
        let idx = if let Ok(val) = path.parse::<u32>() {
            val
        } else {
            abort!(
                on.span(),
                "Invalid identifier {}. Must be decimal field name for tuple variants",
                path
            )
        };

        if let Some(field) = fields.iter().find(|f| f.name == format!("{}", idx)) {
            field
        } else {
            abort!(on.span(), "Unknown field {}", path);
        }
    } else {
        if let Some(field) = fields.iter().find(|f| &f.name == path) {
            field
        } else {
            abort!(on.span(), "Unknown field {}", path);
        }
    }
}

pub fn impl_for_enum(crate_name: &TokenStream, en: &ItemEnum) -> TokenStream {
    let en_ident = &en.ident;
    let (imp, ty, wh) = en.generics.split_for_impl();

    let mut functions = Vec::new();
    let mut generator = Vec::new();
    let mut topics: Vec<(String, Topic, bool)> = Vec::new();

    for variant in &en.variants {
        let (push, (parse_fn_name, parse_fn), fields, topic_len, topic, has_payload) =
            generate_variant_impl(crate_name, &en_ident, variant);

        if let Some(data) = topics.iter().find(|(v, t, p)| *t == topic) {
            abort!(
                variant.span(),
                "Variants `{}` and `{}` have the same topic filter!",
                data.0,
                variant.ident.to_string()
            );
        }

        topics.push((variant.ident.to_string(), topic, has_payload));

        let variant = variant.ident.clone();

        functions.push(quote! {
            #parse_fn
        });

        generator.push((
            quote! {
                if let Ok(field) = Self::#parse_fn_name(topic.layers(), payload) {
                    return Ok(field);
                }
            },
            quote! {
                #push
            },
            topic_len,
        ));
    }

    // Make sure that we always attempt to parse the longest topic first
    generator.sort_by(|a, b| b.2.cmp(&a.2));

    let variant_parse: Vec<&TokenStream> = generator
        .iter()
        .map(|(parse, _push, _topic_len)| parse)
        .collect();

    let variant_push: Vec<&TokenStream> = generator
        .iter()
        .map(|(_parse, push, _topic_len)| push)
        .collect();

    let generic_topicstr: Vec<String> = topics.iter().map(|t| t.1.filter_string()).collect();

    quote! {

        impl #imp #en_ident #ty #wh {
            #(#functions)*
        }

        impl #imp #crate_name::MqttItem for #en_ident #ty #wh {
            type DeserializeError = #crate_name::MqttDeserializeError;
            type SerializeError = ::serde_json::Error;

            fn push_topic_and_payload(self, topic: &mut #crate_name::Topic, payload: &mut Vec<u8>) -> Result<(), Self::SerializeError> {
                match self {
                    #(#variant_push)*
                }
            }

            fn from_topic_and_payload<'a>(
                topic: #crate_name::Topic,
                payload: &'a [u8],
            ) -> Result<Self, Self::DeserializeError> {
                #(#variant_parse)*
                Err(Self::DeserializeError::Invalid)
            }

            fn all_generic_topics() -> &'static [&'static str] {
                &[#(#generic_topicstr ,)*]
            }
        }

    }
}

fn generate_variant_impl(
    crate_name: &TokenStream,
    enum_name: &Ident,
    variant: &Variant,
) -> (
    TokenStream,
    (Ident, TokenStream),
    Vec<IdentifiedField>,
    usize,
    Topic,
    bool,
) {
    let meta_list = if let Some(attr) = get_mqtt_attribute(&variant.attrs) {
        if let Some(list) = get_attribute_list(&attr) {
            list.nested
        } else {
            abort!(variant, "The `mqtt_item` does not have a meta list");
        }
    } else {
        abort!(
            variant,
            "This variant does not have an `mqtt_item` attribute"
        );
    };

    let (mut fields, field_type): (Vec<IdentifiedField>, _) = match &variant.fields {
        syn::Fields::Unnamed(fields) => (
            fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(idx, f)| IdentifiedField {
                    ty: f.ty.clone(),
                    ident: Ident::new(&format!("_mqttitem_field_field_{}", idx), variant.span()),
                    name: format!("{}", idx),
                })
                .collect(),
            FieldType::Unnamed,
        ),
        syn::Fields::Named(fields) => (
            fields
                .named
                .iter()
                .map(|f| IdentifiedField {
                    ty: f.ty.clone(),
                    ident: Ident::new(
                        &format!("_mqttitem_field_{}", f.ident.clone().unwrap()),
                        variant.span(),
                    ),
                    name: f.ident.clone().unwrap().to_string(),
                })
                .collect(),
            FieldType::Named,
        ),
        _ => abort!(
            variant,
            "Unit variants are not supported. Try `{}()` instead",
            variant.ident.to_string()
        ),
    };

    let mut not_processed_fields = fields.clone();
    let mut process_field = |name: &String| {
        let index = if let Some(idx) = not_processed_fields.iter().position(|f| &f.name == name) {
            idx
        } else {
            abort!(
                variant.span(),
                format!("Field `{}` is specified more than once", name)
            );
        };
        not_processed_fields.remove(index);
    };

    let topic = if let Some(topic) = get_kv(&meta_list, "topic") {
        let topic_lit = match topic {
            syn::Lit::Str(str) => str.value(),
            _ => abort!(variant, "The topic must be a literal string."),
        };
        Topic::from_string(&topic_lit, variant)
    } else {
        abort!(variant, "Topic not defined");
    };

    let topic_len = topic.iter().count();

    let payload = if let Some(payload) = get_kv(&meta_list, "payload") {
        match payload {
            syn::Lit::Str(str) => {
                let value = str.value();
                if value.starts_with('<') && value.ends_with('>') {
                    Some(value[1..value.len() - 1].to_string())
                } else {
                    abort!(variant, "Payload field must be written as <field_name>");
                }
            }
            _ => abort!(variant, "Payload field not found"),
        }
    } else {
        None
    };
    // Override_serialize
    let override_serialization = if let Some(override_ser) = get_kv(&meta_list, "serialize_using") {
        match override_ser {
            syn::Lit::Str(str) => {
                let value = str.value();
                Some(value)
            }
            _ => abort!(variant, "Serialization function not found in derive"),
        }
    } else {
        None
    };

    // Override_deserialize
    let override_deserialization =
        if let Some(override_deser) = get_kv(&meta_list, "deserialize_using") {
            match override_deser {
                syn::Lit::Str(str) => {
                    let value = str.value();
                    Some(value)
                }
                _ => abort!(variant, "Deserialization function not found in derive"),
            }
        } else {
            None
        };

    let (payload_serialize, payload_deserialize) = if let Some(payload) = &payload {
        let IdentifiedField { ty, ident, name } =
            find_field(field_type, &variant, &fields, &payload);
        process_field(name);

        let ser = if let Some(override_fun) = override_serialization {
            let override_fun: TokenStream = override_fun.parse().unwrap();
            quote! {
                #override_fun(payload, &#ident)?;
            }
        } else {
            quote! {
                #crate_name::serde_json_serialize(payload, &#ident)?;
            }
        };
        let deser = if let Some(override_fun) = override_deserialization {
            let override_fun: TokenStream = override_fun.parse().unwrap();
            quote! {
                let #ident = #override_fun(payload)?;
            }
        } else {
            quote! {
                let #ident: #ty = #crate_name::serde_json_deserialize(payload)?;
            }
        };

        (Some(ser), Some(deser))
    } else {
        (None, None)
    };

    let mut topic_push = Vec::new();
    let mut topic_parse = Vec::new();

    for topic_layer in topic.parts.iter() {
        let push = match topic_layer {
            TopicPart::Ident(ident) => {
                let IdentifiedField { ty, ident, name } =
                    find_field(field_type, &variant, &fields, ident);
                process_field(name);

                quote! {
                    topic.push(&#ident.to_string());
                }
            }
            TopicPart::Literal(literal) => {
                quote! {
                    topic.push(#literal.into());
                }
            }
        };

        topic_push.push(push);

        let parse = match topic_layer {
            TopicPart::Ident(ident) => {
                let IdentifiedField { ty, ident, name } =
                    find_field(field_type, &variant, &fields, ident);
                quote! {
                   let #ident = if let Some(value) = topic.next() {
                        value
                            .parse()
                            .map_err(|_| {
                                <Self as #crate_name::MqttItem>::DeserializeError::InvalidTopicLayer(#name.into(), value.into())
                            })?
                    } else {
                        return Err(<Self as #crate_name::MqttItem>::DeserializeError::MissingTopicLayer(#name.into()));
                    };
                }
            }
            TopicPart::Literal(literal) => {
                quote! {
                    if let Some(value) = topic.next() {
                        if value != #literal.to_string() {
                            return Err(<Self as #crate_name::MqttItem>::DeserializeError::MissingTopicLayer(#literal.into()));
                        }
                    } else {
                        return Err(<Self as #crate_name::MqttItem>::DeserializeError::MissingTopicLayer(#literal.into()));
                    }
                }
            }
        };

        topic_parse.push(parse);
    }

    if !not_processed_fields.is_empty() {
        let mut msg = format!("The following fields are not part of the topic or payload: ");
        for field in not_processed_fields.iter() {
            msg = format!("{}{}, ", msg, field.name)
        }
        abort!(variant, msg);
    }

    let variant_name = variant.ident.clone();

    let field_names: Vec<TokenStream> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.clone();
            if field_type == FieldType::Unnamed {
                quote! {
                    #ident
                }
            } else {
                let name = Ident::new(&f.name, variant.span());
                quote! {
                    #name: #ident
                }
            }
        })
        .collect();

    let fields_stmt = if FieldType::Named == field_type {
        quote! { #enum_name::#variant_name { #(#field_names,)* } }
    } else {
        quote! { #enum_name::#variant_name ( #(#field_names,)* ) }
    };

    let push = quote! {
        #fields_stmt => {
            #(#topic_push)*
            #payload_serialize
            Ok(())
        }
    };

    let parse_fn_name = Ident::new(
        ("__mqttitem__parse_".to_string() + variant.ident.to_string().as_str()).as_str(),
        Span::call_site(),
    );

    let parse_fn = quote! {
        fn #parse_fn_name<'__topic>(mut topic: impl Iterator<Item = &'__topic str>, payload: &[u8]) -> Result<Self, <Self as #crate_name::MqttItem>::DeserializeError> {
            #(#topic_parse)*
            #payload_deserialize
            Ok(#fields_stmt)
        }
    };

    (
        push,
        (parse_fn_name, parse_fn),
        fields,
        topic_len,
        topic,
        payload.is_some(),
    )
}
