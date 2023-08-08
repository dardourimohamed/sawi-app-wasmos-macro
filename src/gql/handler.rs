use proc_macro::TokenStream;

use quote::{__private::Span, quote};
use serde::{Deserialize, Serialize};
use syn::{parse_macro_input, Ident, ItemFn};

#[derive(Serialize, Deserialize)]
struct HandlerMetadata {
    pub input: String,
    pub output: String,
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
