use ones_core::application::{Context};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char};

#[repr(C)]
pub enum ReturnCode {
    SUCCESS = 0,
    FAIL,
}
#[repr(C)]
pub enum COption<T> {
    None,
    Some(T),
}
#[repr(C)]
pub struct Return<T> {
    data: COption<T>,
    code: ReturnCode,
}
#[repr(C)]
pub struct CContext {
    db_path: *const c_char,
    cache_dir: *const c_char,
}

type ReturnContext = Return<CContext>;
#[no_mangle]
pub extern "C" fn get_context(db_path: *const c_char, cache_dir: *const c_char) -> ReturnContext {
    Return {
        data: COption::Some(CContext { db_path, cache_dir }),
        code: ReturnCode::SUCCESS,
    }
}

type ReturnStr = Return<*mut c_char>;
#[no_mangle]
pub extern "C" fn get_resource(
    this: *const CContext,
    url: *const c_char,
    disable_cache: bool,
) -> ReturnStr {
    let url = unsafe { CStr::from_ptr(url).to_str().unwrap() };
    let db_path = unsafe { CStr::from_ptr((*this).db_path).to_str().unwrap() };
    let cache_dir = unsafe { CStr::from_ptr((*this).cache_dir).to_str().unwrap() };
    let context = Context::new(db_path, cache_dir);
    match context.get_resource(url, disable_cache) {
        Ok(path) => match path {
            Some(path) => ReturnStr {
                data: COption::Some(CString::new(path).unwrap().into_raw()),
                code: ReturnCode::SUCCESS,
            },
            None => ReturnStr {
                data: COption::None,
                code: ReturnCode::FAIL,
            },
        },
        Err(err) => {
            println!("get_resource fail, {:}", err);
            return ReturnStr {
                data: COption::None,
                code: ReturnCode::FAIL,
            };
        }
    }
}

#[no_mangle]
pub extern "C" fn get_app_info(
    this: *const CContext,
    server: *const c_char,
    id: *const c_char,
) -> ReturnStr {
    let id = unsafe { CStr::from_ptr(id).to_str().unwrap() };
    let server = unsafe { CStr::from_ptr(server).to_str().unwrap() };
    let db_path = unsafe { CStr::from_ptr((*this).db_path).to_str().unwrap() };
    let cache_dir = unsafe { CStr::from_ptr((*this).cache_dir).to_str().unwrap() };
    let context = Context::new(db_path, cache_dir);
    match context.get_app_info_str(server, id) {
        Ok(Some(json)) => ReturnStr {
            data: COption::Some(CString::new(json).unwrap().into_raw()),
            code: ReturnCode::SUCCESS,
        },
        Ok(None) => {
            println!("get_app_info empty, maybe id not exist");
            ReturnStr {
                data: COption::None,
                code: ReturnCode::SUCCESS,
            }
        }
        Err(err) => {
            println!("get_app_info fail: {:}", err);
            ReturnStr {
                data: COption::None,
                code: ReturnCode::FAIL,
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn free_context(this: *const CContext) {
    todo!()
}