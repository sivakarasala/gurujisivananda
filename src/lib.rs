pub mod app;
#[cfg(feature = "ssr")]
pub mod auth;
pub mod components;
#[cfg(feature = "ssr")]
pub mod configuration;
pub mod db;
#[cfg(feature = "ssr")]
pub mod import;
#[cfg(feature = "ssr")]
pub mod jobs;
pub mod pages;
#[cfg(feature = "hydrate")]
pub mod pwa;
#[cfg(feature = "ssr")]
pub mod routes;
#[cfg(feature = "ssr")]
pub mod storage;
#[cfg(feature = "ssr")]
pub mod telemetry;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    pwa::register_service_worker();
    leptos::mount::hydrate_body(App);
}
