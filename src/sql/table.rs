use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, DeriveInput, Expr, ExprAssign, Field, Lit,
    Type, TypePath, Visibility,
};
use wasmos_types::sql::{DDLOp, FieldDDL, TableDDL, TableDDLOp};

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
        op: DDLOp::Keep,
    }
}

pub fn table(attr: TokenStream, item: TokenStream) -> TokenStream {
    let tbl_drop = syn::parse::<Ident>(attr.to_owned())
        .and_then(|t| {
            Ok(
                if ["dropped_table_and_data", "drop_table_and_data"]
                    .contains(&t.to_string().to_lowercase().as_str())
                {
                    Some(TableDDLOp::DropAll)
                } else if ["dropped", "drop"].contains(&t.to_string().to_lowercase().as_str()) {
                    Some(TableDDLOp::Drop)
                } else {
                    None
                },
            )
        })
        .unwrap_or(None);
    let tbl_undrop = syn::parse::<Ident>(attr.to_owned())
        .and_then(|t| {
            Ok(
                if ["undropped", "undrop"].contains(&t.to_string().to_lowercase().as_str()) {
                    Some(TableDDLOp::Undrop)
                } else {
                    None
                },
            )
        })
        .unwrap_or(None);
    let tbl_rename_from = syn::parse::<ExprAssign>(attr)
        .and_then(|t| {
            let l = match *t.left {
                Expr::Path(p) => p.path.get_ident().unwrap().to_string(),
                _ => "".to_owned(),
            };
            let r = match *t.right {
                Expr::Lit(l) => match l.lit {
                    Lit::Str(s) => s.value(),
                    _ => panic!("table rename source should be str literal"),
                },
                _ => panic!("table rename source should be str literal"),
            };
            if ["renamed_from", "rename_from"].contains(&l.to_lowercase().as_str()) {
                Ok(Some(r))
            } else {
                panic!("table attribute should be one of: 'renamed_from', 'rename_from'")
            }
        })
        .unwrap_or(None);

    let input = parse_macro_input!(item as DeriveInput);
    let struct_name = input.ident;
    let vis = if let None = tbl_drop {
        input.vis
    } else {
        Visibility::Inherited
    };
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

    let field_names = fields
        .iter()
        .filter_map(|field| field.0.ident.as_ref())
        .collect::<Vec<_>>();
    let field_names_str = fields
        .iter()
        .filter_map(|field| field.0.ident.as_ref().map(|f| f.to_string()))
        .collect::<Vec<_>>();
    let cols = fields
        .iter()
        .filter_map(|field| {
            field
                .0
                .ident
                .as_ref()
                .map(|f| Ident::new(&f.to_string().to_case(Case::Pascal), Span::call_site()))
        })
        .collect::<Vec<_>>();
    let field_types = fields.iter().map(|field| &field.0.ty).collect::<Vec<_>>();

    let ddl = format!(
        "{}\0",
        serde_json::to_string(&TableDDL {
            name: t_name.to_owned(),
            cols: fields
                .iter()
                .map(|f| f.1.to_owned())
                .collect::<Vec<FieldDDL>>(),
            op: if let Some(op) = &tbl_drop {
                op.to_owned()
            } else if let Some(op) = &tbl_undrop {
                op.to_owned()
            } else if let Some(rename_src) = tbl_rename_from {
                TableDDLOp::Rename(rename_src)
            } else {
                TableDDLOp::Keep
            }
        })
        .unwrap()
    );

    let ddl_name = Ident::new(
        format!("wasmos_table_ddl_{}", t_name).as_str(),
        Span::call_site(),
    );

    let output = if let Some(_) = tbl_drop {
        quote! {
            mod #struct_name {
                #[no_mangle]
                extern "C" fn #ddl_name() -> *const u8 {
                    #ddl.as_ptr()
                }
            }
        }
    } else {
        quote! {
            #[allow(non_snake_case)]
            #vis mod #struct_name {
                pub const T_NAME: &'static str = #t_name;

                pub trait SelectTypeValidator {
                    #(fn #field_names(_: #field_types) {} )*
                }

                #[wasmos::select_from(super::#struct_name)]
                pub struct SelectAll {
                    #(pub #field_names: #field_types,)*
                }

                pub mod Filter {

                    #[derive(wasmos::serde::Serialize)]
                    pub struct SQLFilter(wasmos::sql::FilterItem);
                    impl wasmos::sql::SQLFilterTrait for SQLFilter {
                        fn get_filter(&self) -> wasmos::sql::FilterItem {
                            self.0.clone()
                        }
                    }

                    #(
                        pub mod #cols {
                            pub fn eq(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Eq{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn ne(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Ne{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn in_<VEC>(values: VEC) -> super::SQLFilter where VEC: IntoIterator<Item = #field_types> {
                                super::SQLFilter(wasmos::sql::FilterItem::In{
                                    col: #field_names_str.to_string(),
                                    values: values.into_iter().map(|v| wasmos::serde_json::to_value(v).unwrap()).collect::<Vec<wasmos::serde_json::Value>>()
                                })
                            }
                            pub fn nin<VEC>(values: VEC) -> super::SQLFilter where VEC: IntoIterator<Item = #field_types> {
                                super::SQLFilter(wasmos::sql::FilterItem::Nin{
                                    col: #field_names_str.to_string(),
                                    values: values.into_iter().map(|v| wasmos::serde_json::to_value(v).unwrap()).collect::<Vec<wasmos::serde_json::Value>>()
                                })
                            }
                            pub fn gt(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Gt{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn gte(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Gte{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn lt(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Lt{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn lte(value: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Lte{
                                    col: #field_names_str.to_string(),
                                    value: wasmos::serde_json::to_value(value).unwrap()
                                })
                            }
                            pub fn between(start: #field_types, end: #field_types) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Between{
                                    col: #field_names_str.to_string(),
                                    start: wasmos::serde_json::to_value(start).unwrap(),
                                    end: wasmos::serde_json::to_value(end).unwrap()
                                })
                            }
                            pub fn like(expr: String) -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::Like{
                                    col: #field_names_str.to_string(),
                                    expr: expr
                                })
                            }
                            pub fn is_null() -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::IsNull{
                                    col: #field_names_str.to_string()
                                })
                            }
                            pub fn is_not_null() -> super::SQLFilter {
                                super::SQLFilter(wasmos::sql::FilterItem::IsNotNull{
                                    col: #field_names_str.to_string()
                                })
                            }
                        }
                    )*
                }

                #[derive(wasmos::serde::Serialize)]
                pub struct Insert {
                    #(pub #field_names: #field_types,)*
                }
                impl Insert {
                    pub async fn exec(&self) {
                        let s = wasmos::serde_json::json!({
                            "op": "insert".to_string(),
                            "tbl": #t_name.to_string(),
                            "row": self
                        });
                        let _ = wasmos::sql::sql_exec(s).await;
                    }
                }

                #[no_mangle]
                extern "C" fn #ddl_name() -> *const u8 {
                    #ddl.as_ptr()
                }
            }

        }
    };

    output.into()
}
