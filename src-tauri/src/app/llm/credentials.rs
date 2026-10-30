//! Legacy Windows DPAPI compatibility; new keys are stored in the local library.
#[cfg(windows)]
pub fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let length = u32::try_from(input.len()).map_err(|_| "Key 太长")?;
    let source = Blob {
        len: length,
        data: input.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        len: 0,
        data: std::ptr::null_mut(),
    };
    // CRYPTPROTECT_UI_FORBIDDEN: background calls must never show system prompts.
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &source,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &source,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err("无法使用当前 Windows 账户读取或保存 Key，请重新填写".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.data, output.len as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(output.data, 0, output.len as usize);
        LocalFree(output.data.cast());
    }
    Ok(bytes)
}
#[cfg(not(windows))]
pub fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("当前版本的安全 Key 存储需要 Windows".into())
}
