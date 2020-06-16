use std::ffi::c_void;

use crate::vm::error::VMLogicError;
use crate::vm::logic::VMLogic;
use wasmer_runtime::memory::Memory;
use wasmer_runtime::{func, imports, Ctx, ImportObject};

type Result<T> = ::std::result::Result<T, VMLogicError>;
struct ImportReference(*mut c_void);
unsafe impl Send for ImportReference {}
unsafe impl Sync for ImportReference {}

macro_rules! wrapped_imports {
        ( $( $func:ident < [ $( $arg_name:ident : $arg_type:ident ),* ] -> [ $( $returns:ident ),* ] >, )* ) => {
            $(
                #[allow(unused_parens)]
                fn $func( ctx: &mut Ctx, $( $arg_name: $arg_type ),* ) -> Result<($( $returns ),*)> {
                    let logic: &mut VMLogic<'_> = unsafe { &mut *(ctx.data as *mut VMLogic<'_>) };
                    logic.$func( $( $arg_name, )* )
                }
            )*

            pub(crate) fn build(memory: Memory, logic: &mut VMLogic<'_>) -> ImportObject {
                let raw_ptr = logic as *mut _ as *mut c_void;
                let import_reference = ImportReference(raw_ptr);
                imports! {
                    move || {
                        let dtor = (|_: *mut c_void| {}) as fn(*mut c_void);
                        (import_reference.0, dtor)
                    },
                    "env" => {
                        "memory" => memory,
                        $(
                            stringify!($func) => func!($func),
                        )*
                    },
                }
            }
        }
    }

wrapped_imports! {
    // #############
    // # Registers #
    // #############
    read_register<[register_id: u64, ptr: u64] -> []>,
    register_len<[register_id: u64] -> [u64]>,
}
