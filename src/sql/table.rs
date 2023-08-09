use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, DeriveInput, Expr, ExprAssign, Field, Lit,
    LitStr, Type, TypePath, Visibility,
};
use riwaq_types::sql::{DDLOp, FieldDDL, TableDDL, TableDDLOp};

fn field_to_ddl(f: &Field) -> FieldDDL {
    let rename = f.attrs.iter().find_map(|a| {
        a.path()
            .get_ident()
            .map(|id| {
                if ["from", "rename_from", "renamed_from"]
                    .contains(&id.to_string().to_lowercase().as_str())
                {
                    Some(
                        a.parse_args::<LitStr>()
                            .expect("invalid source column name")
                            .value(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or(None)
    });

    let optional;
    let r_ty = match &f.ty {
        syn::Type::Path(p) => {
            let last_p = &p.path.segments.last().unwrap();
            if last_p.ident.to_string() == "Option" {
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
        }
        _ => panic!("unsupported type"),
    };

    let ty = match r_ty.as_str() {
        "bool" => "BOOLEAN",
        "i8" => "TINYINT",
        "i16" => "SMALLINT",
        "i32" => "INT",
        "i64" => "BIGINT",
        "f32" => "FLOAT",
        "f64" => "DOUBLE",
        "char" => "VARCHAR(1)",
        "str" => "VARCHAR(65535)",
        "String" => "VARCHAR(65535)",
        t => panic!("unsupported type '{}'", t),
    }
    .to_string();

    // let default: Option<Value> = f.attrs.iter().find_map(|a| {
    //     a.path().get_ident().and_then(|id| {
    //         if ["default", "default_value"].contains(&id.to_string().as_str()) {
    //             let r = a
    //                 .parse_args::<LitStr>()
    //                 .map(|s| {
    //                     if !["str", "String"].contains(&r_ty.as_str()) {
    //                         panic!("default value of type string literal should only be used on fields of type: 'str' or 'String'")
    //                     };
    //                     Some(Value::from(s.value()))
    //                 })
    //                 .or_else(|_| {
    //                     a.parse_args::<LitFloat>()
    //                         .map(|i| {
    //                             if !["f32", "f64"].contains(&r_ty.as_str()) {
    //                                 panic!("default value of type float should only be used on fields of type: 'f32' or 'f64'")
    //                             };
    //                             Some(Value::from(from_str::<f64>(i.base10_digits()).unwrap()))
    //                         })
    //                         .or_else(|_| {
    //                             a.parse_args::<LitInt>()
    //                                 .map(|i| {
    //                                     if !["i8", "i16", "i32", "i64"].contains(&r_ty.as_str()) {
    //                                         panic!("default value of type int should only be used on fields of type: 'i8', 'i16', 'i32' or 'i64'")
    //                                     };
    //                                     Some(Value::from(
    //                                         from_str::<i64>(i.base10_digits()).unwrap(),
    //                                     ))
    //                                 })
    //                                 .or_else(|_| {
    //                                     a.parse_args::<LitChar>()
    //                                         .map(|i| {
    //                                             if !["char"].contains(&r_ty.as_str()) {
    //                                                 panic!("default value of type char should only be used on fields of type: 'char'")
    //                                             };
    //                                             Some(Value::from(i.value().to_string()))})
    //                                         .or_else(|_| {
    //                                             a.parse_args::<LitBool>()
    //                                                 .map(|i| {
    //                                                     if !["bool"].contains(&r_ty.as_str()) {
    //                                                         panic!("default value of type bool should only be used on fields of type: 'bool'")
    //                                                     };
    //                                                     Some(Value::from(i.value()))
    //                                                 })
    //                                         })
    //                                 })
    //                         })
    //                 }).map_err(|_| a.parse_args::<Path>().unwrap().get_ident().unwrap().to_string()).expect("invalid default value");
    //             r
    //         } else {
    //             None
    //         }
    //     })
    // });

    FieldDDL {
        name: f
            .ident
            .as_ref()
            .expect("field sould have a name")
            .to_string(),
        opt: optional,
        ty: ty,
        default: None,
        op: rename.map_or(DDLOp::Keep, |n| DDLOp::Rename(n)),
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
        format!("riwaq_table_ddl_{}", t_name).as_str(),
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

                #[riwaq::select_from(super::#struct_name)]
                pub struct SelectAll {
                    #(pub #field_names: #field_types,)*
                }

                #[derive(riwaq::serde::Serialize)]
                pub struct SQLFilter(riwaq::sql::FilterItem);
                impl riwaq::sql::SQLFilterTrait for SQLFilter {
                    fn get_filter(&self) -> riwaq::sql::FilterItem {
                        self.0.clone()
                    }
                }

                #(
                    pub mod #cols {
                        pub fn eq(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Eq{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn ne(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Ne{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn in_<VEC>(values: VEC) -> super::SQLFilter where VEC: IntoIterator<Item = #field_types> {
                            super::SQLFilter(riwaq::sql::FilterItem::In{
                                col: #field_names_str.to_string(),
                                values: values.into_iter().map(|v| riwaq::serde_json::to_value(v).unwrap()).collect::<Vec<riwaq::serde_json::Value>>()
                            })
                        }
                        pub fn nin<VEC>(values: VEC) -> super::SQLFilter where VEC: IntoIterator<Item = #field_types> {
                            super::SQLFilter(riwaq::sql::FilterItem::Nin{
                                col: #field_names_str.to_string(),
                                values: values.into_iter().map(|v| riwaq::serde_json::to_value(v).unwrap()).collect::<Vec<riwaq::serde_json::Value>>()
                            })
                        }
                        pub fn gt(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Gt{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn gte(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Gte{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn lt(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Lt{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn lte(value: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Lte{
                                col: #field_names_str.to_string(),
                                value: riwaq::serde_json::to_value(value).unwrap()
                            })
                        }
                        pub fn between(start: #field_types, end: #field_types) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Between{
                                col: #field_names_str.to_string(),
                                start: riwaq::serde_json::to_value(start).unwrap(),
                                end: riwaq::serde_json::to_value(end).unwrap()
                            })
                        }
                        pub fn like(expr: String) -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::Like{
                                col: #field_names_str.to_string(),
                                expr: expr
                            })
                        }
                        pub fn is_null() -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::IsNull{
                                col: #field_names_str.to_string()
                            })
                        }
                        pub fn is_not_null() -> super::SQLFilter {
                            super::SQLFilter(riwaq::sql::FilterItem::IsNotNull{
                                col: #field_names_str.to_string()
                            })
                        }
                    }
                )*

                #[derive(riwaq::serde::Serialize)]
                pub struct Insert {
                    #(pub #field_names: #field_types,)*
                }
                impl Insert {
                    pub async fn exec(&self) -> Result<i64, String> {
                        let s = riwaq::serde_json::json!(riwaq::sql::Insert {
                            op: Some("Insert".to_string()),
                            tbl: #t_name.to_string(),
                            values: riwaq::serde_json::to_value(self).unwrap()
                        });
                        riwaq::sql::sql_exec(s).await
                    }
                }

                pub mod Update {

                    pub struct Update(riwaq::sql::Update<super::SQLFilter>);
                    impl Update {
                        #(
                            pub fn #field_names(self, value: #field_types) -> Update {
                                let mut values = self.0.values;
                                values.insert(#field_names_str.to_string(), riwaq::serde_json::to_value(value).unwrap());
                                Update(riwaq::sql::Update {
                                    values,
                                    ..self.0
                                })
                            }
                        )*

                        pub fn and(self, filter: super::SQLFilter) -> Self {
                            Self(riwaq::sql::Update {
                                filter: Some(match self.0.filter {
                                    Some(ex_filter) => ex_filter.and(filter),
                                    _ => riwaq::sql::FilterStmt::Filter(filter),
                                }),
                                ..self.0
                            })
                        }
                        pub fn and_all<VEC>(self, filter: VEC) -> Self
                        where
                            VEC: IntoIterator<Item = super::SQLFilter>,
                        {
                            Self(riwaq::sql::Update {
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
                            })
                        }
                        pub fn where_(self, filter: super::SQLFilter) -> Self {
                            self.and(filter)
                        }
                        pub fn or(self, filter: super::SQLFilter) -> Self {
                            Self(riwaq::sql::Update {
                                filter: Some(match self.0.filter {
                                    Some(ex_filter) => ex_filter.or(filter),
                                    _ => riwaq::sql::FilterStmt::Filter(filter),
                                }),
                                ..self.0
                            })
                        }
                        pub fn or_any<VEC>(self, filter: VEC) -> Self
                        where
                            VEC: IntoIterator<Item = super::SQLFilter>,
                        {
                            Self(riwaq::sql::Update {
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
                            })
                        }

                        pub async fn exec(&self) -> Result<i64, String> {
                            let s = riwaq::serde_json::json!(self.0);
                            riwaq::sql::sql_exec(s).await
                        }
                    }

                    #(
                        pub fn #field_names(value: #field_types) -> Update {
                            Update(riwaq::sql::Update {
                                op: Some("Update".to_string()),
                                tbl: #t_name.to_string(),
                                values: std::collections::HashMap::from([(#field_names_str.to_string(), riwaq::serde_json::to_value(value).unwrap())]),
                                filter: None
                            })
                        }
                    )*
                }

                pub mod Delete {

                    pub struct Delete(riwaq::sql::Delete<super::SQLFilter>);
                    impl Delete {
                        pub fn and(self, filter: super::SQLFilter) -> Self {
                            Self(riwaq::sql::Delete {
                                filter: Some(match self.0.filter {
                                    Some(ex_filter) => ex_filter.and(filter),
                                    _ => riwaq::sql::FilterStmt::Filter(filter),
                                }),
                                ..self.0
                            })
                        }
                        pub fn and_all<VEC>(self, filter: VEC) -> Self
                        where
                            VEC: IntoIterator<Item = super::SQLFilter>,
                        {
                            Self(riwaq::sql::Delete {
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
                            })
                        }
                        pub fn or_any<VEC>(self, filter: VEC) -> Self
                        where
                            VEC: IntoIterator<Item = super::SQLFilter>,
                        {
                            Self(riwaq::sql::Delete {
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
                            })
                        }

                        pub async fn exec(&self) -> Result<i64, String> {
                            let s = riwaq::serde_json::json!(self.0);
                            riwaq::sql::sql_exec(s).await
                        }
                    }

                    pub fn where_(filter: super::SQLFilter) -> Delete {
                        Delete(riwaq::sql::Delete {
                            op: Some("Delete".to_string()),
                            tbl: #t_name.to_string(),
                            filter: Some(riwaq::sql::FilterStmt::Filter(filter))
                        })
                    }

                    pub fn all_rows() -> Delete {
                        Delete(riwaq::sql::Delete {
                            op: Some("Delete".to_string()),
                            tbl: #t_name.to_string(),
                            filter: None
                        })
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
