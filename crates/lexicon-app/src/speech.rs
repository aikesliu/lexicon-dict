//! 发音：Linux 用 espeak-ng + pw-play；Windows 用 SAPI。

pub fn speak(word: &str, accent: &str) {
    let word = word.trim();
    if word.is_empty() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        use std::sync::Mutex;
        static CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
        let voice = if accent == "uk" { "en-gb" } else { "en-us" };
        if let Ok(mut guard) = CHILD.lock() {
            if let Some(mut p) = guard.take() {
                let _ = p.kill();
                let _ = p.wait();
            }
            if let Ok(mut espeak) = Command::new("espeak-ng")
                .args(["-v", voice, "-s", "160", "--stdout", word])
                .stdout(Stdio::piped())
                .spawn()
            {
                if let Some(out) = espeak.stdout.take() {
                    if let Ok(p) = Command::new("pw-play")
                        .arg("-")
                        .stdin(Stdio::from(out))
                        .spawn()
                    {
                        *guard = Some(p);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        windows_speak(word);
    }
}

#[cfg(target_os = "windows")]
fn windows_speak(word: &str) {
    use std::cell::RefCell;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Media::Speech::{ISpVoice, SPF_ASYNC, SPF_PURGEBEFORESPEAK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    const CLSID_SPVOICE: GUID = GUID::from_u128(0x96749377_3391_11d2_9ee3_00c04f797396);
    thread_local! {
        static VOICE: RefCell<Option<ISpVoice>> = const { RefCell::new(None) };
    }
    VOICE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                *slot = CoCreateInstance(&CLSID_SPVOICE, None, CLSCTX_ALL).ok();
            }
        }
        let Some(v) = slot.as_ref() else { return };
        let mut wide: Vec<u16> = word.encode_utf16().collect();
        wide.push(0);
        unsafe {
            let _ = v.Speak(
                PCWSTR(wide.as_ptr()),
                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                None,
            );
        }
    });
}
