use crate::components::use_toast;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Trigger signal to refresh storage indicator after save/remove.
#[cfg(feature = "hydrate")]
fn refresh_storage(storage_used: RwSignal<Option<(f64, f64)>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let estimate = crate::pwa::get_storage_estimate().await;
        storage_used.set(estimate);
    });
}

fn format_bytes(bytes: f64) -> String {
    if bytes >= 1_073_741_824.0 {
        format!("{:.1} GB", bytes / 1_073_741_824.0)
    } else {
        format!("{:.0} MB", bytes / 1_048_576.0)
    }
}

const PAGE_SIZE: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrackResult {
    pub id: String,
    pub title: String,
    pub thumbnail_url: String,
    pub channel: String,
    pub duration: String,
}

#[cfg(feature = "ssr")]
fn format_duration(seconds: i32) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{}:{:02}", m, s)
}

#[server]
pub async fn search_tracks(query: String, offset: i64) -> Result<Vec<TrackResult>, ServerFnError> {
    let pool = crate::db::db().await?;
    let limit = PAGE_SIZE as i64;

    let rows = if query.trim().is_empty() {
        crate::db::list_tracks(&pool, limit, offset).await
    } else {
        crate::db::search_tracks(&pool, &query, limit, offset).await
    }
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| TrackResult {
            id: r.id.to_string(),
            title: r.title,
            thumbnail_url: r.thumbnail_url,
            channel: r.channel,
            duration: format_duration(r.duration_seconds),
        })
        .collect())
}

