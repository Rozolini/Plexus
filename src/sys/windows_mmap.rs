use core::mem::size_of;
use core::ptr::{NonNull, null_mut};

use thiserror::Error;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    MapViewOfFile, PAGE_READONLY, PAGE_READWRITE, UnmapViewOfFile,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Error)]
pub enum MappingError {
    #[error("mapping size must be > 0")]
    InvalidSize,
    #[error("mapping name must not be empty")]
    InvalidName,
    #[error("mapping name exceeds win32 object namespace limits")]
    NameTooLong,
    #[error("mapping name must use Local\\ or Global\\ namespace")]
    InvalidNamespace,
    #[error("acl profile setup failed")]
    InvalidAclProfile,
    #[error("win32 api failed with error code {0}")]
    OsError(u32),
    #[error("null mapping pointer")]
    NullMapping,
}

pub struct SharedMapping {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    ptr: NonNull<u8>,
    len: usize,
    created_new: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingSecurityProfile {
    /// Owner + local system full access.
    ProducerOwnerOnly,
    /// Authenticated users read, owner/system full control.
    ProducerConsumerLocalUsers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappingMetadata {
    pub metadata_version: u16,
    pub created_new: bool,
    pub length: usize,
}

impl SharedMapping {
    pub fn create(name: &str, len: usize, access: MapAccess) -> Result<Self, MappingError> {
        Self::create_impl(name, len, access, None)
    }

    pub fn create_with_profile(
        name: &str,
        len: usize,
        access: MapAccess,
        security_profile: MappingSecurityProfile,
    ) -> Result<Self, MappingError> {
        Self::create_impl(name, len, access, Some(security_profile))
    }

    fn create_impl(
        name: &str,
        len: usize,
        access: MapAccess,
        security_profile: Option<MappingSecurityProfile>,
    ) -> Result<Self, MappingError> {
        if len == 0 {
            return Err(MappingError::InvalidSize);
        }
        if name.trim().is_empty() {
            return Err(MappingError::InvalidName);
        }
        if name.encode_utf16().count() >= 250 {
            return Err(MappingError::NameTooLong);
        }
        if !name.starts_with("Local\\") && !name.starts_with("Global\\") {
            return Err(MappingError::InvalidNamespace);
        }

        let wide_name = to_wide(name);
        let protect = match access {
            MapAccess::ReadOnly => PAGE_READONLY,
            MapAccess::ReadWrite => PAGE_READWRITE,
        };
        let desired_access = match access {
            MapAccess::ReadOnly => FILE_MAP_READ,
            MapAccess::ReadWrite => FILE_MAP_ALL_ACCESS,
        };

        let mut descriptor = null_mut();
        let security_attributes = if let Some(profile) = security_profile {
            descriptor = build_descriptor_for_profile(profile)?;
            Some(SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            })
        } else {
            None
        };
        let security_attributes_ptr = security_attributes
            .as_ref()
            .map_or(null_mut::<SECURITY_ATTRIBUTES>(), |attrs| {
                attrs as *const _ as *mut _
            });

        // SAFETY[INV-MAP-LIFETIME]: WinAPI call with validated inputs.
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                security_attributes_ptr,
                protect,
                (len as u64 >> 32) as u32,
                len as u32,
                wide_name.as_ptr(),
            )
        };
        // SAFETY[INV-MAP-LIFETIME]: descriptor allocated by ConvertString... and released once.
        unsafe {
            if !descriptor.is_null() {
                LocalFree(descriptor.cast());
            }
        }
        if handle.is_null() {
            // SAFETY[INV-MAP-LIFETIME]: querying thread-local Win32 error after failed call.
            let last_error = unsafe { GetLastError() };
            return Err(MappingError::OsError(last_error));
        }
        // SAFETY[INV-MAP-LIFETIME]: querying thread-local Win32 error after CreateFileMappingW.
        let create_error = unsafe { GetLastError() };
        let created_new = create_error != ERROR_ALREADY_EXISTS;

        // SAFETY[INV-MAP-LIFETIME]: handle is a live mapping handle.
        let mapped_ptr = unsafe { MapViewOfFile(handle, desired_access, 0, 0, len) };
        if mapped_ptr.Value.is_null() {
            // SAFETY[INV-MAP-LIFETIME]: handle was created by CreateFileMappingW and must be closed.
            unsafe { CloseHandle(handle) };
            // SAFETY[INV-MAP-LIFETIME]: querying thread-local Win32 error after failed map view.
            return Err(MappingError::OsError(unsafe { GetLastError() }));
        }

        let ptr = NonNull::new(mapped_ptr.Value.cast::<u8>()).ok_or(MappingError::NullMapping)?;
        Ok(Self {
            handle,
            view: mapped_ptr,
            ptr,
            len,
            created_new,
        })
    }

    #[must_use]
    pub fn as_non_null(&self) -> NonNull<u8> {
        self.ptr
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn created_new(&self) -> bool {
        self.created_new
    }

    #[must_use]
    pub fn metadata(&self) -> MappingMetadata {
        MappingMetadata {
            metadata_version: 1,
            created_new: self.created_new,
            length: self.len,
        }
    }
}

impl Drop for SharedMapping {
    fn drop(&mut self) {
        // SAFETY[INV-MAP-LIFETIME]: resources acquired by WinAPI and released exactly once.
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.handle);
        }
    }
}

fn to_wide(input: &str) -> Vec<u16> {
    input.encode_utf16().chain(core::iter::once(0)).collect()
}

fn build_descriptor_for_profile(
    profile: MappingSecurityProfile,
) -> Result<*mut core::ffi::c_void, MappingError> {
    let sddl = match profile {
        // SY and BA full control.
        MappingSecurityProfile::ProducerOwnerOnly => "D:P(A;;GA;;;SY)(A;;GA;;;BA)",
        // SY+BA full control, everyone read/write for local IPC interoperability.
        MappingSecurityProfile::ProducerConsumerLocalUsers => {
            "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;WD)"
        }
    };

    let wide = to_wide(sddl);
    let mut descriptor: *mut core::ffi::c_void = null_mut();
    // SAFETY[INV-MAP-LIFETIME]: WinAPI converts valid SDDL into allocated descriptor.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1 as u32,
            &mut descriptor,
            null_mut(),
        )
    };
    if ok == 0 || descriptor.is_null() {
        return Err(MappingError::InvalidAclProfile);
    }
    Ok(descriptor)
}
