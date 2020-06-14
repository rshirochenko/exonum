#![no_main]

#[link(wasm_import_module = "host")]
extern "C" {
    fn get_counter() -> i32;
    fn add_to_counter(value_to_add: i32, add_value: i32) -> i32;
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
