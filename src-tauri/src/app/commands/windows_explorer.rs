//! Ask the Windows Shell to reuse the target folder's window and select a file.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::Mutex;

// Serialize requests, including clicks from separate app windows, while the
// Shell opens a folder. Do not cache window handles: users can navigate or close them.
static REVEAL_LOCK: Mutex<()> = Mutex::new(());

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(memory: *const c_void);
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHParseDisplayName(
        name: *const u16,
        bind_context: *mut c_void,
        item: *mut *mut c_void,
        attributes_in: u32,
        attributes_out: *mut u32,
    ) -> i32;
    fn SHOpenFolderAndSelectItems(
        item: *const c_void,
        count: u32,
        children: *const *const c_void,
        flags: u32,
    ) -> i32;
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: Created only after successful initialization on this thread.
        unsafe { CoUninitialize() };
    }
}

struct ShellItem(*mut c_void);

impl Drop for ShellItem {
    fn drop(&mut self) {
        // SAFETY: SHParseDisplayName allocates the PIDL with the COM allocator.
        unsafe { CoTaskMemFree(self.0) };
    }
}

fn check_result(result: i32, operation: &str) -> Result<(), String> {
    if result < 0 {
        Err(format!(
            "{operation}失败（Windows 错误 0x{:08X}）",
            result as u32
        ))
    } else {
        Ok(())
    }
}

// Called on a blocking worker, never on the UI thread. No process launch or
// fallback to explorer /select: that would recreate the duplicate-window bug.
pub(super) fn select_file(path: &Path) -> Result<(), String> {
    let _lock = REVEAL_LOCK.lock().map_err(|error| error.to_string())?;
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut name: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    if name.contains(&0) {
        return Err("文件路径包含无效字符".into());
    }
    name.push(0);

    // SAFETY: Reserved is null; 2 is COINIT_APARTMENTTHREADED. The guard balances
    // both S_OK and S_FALSE, and remains on the calling worker for its lifetime.
    check_result(
        unsafe { CoInitializeEx(ptr::null_mut(), 2) },
        "初始化文件管理器",
    )?;
    let _apartment = ComApartment;
    let mut item = ShellItem(ptr::null_mut());
    // SAFETY: name is NUL-terminated UTF-16 and the output pointer is valid.
    check_result(
        unsafe {
            SHParseDisplayName(
                name.as_ptr(),
                ptr::null_mut(),
                &mut item.0,
                0,
                ptr::null_mut(),
            )
        },
        "定位图片文件",
    )?;
    if item.0.is_null() {
        return Err("无法定位图片文件".into());
    }
    // SAFETY: A valid absolute PIDL with count=0 selects this single file in its
    // parent folder. The Shell handles existing windows and brings one forward.
    // https://learn.microsoft.com/windows/win32/api/shlobj_core/nf-shlobj_core-shopenfolderandselectitems
    check_result(
        unsafe { SHOpenFolderAndSelectItems(item.0, 0, ptr::null(), 0) },
        "在文件管理器中打开",
    )
}
