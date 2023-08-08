use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

pub mod select_from;
pub mod table;

pub fn db_init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input: ItemFn = parse_macro_input!(item);

    let body = input.block.stmts;

    TokenStream::from(quote! {
        #[no_mangle]
        extern "C" fn wasmos_settings_db_conn() -> *const u8 {
            wasmos::tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    let res: wasmos::sql::DBConn = {
                        #(#body)*
                    };
                    format!("{}\0", wasmos::serde_json::to_string(&res).unwrap()).as_ptr()
                })
        }
    })
}
