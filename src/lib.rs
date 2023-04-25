use proc_macro::TokenStream;

mod gql;
mod sql;

#[proc_macro_derive(Object)]
pub fn object(item: TokenStream) -> TokenStream {
    gql::object(item)
}

#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    gql::handler(attr, item)
}


#[proc_macro_attribute]
pub fn table(attr: TokenStream, item: TokenStream) -> TokenStream {
    sql::table(attr, item)
}
