//! Commande de l'Explorateur Windows 11 (menu contextuel moderne) : lance dcleaner.exe sur le dossier/lecteur.
//! Chargée par Explorer via le paquet « sparse » (voir package/AppxManifest.xml).
use std::{ffi::c_void, path::PathBuf};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        System::{Com::*, LibraryLoader::*},
        UI::Shell::*,
    },
};

/// Doit rester identique à package/AppxManifest.xml.
const CLSID: GUID = GUID::from_u128(0x5d3b0b7e_8f0a_4c1e_9a62_1c7e5f4d2a10);

/// Dossier de cette DLL (dcleaner.exe est à côté).
fn dll_dir() -> Option<PathBuf> {
    unsafe {
        let mut h = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(dll_dir as *const u16),
            &mut h,
        )
        .ok()?;
        let mut buf = [0u16; 1024];
        let n = GetModuleFileNameW(Some(h), &mut buf) as usize;
        let p = PathBuf::from(String::from_utf16(&buf[..n]).ok()?);
        p.parent().map(PathBuf::from)
    }
}

#[implement(IExplorerCommand)]
struct Cmd;

impl IExplorerCommand_Impl for Cmd_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        unsafe { SHStrDupW(w!("Inspecter et nettoyer avec DCleaner")) }
    }

    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        let exe = dll_dir().ok_or(E_FAIL)?.join("dcleaner.exe");
        unsafe { SHStrDupW(&HSTRING::from(format!("{},0", exe.display()))) }
    }

    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID)
    }

    fn GetState(&self, _items: Ref<IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(&self, items: Ref<IShellItemArray>, _bind: Ref<IBindCtx>) -> Result<()> {
        let items = items.ok()?;
        unsafe {
            let name = items.GetItemAt(0)?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = name.to_string();
            CoTaskMemFree(Some(name.0 as *const c_void));
            let exe = dll_dir().ok_or(E_FAIL)?.join("dcleaner.exe");
            std::process::Command::new(exe).arg(path.map_err(|_| E_FAIL)?).spawn().map_err(|_| E_FAIL)?;
        }
        Ok(())
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<IUnknown>, iid: *const GUID, obj: *mut *mut c_void) -> Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let cmd: IExplorerCommand = Cmd.into();
        unsafe { cmd.query(iid, obj).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
pub extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, obj: *mut *mut c_void) -> HRESULT {
    unsafe {
        if clsid.is_null() || iid.is_null() || obj.is_null() {
            return E_POINTER;
        }
        if *clsid != CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory.into();
        factory.query(iid, obj)
    }
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}
