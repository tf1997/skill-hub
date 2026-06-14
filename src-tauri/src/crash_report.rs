#[cfg(windows)]
mod platform {
    use std::{
        backtrace::Backtrace,
        ffi::OsStr,
        os::windows::ffi::OsStrExt,
        panic::{self, AssertUnwindSafe, PanicHookInfo},
        process, ptr, thread,
    };

    use std::ffi::c_void;

    const EVENT_SOURCE: &str = "Skill Hub";
    const EVENTLOG_ERROR_TYPE: u16 = 0x0001;
    const EVENT_ID_RUST_PANIC: u32 = 1001;
    const EVENT_ID_RUNTIME_ERROR: u32 = 1002;
    const EVENT_ID_NATIVE_EXCEPTION: u32 = 1003;
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    const MAX_EVENT_MESSAGE_CHARS: usize = 12_000;

    type EventSourceHandle = *mut c_void;
    type UnhandledExceptionFilter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    #[repr(C)]
    #[allow(dead_code)]
    struct ExceptionPointers {
        exception_record: *mut ExceptionRecord,
        context_record: *mut c_void,
    }

    #[repr(C)]
    #[allow(dead_code)]
    struct ExceptionRecord {
        exception_code: u32,
        exception_flags: u32,
        exception_record: *mut ExceptionRecord,
        exception_address: *mut c_void,
        number_parameters: u32,
        exception_information: [usize; 15],
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn RegisterEventSourceW(
            lpuncservername: *const u16,
            lpsourcename: *const u16,
        ) -> EventSourceHandle;
        fn ReportEventW(
            heventlog: EventSourceHandle,
            wtype: u16,
            wcategory: u16,
            dweventid: u32,
            lpusersid: *const c_void,
            wnumstrings: u16,
            dwdatasize: u32,
            lpstrings: *const *const u16,
            lprawdata: *const c_void,
        ) -> i32;
        fn DeregisterEventSource(heventlog: EventSourceHandle) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(
            lptoplevel_exception_filter: Option<UnhandledExceptionFilter>,
        ) -> Option<UnhandledExceptionFilter>;
    }

    pub fn install() {
        install_panic_hook();
        install_unhandled_exception_filter();
    }

    pub fn report_fatal_error(context: &str, detail: &str) {
        let message = format!(
            "Skill Hub fatal runtime error.\r\nVersion: {}\r\nProcess ID: {}\r\nContext: {}\r\nDetail:\r\n{}",
            env!("CARGO_PKG_VERSION"),
            process::id(),
            context,
            detail
        );
        report_event(EVENT_ID_RUNTIME_ERROR, &message);
    }

    fn install_panic_hook() {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                report_panic(info);
            }));
            previous_hook(info);
        }));
    }

    fn install_unhandled_exception_filter() {
        unsafe {
            SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
        }
    }

    fn report_panic(info: &PanicHookInfo<'_>) {
        let current_thread = thread::current();
        let thread_name = current_thread.name().unwrap_or("unnamed");
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());
        let payload = panic_payload(info);
        let backtrace = Backtrace::force_capture();
        let message = format!(
            "Skill Hub crashed due to a Rust panic.\r\nVersion: {}\r\nProcess ID: {}\r\nThread: {} ({:?})\r\nLocation: {}\r\nMessage: {}\r\nBacktrace:\r\n{}",
            env!("CARGO_PKG_VERSION"),
            process::id(),
            thread_name,
            current_thread.id(),
            location,
            payload,
            backtrace
        );
        report_event(EVENT_ID_RUST_PANIC, &message);
    }

    fn panic_payload(info: &PanicHookInfo<'_>) -> String {
        if let Some(message) = info.payload().downcast_ref::<&str>() {
            return (*message).to_string();
        }
        if let Some(message) = info.payload().downcast_ref::<String>() {
            return message.clone();
        }
        "non-string panic payload".to_string()
    }

    unsafe extern "system" fn unhandled_exception_filter(
        exception_info: *mut ExceptionPointers,
    ) -> i32 {
        let _ = panic::catch_unwind(|| {
            let mut code = 0;
            let mut address = 0usize;
            if !exception_info.is_null() {
                let record = (*exception_info).exception_record;
                if !record.is_null() {
                    code = (*record).exception_code;
                    address = (*record).exception_address as usize;
                }
            }

            let message = format!(
                "Skill Hub crashed due to an unhandled Windows exception.\r\nVersion: {}\r\nProcess ID: {}\r\nException code: 0x{code:08X}\r\nException address: 0x{address:X}",
                env!("CARGO_PKG_VERSION"),
                process::id()
            );
            report_event(EVENT_ID_NATIVE_EXCEPTION, &message);
        });

        EXCEPTION_CONTINUE_SEARCH
    }

    fn report_event(event_id: u32, message: &str) {
        let source = wide_null(EVENT_SOURCE);
        let message = truncate_message(message);
        let message = wide_null(&message);
        let strings = [message.as_ptr()];

        unsafe {
            let handle = RegisterEventSourceW(ptr::null(), source.as_ptr());
            if handle.is_null() {
                return;
            }

            ReportEventW(
                handle,
                EVENTLOG_ERROR_TYPE,
                0,
                event_id,
                ptr::null(),
                strings.len() as u16,
                0,
                strings.as_ptr(),
                ptr::null(),
            );
            DeregisterEventSource(handle);
        }
    }

    fn truncate_message(message: &str) -> String {
        if message.chars().count() <= MAX_EVENT_MESSAGE_CHARS {
            return message.to_string();
        }

        let mut truncated = message
            .chars()
            .take(MAX_EVENT_MESSAGE_CHARS)
            .collect::<String>();
        truncated.push_str("\r\n...[truncated]");
        truncated
    }

    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn install() {}

    pub fn report_fatal_error(_context: &str, _detail: &str) {}
}

pub fn install() {
    platform::install();
}

pub fn report_fatal_error(context: &str, detail: &str) {
    platform::report_fatal_error(context, detail);
}
