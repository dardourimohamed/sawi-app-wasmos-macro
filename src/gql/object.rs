use proc_macro::TokenStream;

use quote::{__private::Span, quote, ToTokens};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use syn::{parse_macro_input, Ident, ItemStruct, PathArguments, TypePath};

#[derive(Serialize, Deserialize)]
struct HandlerMetadata {
    pub input: String,
    pub output: String,
}

fn parse_type(i: String, p: &TypePath) -> (Value, Vec<(String, impl ToTokens)>) {
    let mut ext = vec![];
    let last_p = p.path.segments.last().unwrap().ident.to_string();
    let md_id = format!("metadata_{}", i);
    match last_p.as_str() {
        "bool" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
        | "u128" | "usize" | "f32" | "f64" | "char" | "String" => {
            (serde_json::Value::String(last_p.to_string()), ext)
        }
        "Vec" | "Option" => {
            let arg = match &p.path.segments.last().unwrap().arguments {
                PathArguments::AngleBracketed(args) => args.args.first().unwrap(),
                _ => panic!(),
            };

            let a = match arg {
                syn::GenericArgument::Type(t) => match t {
                    syn::Type::Path(p) => p,
                    _ => panic!(),
                },
                _ => panic!(),
            };

            let res = parse_type(format!("{}_0", i), a);
            ext.extend(res.1);
            (
                json!({
                    "container": last_p,
                    "content": res.0
                }),
                ext,
            )
        }
        _ => (
            json!({
                "container": "Obj",
                "content": format!("{{{}}}", md_id)}
            ),
            vec![(md_id, quote!(#p::metastruct()))],
        ),
    }
}

pub fn object(item: TokenStream) -> TokenStream {
    let input: ItemStruct = parse_macro_input!(item);
    let name = input.ident;

    let mut ext: Vec<(String, _)> = vec![];
    let mut ms = serde_json::Map::default();
    ms.insert(
        "_name_".to_owned(),
        serde_json::Value::String(name.to_string()),
    );

    for (i, field) in input.fields.iter().enumerate() {
        let k = field
            .ident
            .to_owned()
            .map(|f| f.to_string())
            .unwrap_or(i.to_string());
        ms.insert(
            k.to_owned(),
            match &field.ty {
                syn::Type::Path(p) => {
                    let res = parse_type(i.to_string(), p);
                    ext.extend(res.1);
                    res.0
                }
                _ => panic!("unknown type for field {}", k),
            },
        );
    }

    let rendered_ext = ext
        .iter()
        .map(|e| {
            let id = syn::Ident::new(&e.0, Span::call_site());
            let expr = &e.1;
            quote!(let #id = #expr;)
        })
        .collect::<Vec<_>>();
    let format_params = ext
        .iter()
        .map(|e| {
            let id_s = format!("\"{{{}}}\"", e.0);
            let id = Ident::new(e.0.as_str(), Span::call_site());
            quote!(metadata = metadata.replace(#id_s, #id.as_str()))
        })
        .collect::<Vec<_>>();

    let metastruct = serde_json::to_string(&serde_json::Value::Object(ms)).unwrap();
    TokenStream::from(quote! {
        impl riwaq::gql::ObjectMeta for #name {
            fn metastruct() -> String {
                #(#rendered_ext)*
                let mut metadata = #metastruct.to_string();
                #(#format_params)*;
                metadata
            }
        }
    })
}
