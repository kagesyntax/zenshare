//! Android entry point.
//!
//! Android has no `main()`. The OS loads this shared library into an Activity
//! and calls `android_main`, so this crate is a `cdylib` rather than a binary.
//! Everything below it — engine, transfer, UI — is the same code the desktop
//! runs; only the process lifecycle and two Android-specific details differ.

#![deny(missing_docs)]
// JNI is unsafe by nature: we hand raw pointers to the JVM and call methods by
// name and signature. The unsafety is confined to `acquire_multicast_lock`.
#![allow(unsafe_code)]

#[cfg(target_os = "android")]
mod android {
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

    /// Called by `android-activity` once the Activity is ready.
    ///
    /// `#[unsafe(no_mangle)]` is required in edition 2024; the symbol name is
    /// what the loader looks up with `dlsym`.
    #[unsafe(no_mangle)]
    fn android_main(app: AndroidApp) {
        // Held for the lifetime of the process, which is what the lock wants.
        let _ = MULTICAST_LOCK.set(acquire_multicast_lock(&app));

        // Android has no $HOME. Internal storage is private and always writable;
        // external storage is app-scoped but visible over USB, which is where
        // received files should land so the user can actually find them.
        let internal = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
        let external = app.external_data_path();

        let paths = Arc::new(AppPaths::android(internal, external));
        let _ = paths.ensure();

        let model = Arc::new(RwLock::new(UiModel::default()));
        let (tx, rx) = async_channel::unbounded();

        let _ = engine::spawn(
            Arc::clone(&model),
            rx,
            Ipv4Addr::UNSPECIFIED,
            Arc::clone(&paths),
        );

        let mut options = eframe::NativeOptions::default();
        options.android_app = Some(app);
        options.viewport = eframe::egui::ViewportBuilder::default().with_title("ZenShare");

        let _ = eframe::run_native(
            "ZenShare",
            options,
            Box::new(move |cc| Ok(zs_ui::make_app(cc, model, tx, AppOptions::default()))),
        );
    }

    /// Acquire the multicast lock, returning it so it is never dropped.
    ///
    /// Every step is best-effort: if any JNI call fails we return `None` and the
    /// app runs without discovery rather than refusing to start. A phone that
    /// cannot find peers is annoying; a phone that crashes on launch is broken.
    fn acquire_multicast_lock(app: &AndroidApp) -> Option<GlobalRef> {
        // SAFETY: both pointers are owned by android-activity and remain valid
        // for the lifetime of the process.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;

        // `activity_as_ptr` hands back a pointer to the reference, not the
        // reference itself, so this is one dereference deep.
        // SAFETY: the Activity reference is valid while android_main is running.
        let activity_ref: *mut jobject = app.activity_as_ptr().cast();
        let activity = unsafe { JObject::from_raw(*activity_ref) };

        let service = env.new_string("wifi").ok()?;
        let wifi = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&JObject::from(service))],
            )
            .ok()?
            .l()
            .ok()?;

        let tag = env.new_string("zenshare").ok()?;
        let lock = env
            .call_method(
                &wifi,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&JObject::from(tag))],
            )
            .ok()?
            .l()
            .ok()?;

        // Reference-counted locks can be released by anything; a single
        // acquire/release pair is easier to reason about.
        env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(0)])
            .ok()?;
        env.call_method(&lock, "acquire", "()V", &[]).ok()?;

        env.new_global_ref(lock).ok()
    }
}
