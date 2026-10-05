use std::path::PathBuf;

use anyhow::{Context, Result};
use windows::core::{GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{SHGetKnownFolderPath, SHStrDupW, KF_FLAG_DEFAULT};

pub struct Com;

impl Com {
    pub fn initialize() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .context("failed to initialize COM")?;
        Ok(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub fn known_folder(id: &GUID) -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
        .context("failed to look up a known folder")?;
    taken_string(path).map(PathBuf::from)
}

pub fn set_string(store: &IPropertyStore, key: &PROPERTYKEY, value: &str) -> Result<()> {
    unsafe {
        let mut variant = PROPVARIANT::default();
        (*variant.Anonymous.Anonymous).vt = VT_LPWSTR;
        (*variant.Anonymous.Anonymous).Anonymous.pwszVal = SHStrDupW(&HSTRING::from(value))?;
        let result = store.SetValue(key, &variant);
        PropVariantClear(&mut variant)?;
        result.context("failed to set a shell property")
    }
}

#[cfg(test)]
pub fn get_string(store: &IPropertyStore, key: &PROPERTYKEY) -> Result<String> {
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;

    unsafe {
        let mut variant = store.GetValue(key)?;
        let text = PropVariantToStringAlloc(&variant);
        PropVariantClear(&mut variant)?;
        taken_string(text?)
    }
}

fn taken_string(text: PWSTR) -> Result<String> {
    let result = unsafe { text.to_string() }.context("a shell string was not valid UTF-16");
    unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
    result
}
