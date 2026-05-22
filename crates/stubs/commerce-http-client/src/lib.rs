#![allow(non_snake_case)]

use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn SetHttpOptions(
    _max_connections: i32,
    _request_timeout_seconds: i32,
    _persistent_data_path: *const c_char,
    _disable_server_verify: bool,
    _disable_certificate_revoke_check: bool,
) -> isize {
    1
}
