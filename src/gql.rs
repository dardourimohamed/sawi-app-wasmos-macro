use proc_macro::TokenStream;

use quote::{__private::Span, quote, ToTokens};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use syn::{parse_macro_input, Ident, ItemFn, ItemStruct, PathArguments, TypePath};

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
        impl wasmos::gql::ObjectMeta for #name {
            fn metastruct() -> String {
                #(#rendered_ext)*
                let mut metadata = #metastruct.to_string();
                #(#format_params)*;
                metadata
            }
        }
    })
}

pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input: ItemFn = parse_macro_input!(item);
    let name = input.sig.ident;
    let handler_name = Ident::new(
        format!("wasmos_handler_{}", name).as_str(),
        Span::call_site(),
    );
    let metadata_name = Ident::new(
        format!("wasmos_handler_metadata_{}", name).as_str(),
        Span::call_site(),
    );
    let body = input.block.stmts;
    let res_type = match &input.sig.output {
        syn::ReturnType::Default => quote! {()},
        syn::ReturnType::Type(_, req_ty) => quote! {#req_ty},
    };
    let res_metadata = match input.sig.output {
        syn::ReturnType::Default => quote! {()},
        syn::ReturnType::Type(_, res_ty) => {
            quote! {wasmos::serde_json::from_str::<wasmos::serde_json::Value>(&#res_ty::metastruct()).unwrap()}
        }
    };

    let mut req = None;
    let mut req_metadata = quote! {()};
    for i in input.sig.inputs.iter() {
        match i {
            syn::FnArg::Receiver(_) => panic!("handler does not support 'this' argument"),
            syn::FnArg::Typed(arg) => match *arg.ty.to_owned() {
                syn::Type::Verbatim(type_name) => match type_name.to_string().as_str() {
                    "Request" => req = Some(arg),
                    _ => panic!("unknown parameter type {}", type_name),
                },
                syn::Type::Path(p) => {
                    let last_seg = p.path.segments.last().unwrap();
                    if last_seg.ident.to_string().as_str() == "Request" {
                        req_metadata = match &last_seg.arguments {
                            syn::PathArguments::AngleBracketed(arg) => {
                                match arg.args.first().unwrap() {
                                    syn::GenericArgument::Type(ty) => {
                                        quote! {wasmos::serde_json::from_str::<wasmos::serde_json::Value>(&#ty::metastruct()).unwrap()}
                                    }
                                    _ => panic!("The body Type should be a type"),
                                }
                            }
                            _ => panic!("The request should get the body Type"),
                        };
                        req = Some(arg)
                    } else {
                        panic!(
                            "unknown parameter type {}",
                            p.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<String>>()
                                .join("::")
                        );
                    }
                }
                _ => panic!("unknown parameter type"),
            },
        }
    }

    let req_parser = match req {
        Some(r) => {
            let pat = r.pat.clone();
            let ty = r.ty.clone();
            quote! {
                let req_str = unsafe { std::ffi::CString::from_raw(ptr as _).into_string().unwrap() };
                let #pat = wasmos::serde_json::from_str::<#ty>(req_str.as_str()).unwrap();
            }
        }
        None => quote!(),
    };

    TokenStream::from(quote! {
        #[no_mangle]
        extern "C" fn #handler_name(ptr: *const u8) -> *const u8 {
            wasmos::tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    #req_parser
                    let res: #res_type = {
                        #(#body)*
                    };
                    let mut res_str = wasmos::serde_json::to_string(&res).unwrap();
                    res_str.push('\0');
                    res_str.as_ptr()
                })
        }
        #[no_mangle]
        extern "C" fn #metadata_name() -> *const u8 {
            let input = #req_metadata;
            let output = #res_metadata;
            let mut res_str = wasmos::serde_json::to_string(&wasmos::serde_json::json!({
                "input": input,
                "output": output
            })).unwrap();
            res_str.push('\0');
            res_str.as_ptr()
        }
    })
}
