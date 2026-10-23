//! Android: reqwest checks server certificates with Android's own verifier
//! (`rustls-platform-verifier`), which must be given the JVM and the app context
//! before the first HTTPS request, or it panics. Its Kotlin half comes from the
//! local Maven repository wired up in `gen/android/app/build.gradle.kts`.

use std::time::Duration;

use jni::objects::JObject;
use jni::{jni_sig, jni_str, JavaVM};
use tao::platform::android::prelude::main_android_context;

/// Initialises the verifier now if tao already knows the activity, otherwise as
/// soon as it does (polled for up to 5 s).
pub fn init() {
    if try_init() {
        return;
    }
    std::thread::spawn(|| {
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(50));
            if try_init() {
                return;
            }
        }
        log::error!("TLS verifier not initialised: no Android activity");
    });
}

fn try_init() -> bool {
    let Some(ctx) = main_android_context() else {
        return false;
    };
    // SAFETY: tao stores a valid JavaVM pointer and a global reference to the activity.
    let vm = unsafe { JavaVM::from_raw(ctx.java_vm.cast()) };
    let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, ctx.context_jobject.cast()) };
        let app = env
            .call_method(
                &activity,
                jni_str!("getApplicationContext"),
                jni_sig!(() -> android.content.Context),
                &[],
            )?
            .l()?;
        std::mem::forget(activity); // tao owns that global reference
        rustls_platform_verifier::android::init_with_env(env, app)
    });
    if let Err(e) = res {
        log::error!("TLS verifier init failed: {e}");
    }
    true
}
