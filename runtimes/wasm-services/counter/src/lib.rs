#![no_main]

#[no_mangle]
pub fn add_one(value: i32) -> i32 {
    return value + 1;
}


#[no_mangle]
pub fn multiply_2(value: i32) -> i32 {
    return value * 2;
}
