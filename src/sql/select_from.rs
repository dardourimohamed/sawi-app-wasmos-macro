use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident, Path, Type};

pub fn select_from(attr: TokenStream, item: TokenStream) -> TokenStream {
    let table_p = parse_macro_input!(attr as Path);
    let input = parse_macro_input!(item as DeriveInput);
    let id = input.ident.clone();
    let impl_id = Ident::new(&format!("{}Query", &id), Span::call_site());

    let (idents, types): (Vec<Ident>, Vec<Type>) = match input.clone().data {
        syn::Data::Struct(s) => s,
        _ => panic!("struct must have named fields"),
    }
    .fields
    .into_iter()
    .filter_map(|field| field.ident.map(|f| (f, field.ty)))
    .unzip();

    let f_names = idents.iter().map(|f| f.to_string());

    TokenStream::from(quote!(
        #[derive(riwaq::serde::Deserialize)]
        #input
        impl #table_p::SelectTypeValidator for #id {
            #(fn #idents(_: #types) {})*
        }

        #[derive(riwaq::serde::Serialize)]
        pub struct #impl_id(riwaq::sql::Select<#table_p::SQLFilter>);
        impl std::fmt::Debug for #impl_id {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&format!("{}", self.0))
            }
        }
        impl #impl_id {
            pub fn and(self, filter: #table_p::SQLFilter) -> Self {
                Self (
                    riwaq::sql::Select {
                        filter: Some(match self.0.filter {
                            Some(ex_filter) => ex_filter.and(filter),
                            _ => riwaq::sql::FilterStmt::Filter(filter),
                        }),
                        ..self.0
                    }
                )
            }
            pub fn and_all<VEC>(self, filter: VEC) -> Self
            where
                VEC: IntoIterator<Item = #table_p::SQLFilter>,
            {
                Self (
                    riwaq::sql::Select {
                        filter: Some(match self.0.filter {
                            Some(ex_filter) => ex_filter.and_all::<VEC>(filter),
                            _ => riwaq::sql::FilterStmt::And(
                                filter
                                    .into_iter()
                                    .map(|item| riwaq::sql::FilterStmt::Filter(item))
                                    .collect(),
                            ),
                        }),
                        ..self.0
                    }
                )
            }
            pub fn where_(self, filter: #table_p::SQLFilter) -> Self {
                self.and(filter)
            }
            pub fn or(self, filter: #table_p::SQLFilter) -> Self {
                Self (
                    riwaq::sql::Select {
                        filter: Some(match self.0.filter {
                            Some(ex_filter) => ex_filter.or(filter),
                            _ => riwaq::sql::FilterStmt::Filter(filter),
                        }),
                        ..self.0
                    }
                )
            }
            pub fn or_any<VEC>(self, filter: VEC) -> Self
            where
                VEC: IntoIterator<Item = #table_p::SQLFilter>,
            {
                Self (
                    riwaq::sql::Select {
                        filter: Some(match self.0.filter {
                            Some(ex_filter) => ex_filter.or_any::<VEC>(filter),
                            _ => riwaq::sql::FilterStmt::Or(
                                filter
                                    .into_iter()
                                    .map(|item| riwaq::sql::FilterStmt::Filter(item))
                                    .collect(),
                            ),
                        }),
                        ..self.0
                    }
                )
            }

            pub async fn exec(&self) -> Result<Vec<#id>, String> {
                riwaq::sql::sql_query(
                    riwaq::serde_json::to_value(&self).unwrap()
                ).await
                .map(|res| riwaq::serde_json::from_value::<Vec<#id>>(res).unwrap())
            }
        }

        impl #id {
            pub fn find() -> #impl_id {
                #impl_id(riwaq::sql::Select {
                    op: Some("Select".to_string()),
                    tbl: #table_p::T_NAME.to_string(),
                    cols: vec![#(#f_names.to_string() ,)*],
                    filter: None
                })
            }
        }

    ))
}
