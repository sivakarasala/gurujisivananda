use crate::components::ToastProvider;
use crate::pages::login::get_current_user;
use crate::pages::{AdminPage, GurujiPage, LoginPage};
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes, A},
    StaticSegment,
};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="icon" type="image/x-icon" href="/favicon.ico"/>
                <link rel="manifest" href="/manifest.json"/>
                <meta name="theme-color" content="#3a1f0d"/>
                <meta name="apple-mobile-web-app-capable" content="yes"/>
                <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent"/>
                <meta name="apple-mobile-web-app-title" content="Guruji Audio"/>
                <link rel="apple-touch-icon" href="/guruji.jpg"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/gurujisivananda.css"/>
        <Title text="Guruji Sivananda"/>

        <ToastProvider>
            <Router>
                <Header/>
                <main>
                    <Routes fallback=|| "Page not found.".into_view()>
                        <Route path=StaticSegment("") view=GurujiPage/>
                        <Route path=StaticSegment("login") view=LoginPage/>
                        <Route path=StaticSegment("admin") view=AdminPage/>
                    </Routes>
                </main>
            </Router>
        </ToastProvider>
    }
}

#[component]
fn Header() -> impl IntoView {
    let is_muted = RwSignal::new(true);
    let audio_ref = NodeRef::<leptos::html::Audio>::new();

    let current_user = Resource::new(|| (), |_| get_current_user());

    let logout_action = ServerAction::<crate::pages::login::Logout>::new();
    let logout_value = logout_action.value();

    Effect::new(move || {
        if let Some(Ok(())) = logout_value.get() {
            #[cfg(feature = "hydrate")]
            {
                let window = leptos::web_sys::window().unwrap();
                let _ = window.location().set_href("/");
            }
        }
    });

    let toggle_audio = move |_| {
        is_muted.update(|m| *m = !*m);
        if let Some(audio) = audio_ref.get() {
            let muted = is_muted.get_untracked();
            audio.set_muted(muted);
            if !muted {
                let _ = audio.play();
            }
        }
    };

    view! {
        <header>
            <nav>
                <A href="/" attr:class="logo">
                    <img
                        class="logo-avatar"
                        src="/guruji.jpg"
                        alt="Guruji Sivananda"
                    />
                    "Om Sri Sathguru Sivananda Murthaye Namaha"
                </A>
                <div class="nav-right">
                    <Suspense fallback=|| ()>
                        {move || {
                            current_user.get().map(|result| {
                                match result {
                                    Ok(Some(user)) if user.role == "admin" => {
                                        view! {
                                            <div class="nav-auth">
                                                <A href="/admin" attr:class="nav-link">"Admin"</A>
                                                <button
                                                    class="nav-link logout-btn"
                                                    on:click=move |_| {
                                                        logout_action.dispatch(crate::pages::login::Logout {});
                                                    }
                                                >
                                                    "Logout"
                                                </button>
                                            </div>
                                        }.into_any()
                                    }
                                    _ => {
                                        view! { <span></span> }.into_any()
                                    }
                                }
                            })
                        }}
                    </Suspense>
                    <button class="audio-toggle" on:click=toggle_audio>
                        {move || if is_muted.get() { "\u{1F507}" } else { "\u{1F50A}" }}
                    </button>
                </div>
            </nav>
            <audio
                node_ref=audio_ref
                src="/bg-music.mp3"
                autoplay
                loop
                muted
            />
        </header>
    }
}