#[component]
pub fn GurujiPage() -> impl IntoView {
    let toast = use_toast();

    let query = RwSignal::new(String::new());
    let search_results = RwSignal::new(Vec::<TrackResult>::new());
    let audio_src = RwSignal::new(Option::<String>::None);
    let now_playing = RwSignal::new(Option::<TrackResult>::None);
    let offset = RwSignal::new(0i64);
    let has_more = RwSignal::new(true);
    let loading_more = RwSignal::new(false);
    let search_version = RwSignal::new(0u32);

    let search_action = ServerAction::<SearchTracks>::new();
    let search_pending = search_action.pending();
    let search_result_value = search_action.value();

    // Load recent tracks on page mount
    Effect::new(move || {
        search_action.dispatch(SearchTracks {
            query: String::new(),
            offset: 0,
        });
    });

    // Handle search action results (replaces results for new searches)
    Effect::new(move || {
        if let Some(response) = search_result_value.get() {
            match response {
                Ok(results) => {
                    has_more.set(results.len() >= PAGE_SIZE);
                    offset.set(results.len() as i64);
                    search_results.set(results);
                }
                Err(e) => {
                    toast.error(format!("Search failed: {}", e));
                    search_results.set(vec![]);
                    has_more.set(false);
                }
            }
        }
    });

    // Infinite scroll: load more when user scrolls near bottom
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let closure = Closure::wrap(Box::new(move || {
            if loading_more.get_untracked()
                || !has_more.get_untracked()
                || search_pending.get_untracked()
            {
                return;
            }

            let window = leptos::web_sys::window().unwrap();
            let doc = window.document().unwrap().document_element().unwrap();
            let scroll_top = doc.scroll_top() as f64;
            let scroll_height = doc.scroll_height() as f64;
            let client_height = doc.client_height() as f64;

            if scroll_height - scroll_top - client_height < 400.0 {
                loading_more.set(true);
                let q = query.get_untracked();
                let off = offset.get_untracked();
                let version = search_version.get_untracked();

                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(results) = search_tracks(q, off).await {
                        // Discard if a new search started while loading
                        if search_version.get_untracked() != version {
                            loading_more.set(false);
                            return;
                        }
                        has_more.set(results.len() >= PAGE_SIZE);
                        offset.update(|o| *o += results.len() as i64);
                        search_results.update(|v| v.extend(results));
                    }
                    loading_more.set(false);
                });
            }
        }) as Box<dyn FnMut()>);

        let window = leptos::web_sys::window().unwrap();
        let _ = window.add_event_listener_with_callback("scroll", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    let on_play = move |result: TrackResult| {
        let url = format!("/api/v1/tracks/{}/stream", result.id);
        now_playing.set(Some(result));
        audio_src.set(Some(url));
    };

    // Storage estimate for the indicator
    let storage_used = RwSignal::new(Option::<(f64, f64)>::None);

    // Fetch initial storage estimate on mount (hydrate only)
    #[cfg(feature = "hydrate")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            let estimate = crate::pwa::get_storage_estimate().await;
            storage_used.set(estimate);
        });
    }

    let on_search = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let q = query.get_untracked();
        has_more.set(true);
        search_version.update(|v| *v += 1);
        search_action.dispatch(SearchTracks {
            query: q,
            offset: 0,
        });
    };

    view! {
        <div class="guruji">
            <form class="search-form" on:submit=on_search>
                <input
                    type="text"
                    placeholder="Search tracks..."
                    prop:value=move || query.get()
                    on:input=move |e| query.set(event_target_value(&e))
                />
                <button type="submit" disabled=move || search_pending.get()>
                    {move || if search_pending.get() { "SEARCHING..." } else { "SEARCH" }}
                </button>
            </form>

            {move || {
                storage_used.get().map(|(used, quota)| {
                    let pct = if quota > 0.0 { (used / quota) * 100.0 } else { 0.0 };
                    view! {
                        <div class="storage-indicator">
                            <div class="storage-bar">
                                <div class="storage-fill" style=format!("width: {:.1}%", pct.min(100.0))></div>
                            </div>
                            <span class="storage-text">
                                {format!("Offline storage: {} / {}", format_bytes(used), format_bytes(quota))}
                            </span>
                        </div>
                    }
                })
            }}

            <div class="results">
                <For
                    each=move || search_results.get()
                    key=|result| result.id.clone()
                    children=move |result| {
                        let result_for_play = result.clone();
                        #[cfg(feature = "hydrate")]
                        let result_for_save = result.clone();
                        let download_url = format!("/api/v1/tracks/{}/download", result.id);
                        let is_current = {
                            let tid = result.id.clone();
                            Memo::new(move |_| {
                                now_playing.get()
                                    .as_ref()
                                    .map(|np| np.id == tid)
                                    .unwrap_or(false)
                            })
                        };

                        let is_saved_offline = RwSignal::new(false);
                        let save_pending = RwSignal::new(false);

                        // Check cache status on mount (hydrate only)
                        #[cfg(feature = "hydrate")]
                        {
                            let track_id = result.id.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let cached = crate::pwa::is_track_cached(&track_id).await;
                                is_saved_offline.set(cached);
                            });
                        }

                        let on_save_toggle = move |_| {
                            #[cfg(feature = "hydrate")]
                            {
                                let track_id = result_for_save.id.clone();
                                let currently_saved = is_saved_offline.get_untracked();
                                save_pending.set(true);

                                wasm_bindgen_futures::spawn_local(async move {
                                    let result = if currently_saved {
                                        crate::pwa::remove_track_offline(&track_id).await
                                    } else {
                                        crate::pwa::save_track_offline(&track_id).await
                                    };

                                    match result {
                                        Ok(()) => {
                                            is_saved_offline.set(!currently_saved);
                                            if !currently_saved {
                                                toast.success("Track saved for offline listening".to_string());
                                            } else {
                                                toast.success("Track removed from offline storage".to_string());
                                            }
                                            refresh_storage(storage_used);
                                        }
                                        Err(e) => {
                                            toast.error(format!("Offline storage error: {}", e));
                                        }
                                    }
                                    save_pending.set(false);
                                });
                            }
                        };

                        view! {
                            <div class="result-card" class:active=move || is_current.get()>
                                <img
                                    src=result.thumbnail_url.clone()
                                    alt=result.title.clone()
                                    class="thumbnail"
                                />
                                <div class="result-info">
                                    <span class="result-title">{result.title.clone()}</span>
                                    <span class="result-channel">{result.channel.clone()}</span>
                                    <span class="result-duration">{result.duration.clone()}</span>
                                </div>
                                <div class="result-actions">
                                    <button
                                        class="play-btn"
                                        on:click=move |_| on_play(result_for_play.clone())
                                    >
                                        {move || {
                                            if is_current.get() {
                                                "Playing"
                                            } else {
                                                "Play"
                                            }
                                        }}
                                    </button>
                                    <button
                                        class="save-offline-btn"
                                        class:saved=move || is_saved_offline.get()
                                        disabled=move || save_pending.get()
                                        on:click=on_save_toggle
                                    >
                                        {move || {
                                            if save_pending.get() {
                                                "Saving..."
                                            } else if is_saved_offline.get() {
                                                "Saved"
                                            } else {
                                                "Save Offline"
                                            }
                                        }}
                                    </button>
                                    <a
                                        class="download-btn"
                                        href=download_url.clone()
                                        download=""
                                    >
                                        "Download"
                                    </a>
                                </div>
                            </div>
                        }
                    }
                />

                {move || {
                    loading_more.get().then(|| {
                        view! {
                            <div class="loading-more">
                                <span class="loading-spinner"></span>
                                " Loading more tracks..."
                            </div>
                        }
                    })
                }}

                {move || {
                    (!has_more.get() && search_results.with(|r| !r.is_empty())).then(|| {
                        view! {
                            <div class="end-of-results">
                                "All tracks loaded"
                            </div>
                        }
                    })
                }}
            </div>

            {move || {
                now_playing.get().map(|track| {
                    view! {
                        <div class="audio-player">
                            <div class="player-info">
                                <span class="player-title">{track.title.clone()}</span>
                                <span class="player-channel">{track.channel.clone()}</span>
                            </div>
                            {move || {
                                audio_src.get().map(|src| {
                                    view! {
                                        <audio controls autoplay src=src />
                                    }
                                })
                            }}
                        </div>
                    }
                })
            }}
        </div>
    }
}
