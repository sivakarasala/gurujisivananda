use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Register the service worker from the client.
pub fn register_service_worker() {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .unwrap_or(JsValue::UNDEFINED);

    let sw_container = js_sys::Reflect::get(&navigator, &JsValue::from_str("serviceWorker"))
        .unwrap_or(JsValue::UNDEFINED);
    if sw_container.is_undefined() {
        return;
    }

    wasm_bindgen_futures::spawn_local(async move {
        let register_fn = match js_sys::Reflect::get(&sw_container, &JsValue::from_str("register"))
        {
            Ok(f) if f.is_function() => f.unchecked_into::<js_sys::Function>(),
            _ => return,
        };

        let promise = match register_fn.call1(&sw_container, &JsValue::from_str("/sw.js")) {
            Ok(p) => p.unchecked_into::<js_sys::Promise>(),
            Err(_) => return,
        };

        match JsFuture::from(promise).await {
            Ok(_) => {
                let _ = log_to_console("Service worker registered");
            }
            Err(_) => {
                let _ = log_to_console("Service worker registration failed");
            }
        }
    });
}

/// Send a message to the service worker to cache an audio track for offline playback.
pub async fn save_track_offline(track_id: &str) -> Result<(), String> {
    let controller = get_sw_controller()?;

    let msg = js_sys::Object::new();
    js_sys::Reflect::set(&msg, &"type".into(), &"SAVE_TRACK_OFFLINE".into())
        .map_err(|_| "Failed to build message")?;
    js_sys::Reflect::set(
        &msg,
        &"url".into(),
        &format!("/api/v1/tracks/{}/stream", track_id).into(),
    )
    .map_err(|_| "Failed to build message")?;

    let post_message = js_sys::Reflect::get(&controller, &JsValue::from_str("postMessage"))
        .map_err(|_| "No postMessage on controller")?
        .unchecked_into::<js_sys::Function>();
    post_message
        .call1(&controller, &msg)
        .map_err(|_| "postMessage failed")?;

    Ok(())
}

/// Send a message to the service worker to remove a cached audio track.
pub async fn remove_track_offline(track_id: &str) -> Result<(), String> {
    let controller = get_sw_controller()?;

    let msg = js_sys::Object::new();
    js_sys::Reflect::set(&msg, &"type".into(), &"REMOVE_TRACK_OFFLINE".into())
        .map_err(|_| "Failed to build message")?;
    js_sys::Reflect::set(
        &msg,
        &"url".into(),
        &format!("/api/v1/tracks/{}/stream", track_id).into(),
    )
    .map_err(|_| "Failed to build message")?;

    let post_message = js_sys::Reflect::get(&controller, &JsValue::from_str("postMessage"))
        .map_err(|_| "No postMessage on controller")?
        .unchecked_into::<js_sys::Function>();
    post_message
        .call1(&controller, &msg)
        .map_err(|_| "postMessage failed")?;

    Ok(())
}

/// Check if a track's audio is cached for offline playback using the Cache API directly.
pub async fn is_track_cached(track_id: &str) -> bool {
    let global = js_sys::global();
    let caches = match js_sys::Reflect::get(&global, &JsValue::from_str("caches")) {
        Ok(c) if !c.is_undefined() => c,
        _ => return false,
    };

    let open_fn = match js_sys::Reflect::get(&caches, &JsValue::from_str("open")) {
        Ok(f) if f.is_function() => f.unchecked_into::<js_sys::Function>(),
        _ => return false,
    };

    let cache_promise = match open_fn.call1(&caches, &"gurujisivananda-audio".into()) {
        Ok(p) => p.unchecked_into::<js_sys::Promise>(),
        Err(_) => return false,
    };

    let cache = match JsFuture::from(cache_promise).await {
        Ok(c) => c,
        Err(_) => return false,
    };

    let match_fn = match js_sys::Reflect::get(&cache, &JsValue::from_str("match")) {
        Ok(f) if f.is_function() => f.unchecked_into::<js_sys::Function>(),
        _ => return false,
    };

    let url = format!("/api/v1/tracks/{}/stream", track_id);
    let match_promise = match match_fn.call1(&cache, &url.into()) {
        Ok(p) => p.unchecked_into::<js_sys::Promise>(),
        Err(_) => return false,
    };

    match JsFuture::from(match_promise).await {
        Ok(resp) => !resp.is_undefined(),
        Err(_) => false,
    }
}

/// Get storage usage estimate via navigator.storage.estimate().
/// Returns (used_bytes, quota_bytes).
pub async fn get_storage_estimate() -> Option<(f64, f64)> {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")).ok()?;
    let storage = js_sys::Reflect::get(&navigator, &JsValue::from_str("storage")).ok()?;

    if storage.is_undefined() {
        return None;
    }

    let estimate_fn = js_sys::Reflect::get(&storage, &JsValue::from_str("estimate"))
        .ok()?
        .unchecked_into::<js_sys::Function>();

    let promise = estimate_fn
        .call0(&storage)
        .ok()?
        .unchecked_into::<js_sys::Promise>();

    let result = JsFuture::from(promise).await.ok()?;

    let usage = js_sys::Reflect::get(&result, &JsValue::from_str("usage"))
        .ok()?
        .as_f64()?;
    let quota = js_sys::Reflect::get(&result, &JsValue::from_str("quota"))
        .ok()?
        .as_f64()?;

    Some((usage, quota))
}

fn log_to_console(msg: &str) -> Result<(), JsValue> {
    let global = js_sys::global();
    let console = js_sys::Reflect::get(&global, &JsValue::from_str("console"))?;
    let log_fn = js_sys::Reflect::get(&console, &JsValue::from_str("log"))?
        .unchecked_into::<js_sys::Function>();
    log_fn.call1(&console, &JsValue::from_str(msg))?;
    Ok(())
}

fn get_sw_controller() -> Result<JsValue, String> {
    let global = js_sys::global();
    let navigator = js_sys::Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|_| "No navigator")?;
    let sw_container = js_sys::Reflect::get(&navigator, &JsValue::from_str("serviceWorker"))
        .map_err(|_| "No serviceWorker")?;

    if sw_container.is_undefined() {
        return Err("Service workers not supported".into());
    }

    let controller = js_sys::Reflect::get(&sw_container, &JsValue::from_str("controller"))
        .map_err(|_| "No controller")?;

    if controller.is_null() || controller.is_undefined() {
        return Err("No active service worker controller".into());
    }

    Ok(controller)
}
