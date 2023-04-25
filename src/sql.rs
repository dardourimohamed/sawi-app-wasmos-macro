use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use serde::Serialize;
use syn::{parse_macro_input, AngleBracketedGenericArguments, DeriveInput, Field, Type, TypePath};

#[derive(Serialize, Clone)]
pub struct FieldDDL {
    pub name: String,
    pub opt: bool,
    pub ty: String,
}

#[derive(Serialize)]
pub struct TableDDL {
    pub name: String,
    pub cols: Vec<FieldDDL>,
}

// fn field_attributes(field: &Field) -> Vec<String> {
//     field
//         .attrs
//         .iter()
//         .filter_map(|attr| attr.path().get_ident().map(|i| i.to_string()))
//         .collect::<Vec<String>>()
// }

// fn field_to_sql_create(f: &&Field) -> String {
//     let is_null;
//     let ty = match &f.ty {
//         syn::Type::Path(p) => {
//             let last_p = &p.path.segments.last().unwrap();
//             match if last_p.ident.to_string() == "Option" {
//                 is_null = "";
//                 match &last_p.arguments {
//                     syn::PathArguments::AngleBracketed(AngleBracketedGenericArguments {
//                         args,
//                         ..
//                     }) => match args.first().unwrap() {
//                         syn::GenericArgument::Type(Type::Path(TypePath { path, .. })) => {
//                             path.segments.last().unwrap().ident.to_string()
//                         }
//                         _ => todo!(),
//                     },
//                     _ => panic!(""),
//                 }
//             } else {
//                 is_null = " NOT NULL";
//                 last_p.ident.to_string()
//             }
//             .as_str()
//             {
//                 "bool" => "BOOLEAN",
//                 "i8" => "TINYINT",
//                 "i16" => "SMALLINT",
//                 "i32" => "INT",
//                 "i64" => "BIGINT",
//                 "f32" => "FLOAT",
//                 "f64" => "DOUBLE",
//                 "String" => "VARCHAR(65535)",
//                 t => panic!("unsupported type '{}'", t),
//             }
//             .to_string()
//         }
//         _ => panic!("unsupported type"),
//     };
//     format!(
//         "{name} {ty}{is_null}",
//         name = f
//             .ident
//             .as_ref()
//             .expect("field sould have a name")
//             .to_string(),
//         ty = ty,
//         is_null = is_null
//     )
// }

fn field_to_ddl(f: &Field) -> FieldDDL {
    let optional;
    let ty = match &f.ty {
        syn::Type::Path(p) => {
            let last_p = &p.path.segments.last().unwrap();
            match if last_p.ident.to_string() == "Option" {
                optional = true;
                match &last_p.arguments {
                    syn::PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                        args,
                        ..
                    }) => match args.first().unwrap() {
                        syn::GenericArgument::Type(Type::Path(TypePath { path, .. })) => {
                            path.segments.last().unwrap().ident.to_string()
                        }
                        _ => todo!(),
                    },
                    _ => panic!(""),
                }
            } else {
                optional = false;
                last_p.ident.to_string()
            }
            .as_str()
            {
                "bool" => "BOOLEAN",
                "i8" => "TINYINT",
                "i16" => "SMALLINT",
                "i32" => "INT",
                "i64" => "BIGINT",
                "f32" => "FLOAT",
                "f64" => "DOUBLE",
                "String" => "VARCHAR(65535)",
                t => panic!("unsupported type '{}'", t),
            }
            .to_string()
        }
        _ => panic!("unsupported type"),
    };
    FieldDDL {
        name: f
            .ident
            .as_ref()
            .expect("field sould have a name")
            .to_string(),
        opt: optional,
        ty: ty,
    }
}

pub fn table(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let struct_name = input.ident;
    let vis = input.vis;
    let t_name = struct_name.to_string().to_case(Case::Snake);

    let fields = match input.data {
        syn::Data::Struct(s) => s,
        _ => panic!("struct must have named fields"),
    }
    .fields
    .into_iter()
    .map(|f| {
        let ddl = field_to_ddl(&f);
        (f, ddl)
    })
    .collect::<Vec<(Field, FieldDDL)>>();

    // let sql_create = format!("CREATE TABLE IF NOT EXISTS {t_name} (\n    {fields}\n);\n{create_missing_cols}",
    //     t_name = t_name.clone(),
    //     fields = filtered_fields
    //         .iter()
    //         .map(|f| field_to_sql_create(&f.0))
    //         .collect::<Vec<String>>()
    //         .join(",\n    "),
    //     create_missing_cols = filtered_fields
    //     .iter()
    //     .map(|f| format!("ALTER TABLE {t_name} ADD COLUMN {field};",
    //         t_name = t_name,
    //         field = field_to_sql_create(&f.0)
    //     ))
    //     .collect::<Vec<String>>()
    //     .join("\n")
    // );

    // let sql_drop_fields = dropped_fields
    //     .iter()
    //     .map(|f| {
    //         format!(
    //             "ALTER TABLE {t_name} DROP IF EXISTS {col_name};",
    //             t_name = t_name.clone(),
    //             col_name = f
    //                 .ident
    //                 .as_ref()
    //                 .expect("field sould have a name")
    //                 .to_string()
    //         )
    //     })
    //     .collect::<Vec<String>>();

    let field_names = fields
        .iter()
        .map(|field| &field.0.ident)
        .collect::<Vec<_>>();
    let field_types = fields.iter().map(|field| &field.0.ty).collect::<Vec<_>>();

    // let mut sql: Vec<String> = vec![sql_create];
    // sql.extend(sql_drop_fields);

    // let sql = sql.join("\n");

    let ddl = serde_json::to_string(&TableDDL {
        name: t_name.to_owned(),
        cols: fields
            .iter()
            .map(|f| f.1.to_owned())
            .collect::<Vec<FieldDDL>>(),
    })
    .unwrap();

    let ddl_name = Ident::new(
        format!("wasmos_table_ddl_{}", t_name).as_str(),
        Span::call_site(),
    );

    let output = quote! {
        #[allow(non_snake_case)]
        #vis mod #struct_name {
            #[derive(wasmos::serde::Serialize, Debug)]
            pub struct Insert {
                #(pub #field_names: #field_types,)*
            }
            impl Insert {
                pub async fn exec(&self) {
                    let s = wasmos::serde_json::json!({
                        "op": "insert",
                        "tbl": #t_name,
                        "row": self
                    });
                    wasmos::tokio::task::spawn(async move {
                        let req_ptr = format!("{}\0", wasmos::serde_json::to_string(&s).unwrap()).as_ptr();
                        let res_ptr = unsafe { wasmos::sql::sql_dml(req_ptr) };
                        let res_str = unsafe { std::ffi::CString::from_raw(res_ptr as _).into_string().unwrap() };
                        wasmos::wdbg!(res_str);
                    }).await.unwrap()
                }
            }

            #[no_mangle]
            extern "C" fn #ddl_name() -> *const u8 {
                #ddl.as_ptr()
            }

            // impl wasmos::sql::DDL for #struct_name {
            //     fn ddl() -> String {
            //         #ddl.to_string()
            //     }
            // }
        }

    };

    output.into()
}
