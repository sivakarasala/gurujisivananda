use leptos::prelude::*;

const TOAST_TIMEOUT_MS: u64 = 4000;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToastId(usize);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastVariant {
    Success,
    Error,
}

impl ToastVariant {
    fn css_class(&self) -> &'static str {
        match self {
            ToastVariant::Success => "toast-success",
            ToastVariant::Error => "toast-error",
        }
    }
}

#[derive(Clone)]
pub struct Toast {
    pub id: ToastId,
    pub message: String,
    pub variant: ToastVariant,
}

#[derive(Clone, Copy)]
pub struct ToastState {
    toasts: RwSignal<Vec<Toast>>,
    next_id: RwSignal<usize>,
}

impl ToastState {
    fn new() -> Self {
        Self {
            toasts: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    fn next_id(&self) -> ToastId {
        let id = self.next_id.get_untracked();
        self.next_id.set(id.wrapping_add(1));
        ToastId(id)
    }

    pub fn success(&self, message: impl Into<String>) {
        let id = self.next_id();
        self.toasts.update(|list| {
            list.push(Toast {
                id,
                message: message.into(),
                variant: ToastVariant::Success,
            });
        });
    }

    pub fn error(&self, message: impl Into<String>) {
        let id = self.next_id();
        self.toasts.update(|list| {
            list.push(Toast {
                id,
                message: message.into(),
                variant: ToastVariant::Error,
            });
        });
    }

    pub fn dismiss(&self, id: ToastId) {
        self.toasts.update(|list| list.retain(|t| t.id != id));
    }
}

pub fn use_toast() -> ToastState {
    expect_context::<ToastState>()
}

#[component]
pub fn ToastProvider(children: Children) -> impl IntoView {
    provide_context(ToastState::new());

    view! {
        {children()}
        <ToastContainer/>
    }
}

#[component]
fn ToastContainer() -> impl IntoView {
    let state = use_toast();

    view! {
        <div class="toast-container">
            <For
                each=move || state.toasts.get()
                key=|toast| toast.id
                children=move |toast| {
                    view! { <ToastItem toast=toast/> }
                }
            />
        </div>
    }
}

#[component]
fn ToastItem(toast: Toast) -> impl IntoView {
    let state = use_toast();
    let id = toast.id;

    set_timeout(
        move || state.dismiss(id),
        std::time::Duration::from_millis(TOAST_TIMEOUT_MS),
    );

    let class = format!("toast {}", toast.variant.css_class());

    view! {
        <div class=class>
            <span class="toast-message">{toast.message.clone()}</span>
            <button
                class="toast-close"
                aria-label="Dismiss"
                on:click=move |_| state.dismiss(id)
            >
                "\u{00D7}"
            </button>
        </div>
    }
}
