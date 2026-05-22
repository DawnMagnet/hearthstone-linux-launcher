use std::{ffi::CString, os::raw::c_char};

fn leaked_c_string(value: &str) -> *const c_char {
    CString::new(value).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_create___() -> isize {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_get___(
    _result: isize,
) -> isize {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_get___(
    _result: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_init___(
    _sdk: isize,
    _init_params: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_terminate___(_sdk: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register___(
    _sdk: isize,
    _manifest: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_catalog___(_sdk: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_checkout___(_sdk: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_http___(_sdk: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_scene___(_sdk: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_vc___(_sdk: isize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_add_listener___(
    _sdk: isize,
    _listener: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn SWIGRegisterStringCallback_battlenet_commerce(_callback: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_update___(_sdk: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_unregister_log___(_owner: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_log___(
    _owner: isize,
    _hook: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_register_blz_http___(
    _sdk: isize,
    _http_client: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_load_products___(
    _sdk: isize,
    _request: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(
    _sdk: isize,
    _request: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_purchase___(
    _sdk: isize,
    _purchase: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___(
    _sdk: isize,
    _purchase: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___(
    _sdk: isize,
    _cancel: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_resume___(_sdk: isize) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(
    _sdk: isize,
    _request: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___(
    _sdk: isize,
    _request: isize,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_generate_transaction_id___() -> *const c_char
{
    leaked_c_string("unsupported")
}
