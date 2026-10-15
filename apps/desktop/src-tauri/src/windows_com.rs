//! Establish the UI apartment before any node/WinRT work can initialize COM.
use std::marker::PhantomData;
use std::rc::Rc;
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

#[derive(Debug)]
pub(crate) struct UiApartment {
    // COM initialization/uninitialization must stay on the same OS thread.
    _thread_bound: PhantomData<Rc<()>>,
}

impl UiApartment {
    pub(crate) fn sta() -> Result<Self, i32> {
        Self::initialize(COINIT_APARTMENTTHREADED as u32)
    }

    fn initialize(mode: u32) -> Result<Self, i32> {
        // Null is required for the reserved argument. The non-Send guard
        // balances both S_OK and S_FALSE once, on this same thread.
        let result = unsafe { CoInitializeEx(std::ptr::null(), mode) };
        if result < 0 {
            Err(result)
        } else {
            Ok(Self { _thread_bound: PhantomData })
        }
    }
}

impl Drop for UiApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

pub(crate) fn show_startup_failure() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND};
    let title: Vec<u16> = "Aillies".encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = "Windows could not initialize Aillies' window system. Close Aillies and reopen it. Accounts and pairings have not been changed."
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR | MB_SETFOREGROUND) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Com::COINIT_MULTITHREADED;
    const RPC_E_CHANGED_MODE: i32 = 0x80010106u32 as i32;

    #[test]
    fn ui_sta_is_balanced_and_prevents_later_mta_initialization() {
        std::thread::spawn(|| {
            {
                let _first = UiApartment::sta().unwrap();
                let _second = UiApartment::sta().unwrap(); // S_FALSE also needs balancing.
                assert_eq!(UiApartment::initialize(COINIT_MULTITHREADED as u32).unwrap_err(), RPC_E_CHANGED_MODE);
            }
            // Both STA references were released on their originating thread.
            let _mta = UiApartment::initialize(COINIT_MULTITHREADED as u32).unwrap();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn reproduce_the_reported_mta_to_sta_conflict_without_a_window_or_panic() {
        std::thread::spawn(|| {
            let _mta = UiApartment::initialize(COINIT_MULTITHREADED as u32).unwrap();
            assert_eq!(UiApartment::sta().unwrap_err(), RPC_E_CHANGED_MODE);
        })
        .join()
        .unwrap();
    }
}
