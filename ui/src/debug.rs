use backend::{
    DebugState, auto_save_rune, capture_image, debug_state_receiver, infer_minimap, infer_rune,
    record_images, test_spin_rune,
};
use dioxus::prelude::*;
use tokio::sync::broadcast::error::RecvError;

use crate::components::{
    button::{Button, ButtonStyle},
    section::Section,
};

#[component]
pub fn DebugScreen() -> Element {
    let mut state = use_signal(DebugState::default);

    use_future(move || async move {
        let mut rx = debug_state_receiver().await;
        loop {
            let current_state = match rx.recv().await {
                Ok(state) => state,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => continue,
            };
            if current_state != *state.peek() {
                state.set(current_state);
            }
        }
    });

    rsx! {
        div { class: "flex flex-col h-full overflow-y-auto",
            Section { title: "Debug",
                div { class: "grid grid-cols-2 gap-3",
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async {
                            capture_image(false).await;
                        },

                        "Capture color image"
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async {
                            capture_image(true).await;
                        },

                        "Capture grayscale image"
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async {
                            infer_rune().await;
                        },

                        "Infer rune"
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async {
                            infer_minimap().await;
                        },

                        "Infer minimap"
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async {
                            test_spin_rune().await;
                        },

                        "Spin rune sandbox test"
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async move {
                            record_images(!state.peek().is_recording).await;
                        },

                        if state().is_recording {
                            "Stop recording"
                        } else {
                            "Start recording"
                        }
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async move {
                            auto_save_rune(!state.peek().is_rune_auto_saving).await;
                        },

                        if state().is_rune_auto_saving {
                            "Stop auto saving rune"
                        } else {
                            "Start auto saving rune"
                        }
                    }
                }
            }
        }
    }
}
