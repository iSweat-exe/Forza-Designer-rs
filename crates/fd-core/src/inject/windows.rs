//! Win32 implementation of [`ProcessMemory`] (`OpenProcess` + `Read/WriteProcessMemory`).

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HMODULE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};
use windows_sys::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_IMAGE, MEM_PRIVATE, PAGE_EXECUTE,
    PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_READONLY,
    PAGE_READWRITE, PAGE_WRITECOPY,
};
use windows_sys::Win32::System::ProcessStatus::{K32EnumProcessModulesEx, LIST_MODULES_ALL};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};

use super::layout::GameProfile;
use super::memory::{MemoryRegion, ProcessMemory, RegionKind};
use super::InjectError;

const READABLE: u32 = PAGE_READONLY
    | PAGE_READWRITE
    | PAGE_WRITECOPY
    | PAGE_EXECUTE_READ
    | PAGE_EXECUTE_READWRITE
    | PAGE_EXECUTE_WRITECOPY;
const WRITABLE: u32 =
    PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
const EXECUTABLE: u32 =
    PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
/// Highest user-mode address on x64 Windows.
const USER_CEILING: u64 = 0x7FFF_FFFF_FFFF;

/// First process whose executable name matches one of `names` (case-insensitive).
pub fn find_process_id(names: &[&str]) -> Option<u32> {
    // SAFETY: plain Toolhelp enumeration; the snapshot handle is closed on every path.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if names.iter().any(|n| n.eq_ignore_ascii_case(&exe)) {
                found = Some(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
        found
    }
}

pub struct GameProcess {
    handle: HANDLE,
    pid: u32,
}

// SAFETY: a process handle is just a kernel object reference. ReadProcessMemory,
// WriteProcessMemory and VirtualQueryEx are safe to call concurrently from any
// thread; the handle is closed exactly once, in `Drop`.
unsafe impl Send for GameProcess {}
unsafe impl Sync for GameProcess {}

impl GameProcess {
    pub fn attach(game: &'static GameProfile) -> Result<Self, InjectError> {
        let pid = find_process_id(game.process_names).ok_or_else(|| InjectError::NotRunning {
            game: game.label,
            names: game.process_names.join(" / "),
        })?;
        let access =
            PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION | PROCESS_QUERY_INFORMATION;
        // SAFETY: OpenProcess has no memory-safety preconditions.
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            // SAFETY: GetLastError just reads thread-local state.
            let code = unsafe { GetLastError() };
            return Err(InjectError::Attach {
                pid,
                reason: format!("OpenProcess failed (Win32 error {code})"),
            });
        }
        Ok(Self { handle, pid })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// File version of the running executable (e.g. `364.933`), best effort.
    pub fn game_build(&self) -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: buf/len describe a valid writable buffer.
        let ok = unsafe { QueryFullProcessImageNameW(self.handle, 0, buf.as_mut_ptr(), &mut len) };
        if ok == 0 {
            return None;
        }
        let mut path: Vec<u16> = buf[..len as usize].to_vec();
        path.push(0);
        // SAFETY: `path` is NUL-terminated; `data` is sized by the API itself.
        unsafe {
            let size = GetFileVersionInfoSizeW(path.as_ptr(), null_mut());
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
                return None;
            }
            let mut block: *mut c_void = null_mut();
            let mut blen = 0u32;
            let root: [u16; 2] = [u16::from(b'\\'), 0];
            if VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut block, &mut blen) == 0
                || block.is_null()
                || (blen as usize) < size_of::<VS_FIXEDFILEINFO>()
            {
                return None;
            }
            let info = &*block.cast::<VS_FIXEDFILEINFO>();
            let mut parts = [
                info.dwFileVersionMS >> 16,
                info.dwFileVersionMS & 0xFFFF,
                info.dwFileVersionLS >> 16,
                info.dwFileVersionLS & 0xFFFF,
            ]
            .to_vec();
            while parts.len() > 2 && parts.last() == Some(&0) {
                parts.pop();
            }
            Some(
                parts
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join("."),
            )
        }
    }
}

impl Drop for GameProcess {
    fn drop(&mut self) {
        // SAFETY: handle came from OpenProcess and is closed exactly once.
        unsafe { CloseHandle(self.handle) };
    }
}

impl ProcessMemory for GameProcess {
    fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        if len == 0 {
            return Some(Vec::new());
        }
        let mut buf = vec![0u8; len];
        let mut read = 0usize;
        // SAFETY: buf is valid for `len` bytes; the kernel validates `addr`.
        // The return value is ignored on purpose: a partial copy fails yet still fills `read`.
        let _ = unsafe {
            ReadProcessMemory(
                self.handle,
                addr as *const c_void,
                buf.as_mut_ptr().cast(),
                len,
                &mut read,
            )
        };
        if read == 0 {
            return None;
        }
        buf.truncate(read);
        Some(buf)
    }

    fn write(&self, addr: u64, data: &[u8]) -> Result<(), InjectError> {
        let mut written = 0usize;
        // SAFETY: data is valid for its length; the kernel validates `addr`.
        let ok = unsafe {
            WriteProcessMemory(
                self.handle,
                addr as *const c_void,
                data.as_ptr().cast(),
                data.len(),
                &mut written,
            )
        };
        if ok == 0 || written != data.len() {
            // SAFETY: reads thread-local error state.
            let code = unsafe { GetLastError() };
            return Err(InjectError::Memory {
                addr,
                reason: format!("WriteProcessMemory failed (Win32 error {code})"),
            });
        }
        Ok(())
    }

    fn regions(&self) -> Vec<MemoryRegion> {
        let mut out = Vec::new();
        let mut addr = 0u64;
        while addr < USER_CEILING {
            // SAFETY: MEMORY_BASIC_INFORMATION is plain data; size matches the struct.
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { zeroed() };
            let n = unsafe {
                VirtualQueryEx(
                    self.handle,
                    addr as *const c_void,
                    &mut mbi,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            if n == 0 {
                break;
            }
            let base = mbi.BaseAddress as u64;
            let size = mbi.RegionSize as u64;
            if mbi.State == MEM_COMMIT {
                let ok_protect = mbi.Protect & PAGE_GUARD == 0;
                out.push(MemoryRegion {
                    base,
                    size,
                    kind: match mbi.Type {
                        MEM_IMAGE => RegionKind::Image,
                        MEM_PRIVATE => RegionKind::Private,
                        _ => RegionKind::Mapped,
                    },
                    readable: ok_protect && mbi.Protect & READABLE != 0,
                    writable: ok_protect && mbi.Protect & WRITABLE != 0,
                    executable: mbi.Protect & EXECUTABLE != 0,
                });
            }
            match base.checked_add(size) {
                Some(next) if next > addr => addr = next,
                _ => break,
            }
        }
        out
    }

    fn main_module_base(&self) -> Option<u64> {
        let mut modules: [HMODULE; 1] = [null_mut()];
        let mut needed = 0u32;
        // SAFETY: buffer of one HMODULE; the first module returned is the exe.
        let ok = unsafe {
            K32EnumProcessModulesEx(
                self.handle,
                modules.as_mut_ptr(),
                size_of::<HMODULE>() as u32,
                &mut needed,
                LIST_MODULES_ALL,
            )
        };
        (ok != 0 && !modules[0].is_null()).then(|| modules[0] as u64)
    }
}
