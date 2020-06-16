#![no_main]

#[link(wasm_import_module = "host")]
extern "C" {
    fn get_counter() -> i32;
    fn add_to_counter(value_to_add: i32, add_value: i32) -> i32;
    // #############
    // # Registers #
    // #############
    pub fn read_register(register_id: u64, ptr: u64);
    pub fn register_len(register_id: u64) -> u64;
    // #####################
    // # Miscellaneous API #
    // #####################
    fn value_return(value_len: u64, value_ptr: u64);
}

#[link(wasm_import_module = "mem")]
extern "C" {
    // #############
    // # Registers #
    // #############
    pub fn read_register(register_id: u64, ptr: u64);
    pub fn register_len(register_id: u64) -> u64;
    // #####################
    // # Miscellaneous API #
    // #####################
    fn value_return(value_len: u64, value_ptr: u64);
}


#[no_mangle]
pub fn add_one(value: i32) -> i32 {
    return value + 1;
}

#[no_mangle]
pub fn multiply_2(value: i32) -> i32 {
    return value * 2;
}

#[no_mangle]
pub fn increment_counter(counter_value: i32, add_value: i32) -> i32 {
    let mut current_counter;
    unsafe {
        current_counter = add_to_counter(counter_value, add_value);
    }
    current_counter
}

#[no_mangle]
unsafe fn ext_read_register(register_id: u64, ptr: u64) {
    sys::read_register(register_id, ptr)
}

#[no_mangle]
unsafe fn ext_register_len(register_id: u64) -> u64 {
    sys::register_len(register_id)
}

#[no_mangle]
unsafe fn ext_read_vec() {
    let bytes = vec![0; register_len(0) as usize];
    read_register(0, bytes.as_ptr() as *const u64 as u64);
}
