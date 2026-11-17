//! Android entry point.
//!
//! Android has no `main()`. The OS loads this shared library into an Activity
//! and calls `android_main`, so this crate is a `cdylib` rather than a binary.
//! Everything below it — engine, transfer, UI — is the same code the desktop
//! runs; only the process lifecycle and a few Android-specific details differ.
//!
//! Diagnostics go to logcat under the tag `zenshare`, because a native failure
//! on Android is otherwise completely silent: the Activity finishes and the user
//! lands on the home screen with nothing to read.

#![deny(missing_docs)]
// JNI is unsafe by nature: we hand raw pointers to the JVM and call methods by
// name and signature. The unsafety is confined to `acquire_multicast_lock`.
#![allow(unsafe_code)]

#[cfg(target_os = "android")]
mod android {
    use std::io::Write;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;
    use std::sync::{Arc, OnceLock, RwLock};

    use android_activity::AndroidApp;
    use jni::JavaVM;
    use jni::objects::{GlobalRef, JObject, JValue};
    use jni::sys::jobject;

    use zs_app::engine;
    use zs_app::paths::AppPaths;
    use zs_ui::{AppOptions, UiModel};

    /// Android's Wi-Fi stack drops multicast packets unless the app holds a
    /// `WifiManager.MulticastLock`. Without one, discovery looks like it is
    /// working — the socket binds, announcements go out — and no peer ever
    /// appears, because nothing comes back. This is the single most important
    /// line of Android-specific code in the project.
    static MULTICAST_LOCK: OnceLock<Option<GlobalRef>> = OnceLock::new();

    /// Forwards formatted tracing output to logcat.
    struct Logcat;

    impl Write for Logcat {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            for line in String::from_utf8_lossy(buf).lines() {
                if !line.trim().is_empty() {
                    log::info!("{line}");
                }
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn init_logging() {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("zenshare"),
        );

        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(|| Logcat)
            .try_init();

        // A panic on a background thread used to vanish entirely. This at least
        // puts it in logcat with a file and line.
        std::panic::set_hook(Box::new(|info| {
            log::error!("panic: {info}");
        }));
    }

    /// Called by `android-activity` once the Activity is ready.
    ///
    /// `#[unsafe(no_mangle)]` is required in edition 2024; the symbol name is
    /// what the loader looks up with `dlsym`. It is `pub` so the symbol is
    /// unambiguously exported from the cdylib.
    #[unsafe(no_mangle)]
    pub fn android_main(app: AndroidApp) {
        init_logging();
        log::info!("ZenShare starting ({} build)", env!("CARGO_PKG_VERSION"));

        log::info!("acquiring multicast lock");
        let _ = MULTICAST_LOCK.set(acquire_multicast_lock(&app));

        // Android has no $HOME. Internal storage is private and always writable;
        // external storage is app-scoped but visible over USB, which is where
        // received files should land so the user can actually find them.
        let internal = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
        log::info!("internal storage: {}", internal.display());
        log::info!("external storage: {:?}", app.external_data_path());

        let paths = Arc::new(AppPaths::android(internal, app.external_data_path()));
        if let Err(e) = paths.ensure() {
            log::error!("could not create directories: {e:#}");
        }

        let model = Arc::new(RwLock::new(UiModel::default()));
        let (tx, rx) = async_channel::unbounded();

        if let Err(e) = engine::spawn(
            Arc::clone(&model),
            rx,
            Ipv4Addr::UNSPECIFIED,
            Arc::clone(&paths),
        ) {
            log::error!("engine failed to start: {e:#}");
        }

        let mut options = eframe::NativeOptions::default();
        options.android_app = Some(app);
        options.viewport = eframe::egui::ViewportBuilder::default().with_title("ZenShare");
        // OpenGL ES 3 rather than Vulkan: a device with no usable Vulkan driver
        // cannot create a wgpu surface at all, and the app exits before drawing
        // a single pixel. GLES is present on everything.
        options.renderer = eframe::Renderer::Glow;

        // The result used to be discarded with `let _ =`, which meant a failure
        // here looked exactly like a normal quit: the Activity finishes and the
        // user lands on the home screen with no message. Never swallow this.
        match eframe::run_native(
            "ZenShare",
            options,
            Box::new(move |cc| Ok(zs_ui::make_app(cc, model, tx, AppOptions::default()))),
        ) {
            Ok(()) => log::info!("window closed normally"),
            Err(e) => log::error!("could not create the window: {e}"),
        }
    }

    /// Acquire the multicast lock, returning it so it is never dropped.
    ///
    /// Every step is best-effort: if any JNI call fails we return `None` and the
    /// app runs without discovery rather than refusing to start. A phone that
    /// cannot find peers is annoying; a phone that crashes on launch is broken.
    fn acquire_multicast_lock(app: &AndroidApp) -> Option<GlobalRef> {
        // SAFETY: both pointers are owned by android-activity and remain valid
        // for the lifetime of the process.
        let vm = match unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) } {
            Ok(vm) => vm,
            Err(e) => {
                log::error!("multicast lock: JavaVM::from_raw failed: {e}");
                return None;
            }
        };

        let mut env = match vm.attach_current_thread() {
            Ok(env) => env,
            Err(e) => {
                log::error!("multicast lock: attach_current_thread failed: {e}");
                return None;
            }
        };

        // `activity_as_ptr` returns a `*mut c_void` whose *value is* the jobject.
        // Reinterpret the pointer, do not dereference it: casting to `jobject`
        // first would make it a `*mut *mut _jobject` and reading through it
        // would hand back the Java object's first field instead of the object.
        // SAFETY: the Activity reference is valid while android_main is running.
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jobject) };

        let service = match env.new_string("wifi") {
            Ok(s) => s,
            Err(e) => {
                log::error!("multicast lock: new_string failed: {e}");
                return None;
            }
        };

        let wifi = match env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&JObject::from(service))],
            )
            .and_then(jni::objects::JValueOwned::l)
        {
            Ok(w) if !w.is_null() => w,
            Ok(_) => {
                log::error!("multicast lock: getSystemService(\"wifi\") returned null");
                return None;
            }
            Err(e) => {
                log::error!("multicast lock: getSystemService failed: {e}");
                return None;
            }
        };

        let tag = match env.new_string("zenshare") {
            Ok(s) => s,
            Err(e) => {
                log::error!("multicast lock: new_string failed: {e}");
                return None;
            }
        };

        let lock = match env
            .call_method(
                &wifi,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&JObject::from(tag))],
            )
            .and_then(jni::objects::JValueOwned::l)
        {
            Ok(l) if !l.is_null() => l,
            Ok(_) => {
                log::error!("multicast lock: createMulticastLock returned null");
                return None;
            }
            Err(e) => {
                log::error!("multicast lock: createMulticastLock failed: {e}");
                return None;
            }
        };

        // Reference-counted locks can be released by anything; a single
        // acquire/release pair is easier to reason about.
        if let Err(e) =
            env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(0)])
        {
            log::error!("multicast lock: setReferenceCounted failed: {e}");
            return None;
        }

        if let Err(e) = env.call_method(&lock, "acquire", "()V", &[]) {
            log::error!("multicast lock: acquire failed: {e}");
            return None;
        }

        log::info!("multicast lock acquired");
        env.new_global_ref(lock).ok()
    }
}
