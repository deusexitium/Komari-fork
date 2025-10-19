use dioxus::prelude::*;
use tw_merge::tw_merge;

const DIV_CLASS: &str = "inline-block h-6";

const CLASS: &str = "size-full text-xs text-center font-medium px-2 bg-secondary-surface enabled:hover:bg-tertiary-surface disabled:cursor-not-allowed disabled:text-tertiary-text";

#[derive(Copy, Clone, PartialEq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    OutlinePrimary,
    OutlineSecondary,
    Danger,
}

#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    #[props(default)]
    on_click: Callback,
    #[props(default)]
    disabled: ReadOnlySignal<bool>,
    #[props(default = ButtonStyle::Primary)]
    style: ButtonStyle,
    #[props(default)]
    class: String,
    children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let class = props.class;
    let text_class = match props.style {
        ButtonStyle::Primary | ButtonStyle::OutlinePrimary => "text-primary-text",
        ButtonStyle::Secondary | ButtonStyle::OutlineSecondary => "text-secondary-text",
        ButtonStyle::Danger => "text-danger-text",
    };
    let border_class = match props.style {
        ButtonStyle::Danger | ButtonStyle::Primary | ButtonStyle::Secondary => "",
        ButtonStyle::OutlinePrimary | ButtonStyle::OutlineSecondary => {
            "border border-primary-border"
        }
    };

    rsx! {
        div { class: tw_merge!(DIV_CLASS, class),
            button {
                class: "{CLASS} {text_class} {border_class}",
                disabled: props.disabled,
                onclick: move |_| {
                    props.on_click.call(());
                },
                {props.children}
            }
        }
    }
}
