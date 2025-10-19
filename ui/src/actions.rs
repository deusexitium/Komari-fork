use std::{
    fmt::Display,
    fs::File,
    io::BufReader,
    mem::{discriminant, swap},
    ops::Range,
};

use backend::{
    Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, Bound,
    IntoEnumIterator, KeyBinding, LinkKeyBinding, Minimap, MobbingKey, Platform, Position,
    RotationMode, key_receiver, update_minimap, upsert_minimap,
};
use dioxus::prelude::*;
use futures_util::StreamExt;
use rand::distr::{Alphanumeric, SampleString};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    AppState,
    components::{
        ContentAlign, ContentSide,
        button::{Button, ButtonStyle},
        checkbox::Checkbox,
        icons::{DownArrowIcon, UpArrowIcon, XIcon},
        key::KeyInput,
        labeled::Labeled,
        named_select::NamedSelect,
        numbers::{MillisInput, PrimitiveIntegerInput},
        popup::{PopupContent, PopupContext, PopupTrigger},
        position::PositionInput,
        section::Section,
        select::{Select, SelectOption},
    },
};

const ITEM_TEXT_CLASS: &str =
    "text-center inline-block pt-1 text-ellipsis overflow-hidden whitespace-nowrap";
const ITEM_BORDER_CLASS: &str = "border-r-2 border-secondary-border";

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum ActionsUpdate {
    Set,
    Create(String),
    Delete,
    Update(Vec<Action>),
    UpdateMinimap(Minimap),
}

#[derive(PartialEq, Copy, Clone)]
struct ActionsContext {
    minimap: Memo<Minimap>,
    save_minimap: Callback<Minimap>,
}

#[component]
pub fn ActionsScreen() -> Element {
    let mut minimap = use_context::<AppState>().minimap;
    let mut minimap_preset = use_context::<AppState>().minimap_preset;
    // Non-null view of minimap
    let minimap_view = use_memo(move || minimap().unwrap_or_default());
    // Maps currently selected `minimap` to presets
    let minimap_presets = use_memo(move || {
        minimap()
            .map(|minimap| minimap.actions.into_keys().collect::<Vec<String>>())
            .unwrap_or_default()
    });
    // Maps currently selected `minimap_preset` to actions
    let minimap_preset_actions = use_memo(move || {
        minimap()
            .zip(minimap_preset())
            .and_then(|(minimap, preset)| minimap.actions.get(&preset).cloned())
            .unwrap_or_default()
    });
    // Maps currently selected `minimap_preset` to the index in `minimap_presets`
    let minimap_preset_index = use_memo(move || {
        let presets = minimap_presets();
        minimap_preset().and_then(|preset| {
            presets
                .into_iter()
                .enumerate()
                .find(|(_, p)| &preset == p)
                .map(|(i, _)| i)
        })
    });

    // Handles async operations for action-related
    let coroutine = use_coroutine(move |mut rx: UnboundedReceiver<ActionsUpdate>| async move {
        while let Some(message) = rx.next().await {
            match message {
                ActionsUpdate::Set => {
                    update_minimap(minimap_preset(), minimap()).await;
                }
                ActionsUpdate::Create(preset) => {
                    let Some(mut current_minimap) = minimap() else {
                        continue;
                    };
                    if current_minimap
                        .actions
                        .try_insert(preset.clone(), vec![])
                        .is_err()
                    {
                        continue;
                    }
                    if let Some(current_minimap) = upsert_minimap(current_minimap).await {
                        minimap_preset.set(Some(preset));
                        minimap.set(Some(current_minimap));
                        update_minimap(minimap_preset(), minimap()).await;
                    }
                }
                ActionsUpdate::Delete => {
                    let Some(mut current_minimap) = minimap() else {
                        continue;
                    };
                    let Some(preset) = minimap_preset() else {
                        continue;
                    };

                    if current_minimap.actions.remove(&preset).is_none() {
                        continue;
                    }
                    if let Some(current_minimap) = upsert_minimap(current_minimap).await {
                        minimap_preset.set(current_minimap.actions.keys().next().cloned());
                        minimap.set(Some(current_minimap));
                        update_minimap(minimap_preset(), minimap()).await;
                    }
                }
                ActionsUpdate::Update(actions) => {
                    let Some(mut current_minimap) = minimap() else {
                        continue;
                    };
                    let Some(preset) = minimap_preset() else {
                        continue;
                    };

                    current_minimap.actions.insert(preset, actions);
                    if let Some(current_minimap) = upsert_minimap(current_minimap).await {
                        minimap.set(Some(current_minimap));
                    }
                }
                ActionsUpdate::UpdateMinimap(new_minimap) => {
                    if let Some(new_minimap) = upsert_minimap(new_minimap).await {
                        minimap.set(Some(new_minimap));
                    }
                }
            }
        }
    });

    let save_minimap = use_callback(move |minimap: Minimap| {
        coroutine.send(ActionsUpdate::UpdateMinimap(minimap));
    });
    let select_preset = use_callback(move |index: usize| {
        let selected = minimap_presets.peek().get(index).cloned().unwrap();

        minimap_preset.set(Some(selected));
        coroutine.send(ActionsUpdate::Set);
    });

    use_context_provider(|| ActionsContext {
        minimap: minimap_view,
        save_minimap,
    });

    rsx! {
        div { class: "flex flex-col pb-15 h-full gap-3 overflow-y-auto pr-2",
            SectionRotation { disabled: minimap().is_none() }
            SectionPlatforms { disabled: minimap().is_none() }
            SectionActions {
                actions: minimap_preset_actions,
                disabled: minimap().is_none() || minimap_preset().is_none(),
            }
            SectionLegends {}
        }

        div { class: "flex items-center w-full h-10 pr-2 bg-primary-surface absolute bottom-0",
            NamedSelect {
                class: "flex-grow",
                on_create: move |name| {
                    coroutine.send(ActionsUpdate::Create(name));
                },
                on_delete: move |_| {
                    coroutine.send(ActionsUpdate::Delete);
                },
                disabled: minimap().is_none(),
                delete_disabled: minimap_presets().is_empty(),

                Select::<usize> {
                    class: "w-full",
                    placeholder: "Create an actions preset for the selected map...",
                    disabled: minimap_presets().is_empty(),
                    on_selected: select_preset,

                    for (i , name) in minimap_presets().into_iter().enumerate() {
                        SelectOption::<usize> {
                            value: i,
                            selected: minimap_preset_index() == Some(i),
                            label: name,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SectionRotation(disabled: bool) -> Element {
    #[derive(Clone, Copy, PartialEq)]
    enum PopupContent {
        None,
        Bound(Bound),
        Key(MobbingKey),
    }

    let context = use_context::<ActionsContext>();
    let minimap = context.minimap;
    let save_minimap = context.save_minimap;

    let update_mobbing_button_disabled = use_memo(move || {
        !matches!(
            minimap().rotation_mode,
            RotationMode::AutoMobbing | RotationMode::PingPong
        )
    });

    let edit_mobbing_key = use_callback(move |rotation_mobbing_key| {
        save_minimap(Minimap {
            rotation_mobbing_key,
            ..minimap()
        });
    });

    let edit_mobbing_bound = use_callback(move |bound| {
        let mut minimap = minimap();

        match minimap.rotation_mode {
            RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => return,
            RotationMode::AutoMobbing => {
                minimap.rotation_auto_mob_bound = bound;
            }
            RotationMode::PingPong => {
                minimap.rotation_ping_pong_bound = bound;
            }
        };
        save_minimap(minimap);
    });

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open: bool| {
                popup_open.set(open);
            },
            Section { title: "Rotation",
                div { class: "grid grid-cols-2 gap-3",
                    ActionsSelect::<RotationMode> {
                        label: "Mode",
                        disabled,
                        on_selected: move |rotation_mode| {
                            save_minimap(Minimap {
                                rotation_mode,
                                ..minimap.peek().clone()
                            })
                        },
                        selected: minimap().rotation_mode,
                    }
                    div {}
                    PopupTrigger {
                        Button {
                            style: ButtonStyle::Primary,
                            class: "w-full",
                            disabled: disabled | update_mobbing_button_disabled(),
                            on_click: move |_| {
                                let minimap = minimap.peek();
                                let key = match minimap.rotation_mode {
                                    RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => {
                                        unreachable!()
                                    }
                                    RotationMode::AutoMobbing | RotationMode::PingPong => {
                                        minimap.rotation_mobbing_key
                                    }
                                };
                                popup_content.set(PopupContent::Key(key));
                            },

                            "Update mobbing key"
                        }
                    }
                    PopupTrigger {
                        Button {
                            style: ButtonStyle::Primary,
                            class: "w-full",
                            disabled: disabled || update_mobbing_button_disabled(),
                            on_click: move |_| {
                                let minimap = minimap.peek();
                                let bound = match minimap.rotation_mode {
                                    RotationMode::StartToEnd | RotationMode::StartToEndThenReverse => {
                                        unreachable!()
                                    }
                                    RotationMode::AutoMobbing => minimap.rotation_auto_mob_bound,
                                    RotationMode::PingPong => minimap.rotation_ping_pong_bound,
                                };
                                popup_content.set(PopupContent::Bound(bound));
                            },

                            "Update mobbing bound"
                        }
                    }
                    ActionsCheckbox {
                        label: "Auto mobbing uses key when pathing",
                        tooltip: "Pathing means when the player is moving from one quad to another.",
                        disabled,
                        on_checked: move |auto_mob_use_key_when_pathing| {
                            save_minimap(Minimap {
                                auto_mob_use_key_when_pathing,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().auto_mob_use_key_when_pathing,
                    }
                    ActionsMillisInput {
                        label: "Detect mobs when pathing every",
                        disabled,
                        on_value: move |auto_mob_use_key_when_pathing_update_millis| {
                            save_minimap(Minimap {
                                auto_mob_use_key_when_pathing_update_millis,
                                ..minimap.peek().clone()
                            })
                        },
                        value: minimap().auto_mob_use_key_when_pathing_update_millis,
                    }
                    ActionsCheckbox {
                        label: "Reset normal actions on Erda Shower resets",
                        disabled,
                        on_checked: move |actions_any_reset_on_erda_condition| {
                            save_minimap(Minimap {
                                actions_any_reset_on_erda_condition,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().actions_any_reset_on_erda_condition,
                    }
                }
            }

            match popup_content() {
                PopupContent::None => rsx! {},
                PopupContent::Bound(bound) => rsx! {
                    PopupMobbingBoundInputContent {
                        on_cancel: move |_| {
                            popup_open.set(false);
                        },
                        on_value: move |bound| {
                            edit_mobbing_bound(bound);
                            popup_open.set(false);
                        },
                        value: bound,
                    }
                },
                PopupContent::Key(key) => rsx! {
                    PopupMobbingKeyInputContent {
                        on_cancel: move |_| {
                            popup_open.set(false);
                        },
                        on_value: move |key| {
                            edit_mobbing_key(key);
                            popup_open.set(false);
                        },
                        value: key,
                    }
                },
            }
        }
    }
}

#[component]
fn SectionPlatforms(disabled: bool) -> Element {
    #[component]
    fn PlatformItem(
        platform: Platform,
        on_item_click: Callback,
        on_item_delete: Callback,
    ) -> Element {
        const ICON_CONTAINER_CLASS: &str = "w-4 h-6 flex justify-center items-center";
        const ICON_CLASS: &str = "size-3";

        rsx! {
            div { class: "flex group",
                div {
                    class: "flex-grow grid grid-cols-2 h-6 text-xxs gap-2 text-secondary-text group-hover:bg-secondary-surface",
                    onclick: move |_| {
                        on_item_click(());
                    },
                    div { class: "{ITEM_BORDER_CLASS} {ITEM_TEXT_CLASS}",
                        {format!("X / {} - {}", platform.x_start, platform.x_end)}
                    }
                    div { class: "{ITEM_TEXT_CLASS}", {format!("Y / {}", platform.y)} }
                }
                div { class: "self-stretch invisible group-hover:visible group-hover:bg-secondary-surface flex items-center pr-1",
                    div {
                        class: ICON_CONTAINER_CLASS,
                        onclick: move |e| {
                            e.stop_propagation();
                            on_item_delete(());
                        },
                        XIcon { class: "{ICON_CLASS}" }
                    }
                }
            }
        }
    }

    #[derive(PartialEq, Clone, Copy)]
    enum PopupContent {
        None,
        Edit { platform: Platform, index: usize },
        Add,
    }

    let coroutine = use_coroutine_handle::<ActionsUpdate>();
    let settings = use_context::<AppState>().settings;
    let position = use_context::<AppState>().position;
    let context = use_context::<ActionsContext>();

    let minimap = context.minimap;
    let save_minimap = context.save_minimap;

    let add_platform = use_callback(move |platform| {
        let mut minimap = minimap();

        minimap.platforms.push(platform);
        coroutine.send(ActionsUpdate::UpdateMinimap(minimap));
    });
    let edit_platform = use_callback(move |(new_platform, index): (Platform, usize)| {
        let mut minimap = minimap();
        let Some(platform) = minimap.platforms.get_mut(index) else {
            return;
        };

        *platform = new_platform;
        coroutine.send(ActionsUpdate::UpdateMinimap(minimap));
    });
    let delete_platform = use_callback(move |index| {
        let mut minimap = minimap();

        minimap.platforms.remove(index);
        coroutine.send(ActionsUpdate::UpdateMinimap(minimap));
    });

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    use_future(move || async move {
        let mut platform = Platform::default();
        let mut key_receiver = key_receiver().await;
        loop {
            let key = match key_receiver.recv().await {
                Ok(value) => value,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => continue,
            };
            let Some(settings) = &*settings.peek() else {
                continue;
            };

            if settings.platform_start_key.enabled && settings.platform_start_key.key == key {
                platform.x_start = position.peek().0;
                update_valid_platform_end(&mut platform);
                platform.y = position.peek().1;
                continue;
            }

            if settings.platform_end_key.enabled && settings.platform_end_key.key == key {
                platform.x_end = position.peek().0;
                update_valid_platform_end(&mut platform);
                platform.y = position.peek().1;
                continue;
            }

            if settings.platform_add_key.enabled && settings.platform_add_key.key == key {
                update_valid_platform_end(&mut platform);
                add_platform(platform);
                continue;
            }
        }
    });

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open: bool| {
                popup_open.set(open);
            },
            Section { title: "Platforms",
                div { class: "grid grid-cols-3 gap-3",
                    ActionsCheckbox {
                        label: "Rune pathing",
                        disabled,
                        on_checked: move |rune_platforms_pathing| {
                            save_minimap(Minimap {
                                rune_platforms_pathing,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().rune_platforms_pathing,
                    }
                    ActionsCheckbox {
                        label: "Up jump only",
                        disabled: disabled || !minimap().rune_platforms_pathing,
                        on_checked: move |rune_platforms_pathing_up_jump_only| {
                            save_minimap(Minimap {
                                rune_platforms_pathing_up_jump_only,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().rune_platforms_pathing_up_jump_only,
                    }
                    div {}
                    ActionsCheckbox {
                        label: "Auto-mobbing pathing",
                        disabled,
                        on_checked: move |auto_mob_platforms_pathing| {
                            save_minimap(Minimap {
                                auto_mob_platforms_pathing,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().auto_mob_platforms_pathing,
                    }
                    ActionsCheckbox {
                        label: "Up jump only",
                        disabled: disabled || !minimap().auto_mob_platforms_pathing,
                        on_checked: move |auto_mob_platforms_pathing_up_jump_only| {
                            save_minimap(Minimap {
                                auto_mob_platforms_pathing_up_jump_only,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().auto_mob_platforms_pathing_up_jump_only,
                    }
                    ActionsCheckbox {
                        label: "Bound by platforms",
                        tooltip: "Auto-mobbing bound is computed based on the provided platforms instead of the provided bound.",
                        disabled,
                        on_checked: move |auto_mob_platforms_bound| {
                            save_minimap(Minimap {
                                auto_mob_platforms_bound,
                                ..minimap.peek().clone()
                            })
                        },
                        checked: minimap().auto_mob_platforms_bound,
                    }
                }
                if !minimap().platforms.is_empty() {
                    div { class: "mt-2" }
                }
                for (index , platform) in minimap().platforms.into_iter().enumerate() {
                    PopupTrigger {
                        PlatformItem {
                            platform,
                            on_item_click: move |_| {
                                popup_content
                                    .set(PopupContent::Edit {
                                        platform,
                                        index,
                                    });
                            },
                            on_item_delete: move |_| {
                                delete_platform(index);
                            },
                        }
                    }
                }

                PopupTrigger {
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| {
                            popup_content.set(PopupContent::Add);
                        },
                        disabled,
                        class: "mt-2 w-full",

                        "Add platform"
                    }
                }

                PopupPlatformInputContent {
                    modifying: match popup_content() {
                        PopupContent::None | PopupContent::Add => false,
                        PopupContent::Edit { .. } => true,
                    },
                    on_cancel: move |_| {
                        popup_open.set(false);
                    },
                    on_value: move |mut platform| {
                        update_valid_platform_end(&mut platform);
                        let content = *popup_content.peek();
                        match content {
                            PopupContent::None => unreachable!(),
                            PopupContent::Add => add_platform(platform),
                            PopupContent::Edit { index, .. } => edit_platform((platform, index)),
                        }
                        popup_open.set(false);
                    },
                    value: match popup_content() {
                        PopupContent::None | PopupContent::Add => Platform::default(),
                        PopupContent::Edit { platform, .. } => platform,
                    },
                }
            }
        }
    }
}

#[component]
fn SectionLegends() -> Element {
    rsx! {
        Section { title: "Action legends", class: "text-xs text-primary-text",
            p { "⟳ - Repeat" }
            p { "⏱︎  - Wait" }
            p { "ㄨ - No position" }
            p { "⇈ - Queue to front" }
            p { "⇆ - Any direction" }
            p { "← - Left direction" }
            p { "→ - Right direction" }
            p { "A ~ B - Random range between A and B" }
            p { "A ↝ B - Use A key then B key" }
            p { "A ↜ B - Use B key then A key" }
            p { "A ↭ B - Use A and B keys at the same time" }
            p { "A ↷ B - Use A key then B key while A is held down" }
        }
    }
}

#[component]
fn SectionActions(actions: Memo<Vec<Action>>, disabled: bool) -> Element {
    #[derive(Clone, Copy, PartialEq)]
    enum PopupContent {
        None,
        Add(Action),
        Edit { action: Action, index: usize },
    }

    let coroutine = use_coroutine_handle::<ActionsUpdate>();

    let export_element_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    let export = use_callback(move |_| {
        let js = format!(
            r#"
            const element = document.getElementById("{}");
            if (element === null) {{
                return;
            }}
            const json = await dioxus.recv();

            element.setAttribute("href", "data:application/json;charset=utf-8," + encodeURIComponent(json));
            element.setAttribute("download", "actions.json");
            element.click();
            "#,
            export_element_id(),
        );
        let eval = document::eval(js.as_str());
        let Ok(json) = serde_json::to_string_pretty(&*actions.peek()) else {
            return;
        };
        let _ = eval.send(json);
    });

    let import_element_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    let import = use_callback(move |_| {
        let js = format!(
            r#"
            const element = document.getElementById("{}");
            if (element === null) {{
                return;
            }}
            element.click();
            "#,
            import_element_id()
        );
        document::eval(js.as_str());
    });
    let import_actions = use_callback(move |files| {
        let mut actions = actions();

        for file in files {
            let Ok(file) = File::open(file) else {
                continue;
            };
            let reader = BufReader::new(file);
            let Ok(import_actions) = serde_json::from_reader::<_, Vec<Action>>(reader) else {
                continue;
            };

            let mut i = 0;
            while i < import_actions.len() {
                let action = import_actions[i];
                if matches!(action.condition(), ActionCondition::Linked) {
                    // Malformed
                    i += 1;
                    continue;
                }

                actions.push(action);
                if let Some(range) = find_linked_action_range(&import_actions, i) {
                    actions.extend(import_actions[range.clone()].iter().copied());
                    i += range.count();
                }
                i += 1;
            }
        }

        coroutine.send(ActionsUpdate::Update(actions));
    });

    let add_action = use_callback(move |(action, condition): (Action, ActionCondition)| {
        let mut actions = actions();
        let index = if matches!(action.condition(), ActionCondition::Linked) {
            find_last_linked_action_index(&actions, condition)
                .map(|index| index + 1)
                .unwrap_or(actions.len())
        } else {
            actions.len()
        };

        actions.insert(index, action);
        coroutine.send(ActionsUpdate::Update(actions));
    });

    let edit_action = use_callback(move |(new_action, index): (Action, usize)| {
        let mut actions = actions();
        let Some(action) = actions.get_mut(index) else {
            return;
        };

        *action = new_action;
        coroutine.send(ActionsUpdate::Update(actions));
    });

    let delete_action = use_callback(move |index: usize| {
        let mut actions = actions();
        let Some(condition) = actions.get(index).map(|action| action.condition()) else {
            return;
        };

        // Replaces the first linked action to this `action` condition
        // TODO: Maybe replace find_linked_action_range with a simple lookahead
        if !matches!(condition, ActionCondition::Linked)
            && find_linked_action_range(&actions, index).is_some()
        {
            actions[index + 1] = actions[index + 1].with_condition(condition);
        }
        actions.remove(index);
        coroutine.send(ActionsUpdate::Update(actions));
    });

    let move_action = use_callback(
        move |(index, condition, up): (usize, ActionCondition, bool)| {
            let mut actions = actions();
            let filtered = filter_actions(actions.clone(), condition);
            if (up && index <= filtered.first().expect("cannot be empty").1)
                || (!up && index >= filtered.last().expect("cannot be empty").1)
            {
                return;
            }

            // Finds the action index of `filtered` before or after `index`
            let filtered_index = filtered
                .iter()
                .enumerate()
                .find_map(|(filtered_index, (_, actions_index))| {
                    if *actions_index == index {
                        if up {
                            Some(filtered_index - 1)
                        } else {
                            Some(filtered_index + 1)
                        }
                    } else {
                        None
                    }
                })
                .expect("must be valid index");
            let filtered_condition = filtered[filtered_index].0.condition();
            let action_condition = actions[index].condition();
            match (action_condition, filtered_condition) {
                // Simple case - swapping two linked actions
                (ActionCondition::Linked, ActionCondition::Linked) => {
                    actions.swap(index, filtered[filtered_index].1);
                    coroutine.send(ActionsUpdate::Update(actions));
                    return;
                }
                // Disallows moving up/down if `index` is a linked action and
                // `filtered_index` is a non-linked action
                (ActionCondition::Linked, _) => return,
                _ => (),
            }

            // Finds the first non-linked action index of `filtered` before or after `index`
            let mut filtered_non_linked_index = filtered_index;
            while (up && filtered_non_linked_index > 0)
                || (!up && filtered_non_linked_index < filtered.len() - 1)
            {
                let condition = filtered[filtered_non_linked_index].0.condition();
                if !matches!(condition, ActionCondition::Linked) {
                    break;
                }
                if up {
                    filtered_non_linked_index -= 1;
                } else {
                    filtered_non_linked_index += 1;
                }
            }
            let condition = filtered[filtered_non_linked_index].0.condition();
            if matches!(condition, ActionCondition::Linked) {
                return;
            }

            let actions_non_linked_index = filtered[filtered_non_linked_index].1;
            let first_range = find_linked_action_range(&actions, actions_non_linked_index);
            let mut first_range = if let Some(range) = first_range {
                actions_non_linked_index..range.end
            } else {
                actions_non_linked_index..actions_non_linked_index + 1
            };

            let second_range = find_linked_action_range(&actions, index);
            let mut second_range = if let Some(range) = second_range {
                index..range.end
            } else {
                index..index + 1
            };

            if !up {
                swap(&mut first_range, &mut second_range);
            }

            debug_assert!(
                first_range.end <= second_range.start || second_range.end <= first_range.start
            );
            let second_start = second_range.start;
            let second_actions = actions.drain(second_range).collect::<Vec<_>>();
            let first_actions = actions[first_range.clone()].to_vec();
            for action in first_actions.into_iter().rev() {
                actions.insert(second_start, action);
            }

            let first_start = first_range.start;
            let _ = actions.drain(first_range);
            for action in second_actions.into_iter().rev() {
                actions.insert(first_start, action);
            }
            coroutine.send(ActionsUpdate::Update(actions));
        },
    );

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    let mut handle_add_action_click = move |condition: ActionCondition| {
        let action = Action::Key(ActionKey::default()).with_condition(condition);
        let content = PopupContent::Add(action);
        popup_content.set(content);
    };

    let mut handle_edit_action_click = move |action: Action, index: usize| {
        popup_content.set(PopupContent::Edit { action, index });
    };

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open: bool| {
                popup_open.set(open);
            },
            Section { title: "Normal actions",
                ActionList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::Any);
                    },
                    on_item_click: move |(action, index)| {
                        handle_edit_action_click(action, index);
                    },
                    on_item_move: move |(index, condition, up)| {
                        move_action((index, condition, up));
                    },
                    on_item_delete: move |index| {
                        delete_action(index);
                    },
                    condition_filter: ActionCondition::Any,
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Erda Shower off cooldown priority actions",
                ActionList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::ErdaShowerOffCooldown);
                    },
                    on_item_click: move |(action, index)| {
                        handle_edit_action_click(action, index);
                    },
                    on_item_move: move |(index, condition, up)| {
                        move_action((index, condition, up));
                    },
                    on_item_delete: move |index| {
                        delete_action(index);
                    },
                    condition_filter: ActionCondition::ErdaShowerOffCooldown,
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Every milliseconds priority actions",
                ActionList {
                    on_add_click: move |_| {
                        handle_add_action_click(ActionCondition::EveryMillis(0));
                    },
                    on_item_click: move |(action, index)| {
                        handle_edit_action_click(action, index);
                    },
                    on_item_move: move |(index, condition, up)| {
                        move_action((index, condition, up));
                    },
                    on_item_delete: move |index| {
                        delete_action(index);
                    },
                    condition_filter: ActionCondition::EveryMillis(0),
                    disabled,
                    actions: actions(),
                }
            }
            Section { title: "Import/export actions",
                div { class: "flex gap-2",
                    div { class: "flex-grow",
                        a {
                            id: export_element_id(),
                            class: "w-0 h-0 invisible",
                        }
                        Button {
                            class: "w-full",
                            style: ButtonStyle::Primary,
                            disabled,
                            on_click: move |_| {
                                export(());
                            },
                            "Export"
                        }
                    }
                    div { class: "flex-grow",
                        input {
                            id: import_element_id(),
                            class: "w-0 h-0 invisible",
                            r#type: "file",
                            accept: ".json",
                            name: "Actions JSON",
                            onchange: move |e| {
                                if let Some(files) = e.data.files().map(|engine| engine.files()) {
                                    import_actions(files);
                                }
                            },
                        }
                        Button {
                            class: "w-full",
                            style: ButtonStyle::Primary,
                            disabled,
                            on_click: move |_| {
                                import(());
                            },
                            "Import"
                        }
                    }
                }
            }

            match popup_content() {
                PopupContent::None => rsx! {},
                PopupContent::Add(action) => rsx! {
                    PopupActionInputContent {
                        modifying: false,
                        linkable: !filter_actions(actions(), action.condition()).is_empty(),
                        on_cancel: move |_| {
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        on_value: move |args| {
                            add_action(args);
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        value: action,
                    }
                },
                PopupContent::Edit { action, index } => rsx! {
                    PopupActionInputContent {
                        modifying: true,
                        linkable: filter_actions(actions(), action.condition())
                            .into_iter()
                            .next()
                            .map(|first| first.1 != index)
                            .unwrap_or_default(),
                        on_copy: move |_| {
                            popup_content.set(PopupContent::Add(action));
                        },
                        on_cancel: move |_| {
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        on_value: move |(action, _)| {
                            edit_action((action, index));
                            popup_open.set(false);
                            popup_content.set(PopupContent::None);
                        },
                        value: action,
                    }
                },
            }
        }
    }
}

#[component]
fn PopupPlatformInputContent(
    modifying: bool,
    on_cancel: Callback,
    on_value: Callback<Platform>,
    value: Platform,
) -> Element {
    let position = use_context::<AppState>().position;
    let mut platform = use_signal(|| value);

    rsx! {
        PopupContent { title: if modifying { "Modify platform" } else { "Add platform" },
            div { class: "grid grid-cols-3 gap-3 pb-10 overflow-y-auto",
                ActionsPositionInput {
                    label: "X start",
                    on_icon_click: move |_| {
                        platform.write().x_start = position.peek().0;
                    },
                    on_value: move |x| {
                        platform.write().x_start = x;
                    },
                    value: platform().x_start,
                }
                ActionsPositionInput {
                    label: "X end",
                    on_icon_click: move |_| {
                        platform.write().x_end = position.peek().0;
                    },
                    on_value: move |x| {
                        platform.write().x_end = x;
                    },
                    value: platform().x_end,
                }
                ActionsPositionInput {
                    label: "Y",
                    on_icon_click: move |_| {
                        platform.write().y = position.peek().1;
                    },
                    on_value: move |y| {
                        platform.write().y = y;
                    },
                    value: platform().y,
                }
            }

            div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlinePrimary,
                    on_click: move |_| {
                        on_value(*platform.peek());
                    },

                    if modifying {
                        "Save"
                    } else {
                        "Add"
                    }
                }
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlineSecondary,
                    on_click: move |_| {
                        on_cancel(());
                    },
                    "Cancel"
                }
            }
        }
    }
}

#[component]
fn PopupMobbingBoundInputContent(
    on_cancel: Callback,
    on_value: Callback<Bound>,
    value: Bound,
) -> Element {
    let mut bound = use_signal(|| value);

    rsx! {
        PopupContent { title: "Modify mobbing bound",
            div { class: "grid grid-cols-2 gap-3 pb-10 overflow-y-auto",
                ActionsNumberInputI32 {
                    label: "X offset",
                    on_value: move |x| {
                        bound.write().x = x;
                    },
                    value: bound().x,
                }
                ActionsNumberInputI32 {
                    label: "Y offset",
                    on_value: move |y| {
                        bound.write().y = y;
                    },
                    value: bound().y,
                }
                ActionsNumberInputI32 {
                    label: "Width",
                    on_value: move |width| {
                        bound.write().width = width;
                    },
                    value: bound().width,
                }
                ActionsNumberInputI32 {
                    label: "Height",
                    on_value: move |height| {
                        bound.write().height = height;
                    },
                    value: bound().height,
                }
            }

            div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlinePrimary,
                    on_click: move |_| {
                        on_value(*bound.peek());
                    },

                    "Save"
                }
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlineSecondary,
                    on_click: move |_| {
                        on_cancel(());
                    },
                    "Cancel"
                }
            }
        }
    }
}

#[component]
fn PopupMobbingKeyInputContent(
    on_cancel: Callback,
    on_value: Callback<MobbingKey>,
    value: MobbingKey,
) -> Element {
    let key = ActionKey {
        key: value.key,
        link_key: value.link_key,
        count: value.count,
        with: value.with,
        wait_before_use_millis: value.wait_before_millis,
        wait_before_use_millis_random_range: value.wait_before_millis_random_range,
        wait_after_use_millis: value.wait_after_millis,
        wait_after_use_millis_random_range: value.wait_after_millis_random_range,
        ..ActionKey::default()
    };
    let action = Action::Key(key);

    rsx! {
        PopupContent { title: "Modify mobbing key",
            ActionInput {
                switchable: false,
                modifying: true,
                directionable: false,
                on_cancel,
                on_value: move |(action, _)| {
                    let action = match action {
                        Action::Move(_) => unreachable!(),
                        Action::Key(action) => action,
                    };
                    let key = MobbingKey {
                        key: action.key,
                        link_key: action.link_key,
                        count: action.count,
                        with: action.with,
                        wait_before_millis: action.wait_before_use_millis,
                        wait_before_millis_random_range: action.wait_before_use_millis_random_range,
                        wait_after_millis: action.wait_after_use_millis,
                        wait_after_millis_random_range: action.wait_after_use_millis_random_range,
                    };
                    on_value(key);
                },
                value: action,
            }
        }
    }
}

#[component]
fn PopupActionInputContent(
    modifying: bool,
    linkable: bool,
    #[props(default)] on_copy: Option<Callback>,
    on_cancel: Callback,
    on_value: Callback<(Action, ActionCondition)>,
    value: Action,
) -> Element {
    let name = match value.condition() {
        backend::ActionCondition::Any => "normal",
        backend::ActionCondition::EveryMillis(_) => "every milliseconds",
        backend::ActionCondition::ErdaShowerOffCooldown => "Erda Shower off cooldown",
        backend::ActionCondition::Linked => "linked",
    };
    let title = if modifying {
        format!("Modify a {name} action")
    } else {
        format!("Add a new {name} action")
    };

    rsx! {
        PopupContent { title,
            ActionInput {
                switchable: true,
                modifying,
                linkable,
                positionable: true,
                directionable: true,
                on_copy,
                on_cancel,
                on_value,
                value,
            }
        }
    }
}

#[component]
fn ActionInput(
    switchable: bool,
    modifying: bool,
    #[props(default)] linkable: bool,
    #[props(default)] positionable: bool,
    directionable: bool,
    #[props(default)] on_copy: Option<Callback>,
    on_cancel: Callback,
    on_value: Callback<(Action, ActionCondition)>,
    value: ReadOnlySignal<Action>,
) -> Element {
    let mut action = use_signal(&*value);
    let button_text = use_memo(move || {
        if matches!(action(), Action::Move(_)) {
            "Switch to key"
        } else {
            "Switch to move"
        }
    });

    use_effect(move || {
        action.set(value());
    });

    rsx! {
        div { class: "flex flex-col pb-10 overflow-y-auto max-h-100",
            if switchable || on_copy.is_some() {
                div { class: "grid grid-flow-col",
                    if switchable {
                        Button {
                            style: ButtonStyle::Primary,
                            on_click: move |_| {
                                let value = *value.peek();
                                if discriminant(&value) != discriminant(&*action.peek()) {
                                    action.set(value);
                                } else if matches!(value, Action::Move(_)) {
                                    action
                                        .set(
                                            Action::Key(ActionKey {
                                                condition: value.condition(),
                                                ..ActionKey::default()
                                            }),
                                        );
                                } else {
                                    action
                                        .set(
                                            Action::Move(ActionMove {
                                                condition: value.condition(),
                                                ..ActionMove::default()
                                            }),
                                        );
                                }
                            },
                            class: "text-xxs",

                            {button_text}
                        }
                    }
                    if let Some(on_copy) = on_copy {
                        Button {
                            style: ButtonStyle::Primary,
                            on_click: on_copy,
                            class: "text-xxs",
                            "Copy"
                        }
                    }
                }
                div { class: "col-span-3 border-b border-primary-border mb-3" }
            }
            match action() {
                Action::Move(action) => rsx! {
                    ActionMoveInput {
                        modifying,
                        linkable,
                        on_cancel,
                        on_value: move |(action, condition)| {
                            on_value((Action::Move(action), condition));
                        },
                        value: action,
                    }
                },
                Action::Key(action) => rsx! {
                    ActionKeyInput {
                        modifying,
                        linkable,
                        positionable,
                        directionable,
                        on_cancel,
                        on_value: move |(action, condition)| {
                            on_value((Action::Key(action), condition));
                        },
                        value: action,
                    }
                },
            }
        }
    }
}

#[component]
fn ActionMoveInput(
    modifying: bool,
    linkable: bool,
    on_cancel: Callback,
    on_value: Callback<(ActionMove, ActionCondition)>,
    value: ReadOnlySignal<ActionMove>,
) -> Element {
    let position = use_context::<AppState>().position;
    let mut action = use_signal(&*value);
    let action_condition = value().condition;

    use_effect(move || {
        action.set(value());
    });

    rsx! {
        div { class: "grid grid-cols-3 gap-3",
            // Position
            ActionsCheckbox {
                label: "Adjust",
                on_checked: move |adjust: bool| {
                    let mut action = action.write();
                    action.position.allow_adjusting = adjust;
                },
                checked: action().position.allow_adjusting,
            }
            div { class: "col-span-2" }
            ActionsPositionInput {
                label: "X",
                on_icon_click: move |_| {
                    let mut action = action.write();
                    action.position.x = position.peek().0;
                },
                on_value: move |x| {
                    let mut action = action.write();
                    action.position.x = x;
                },
                value: action().position.x,
            }

            ActionsNumberInputI32 {
                label: "X random range",
                on_value: move |x| {
                    let mut action = action.write();
                    action.position.x_random_range = x;
                },
                value: action().position.x_random_range,
            }
            ActionsPositionInput {
                label: "Y",
                on_icon_click: move |_| {
                    let mut action = action.write();
                    action.position.y = position.peek().1;
                },
                on_value: move |y| {
                    let mut action = action.write();
                    action.position.y = y;
                },
                value: action().position.y,
            }
            ActionsMillisInput {
                label: "Wait after move",
                on_value: move |millis| {
                    let mut action = action.write();
                    action.wait_after_move_millis = millis;
                },
                value: action().wait_after_move_millis,
            }
            if linkable {
                ActionsCheckbox {
                    label: "Linked action",
                    on_checked: move |is_linked: bool| {
                        let mut action = action.write();
                        action.condition = if is_linked {
                            ActionCondition::Linked
                        } else {
                            action_condition
                        };
                    },
                    checked: matches!(action().condition, ActionCondition::Linked),
                }
            }
        }
        div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
            Button {
                class: "flex-grow",
                style: ButtonStyle::OutlinePrimary,
                on_click: move |_| {
                    on_value((*action.peek(), action_condition));
                },
                if modifying {
                    "Save"
                } else {
                    "Add"
                }
            }
            Button {
                class: "flex-grow",
                style: ButtonStyle::OutlineSecondary,
                on_click: move |_| {
                    on_cancel(());
                },
                "Cancel"
            }
        }
    }
}

#[component]
fn ActionKeyInput(
    modifying: bool,
    linkable: bool,
    positionable: bool,
    directionable: bool,
    on_cancel: Callback,
    on_value: Callback<(ActionKey, ActionCondition)>,
    value: ReadOnlySignal<ActionKey>,
) -> Element {
    let position = use_context::<AppState>().position;
    let mut action = use_signal(&*value);
    let action_condition = value().condition;

    use_effect(move || {
        action.set(value());
    });

    rsx! {
        div { class: "grid grid-cols-3 gap-3 pr-2 overflow-y-auto",
            if positionable {
                ActionsCheckbox {
                    label: "Positioned",
                    on_checked: move |has_position: bool| {
                        let mut action = action.write();
                        action.position = has_position.then_some(Position::default());
                    },
                    checked: action().position.is_some(),
                }
                ActionsCheckbox {
                    label: "Adjust",
                    disabled: action().position.is_none(),
                    on_checked: move |adjust: bool| {
                        let mut action = action.write();
                        action.position.as_mut().unwrap().allow_adjusting = adjust;
                    },
                    checked: action().position.map(|pos| pos.allow_adjusting).unwrap_or_default(),
                }
                div {}

                ActionsPositionInput {
                    label: "X",
                    disabled: action().position.is_none(),
                    on_icon_click: action()
                        .position
                        .is_some()
                        .then_some(
                            Callback::new(move |_| {
                                let mut action = action.write();
                                if let Some(pos) = action.position.as_mut() {
                                    pos.x = position.peek().0;
                                }
                            }),
                        ),
                    on_value: move |x| {
                        let mut action = action.write();
                        if let Some(pos) = action.position.as_mut() {
                            pos.x = x;
                        }
                    },
                    value: action().position.map(|pos| pos.x).unwrap_or_default(),
                }
                ActionsNumberInputI32 {
                    label: "X random range",
                    disabled: action().position.is_none(),
                    on_value: move |x| {
                        let mut action = action.write();
                        if let Some(pos) = action.position.as_mut() {
                            pos.x_random_range = x;
                        }
                    },
                    value: action().position.map(|pos| pos.x_random_range).unwrap_or_default(),
                }
                ActionsPositionInput {
                    label: "Y",
                    disabled: action().position.is_none(),
                    on_icon_click: action()
                        .position
                        .is_some()
                        .then_some(
                            Callback::new(move |_| {
                                let mut action = action.write();
                                if let Some(pos) = action.position.as_mut() {
                                    pos.y = position.peek().1;
                                }
                            }),
                        ),
                    on_value: move |y| {
                        let mut action = action.write();
                        if let Some(pos) = action.position.as_mut() {
                            pos.y = y;
                        }
                    },
                    value: action().position.map(|pos| pos.y).unwrap_or_default(),
                }
            }

            // Key, count and link key
            ActionsKeyBindingInput {
                label: "Key",
                disabled: false,
                on_value: move |key: Option<KeyBinding>| {
                    let mut action = action.write();
                    action.key = key.expect("not optional");
                },
                value: Some(action().key),
            }
            ActionsNumberInputU32 {
                label: "Use count",
                on_value: move |count| {
                    let mut action = action.write();
                    action.count = count;
                },
                value: action().count,
            }
            if linkable {
                ActionsCheckbox {
                    label: "Linked action",
                    on_checked: move |is_linked: bool| {
                        let mut action = action.write();
                        action.condition = if is_linked {
                            ActionCondition::Linked
                        } else {
                            action_condition
                        };
                        action.queue_to_front = None;
                    },
                    checked: matches!(action().condition, ActionCondition::Linked),
                }
            } else {
                div {} // Spacer
            }
            ActionsKeyBindingInput {
                label: "Link key",
                disabled: action().link_key.is_none(),
                on_value: move |key: Option<KeyBinding>| {
                    let mut action = action.write();
                    action.link_key = action
                        .link_key
                        .map(|link_key| link_key.with_key(key.expect("not optional")));
                },
                value: action().link_key.unwrap_or_default().key(),
            }
            ActionsSelect::<LinkKeyBinding> {
                label: "Link key type",
                disabled: action().link_key.is_none(),
                on_selected: move |link_key: LinkKeyBinding| {
                    let mut action = action.write();
                    action.link_key = Some(
                        link_key.with_key(action.link_key.expect("has link key if selectable").key()),
                    );
                },
                selected: action().link_key.unwrap_or_default(),
            }
            ActionsCheckbox {
                label: "Has link key",
                on_checked: move |has_link_key: bool| {
                    let mut action = action.write();
                    action.link_key = has_link_key.then_some(LinkKeyBinding::default());
                },
                checked: action().link_key.is_some(),
            }

            // Use with, direction

            ActionsSelect::<ActionKeyWith> {
                label: "Use with",
                disabled: false,
                on_selected: move |with| {
                    let mut action = action.write();
                    action.with = with;
                },
                selected: action().with,
            }
            if directionable {
                ActionsSelect::<ActionKeyDirection> {
                    label: "Use direction",
                    disabled: false,
                    on_selected: move |direction| {
                        let mut action = action.write();
                        action.direction = direction;
                    },
                    selected: action().direction,
                }
            } else {
                div {} // Spacer
            }
            if matches!(
                action().condition,
                ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown
            )
            {
                ActionsCheckbox {
                    label: "Queue to front",
                    on_checked: move |queue_to_front: bool| {
                        let mut action = action.write();
                        action.queue_to_front = Some(queue_to_front);
                    },
                    checked: action().queue_to_front.is_some(),
                }
            } else {
                div {} // Spacer
            }
            if let ActionCondition::EveryMillis(millis) = action().condition {
                ActionsMillisInput {
                    label: "Use every",
                    on_value: move |millis| {
                        let mut action = action.write();
                        action.condition = ActionCondition::EveryMillis(millis);
                    },
                    value: millis,
                }
                div { class: "col-span-2" }
            }

            // Wait before use
            ActionsMillisInput {
                label: "Wait before",
                on_value: move |millis| {
                    let mut action = action.write();
                    action.wait_before_use_millis = millis;
                },
                value: action().wait_before_use_millis,
            }
            ActionsMillisInput {
                label: "Wait random range",
                on_value: move |millis| {
                    let mut action = action.write();
                    action.wait_before_use_millis_random_range = millis;
                },
                value: action().wait_before_use_millis_random_range,
            }
            div {} // Spacer

            // Wait after use
            ActionsMillisInput {
                label: "Wait after",
                on_value: move |millis| {
                    let mut action = action.write();
                    action.wait_after_use_millis = millis;
                },
                value: action().wait_after_use_millis,
            }
            ActionsMillisInput {
                label: "Wait random range",
                on_value: move |millis| {
                    let mut action = action.write();
                    action.wait_after_use_millis_random_range = millis;
                },
                value: action().wait_after_use_millis_random_range,
            }
        }
        div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
            Button {
                class: "flex-grow",
                style: ButtonStyle::OutlinePrimary,
                on_click: move |_| {
                    on_value((*action.peek(), action_condition));
                },
                if modifying {
                    "Save"
                } else {
                    "Add"
                }
            }
            Button {
                class: "flex-grow",
                style: ButtonStyle::OutlineSecondary,
                on_click: move |_| {
                    on_cancel(());
                },
                "Cancel"
            }
        }
    }
}

#[component]
fn ActionList(
    on_add_click: Callback,
    on_item_click: Callback<(Action, usize)>,
    on_item_move: Callback<(usize, ActionCondition, bool)>,
    on_item_delete: Callback<usize>,
    condition_filter: ActionCondition,
    disabled: bool,
    actions: Vec<Action>,
) -> Element {
    #[component]
    fn Icons(
        condition_filter: ActionCondition,
        action: Action,
        index: usize,
        on_item_move: Callback<(usize, ActionCondition, bool)>,
        on_item_delete: Callback<usize>,
    ) -> Element {
        const ICON_CONTAINER_CLASS: &str = "size-fit";
        const ICON_CLASS: &str = "size-3";

        let container_margin = if matches!(action.condition(), ActionCondition::Linked) {
            ""
        } else {
            "mt-2"
        };
        rsx! {
            div { class: "self-stretch invisible group-hover:visible group-hover:bg-secondary-surface flex gap-1 items-center {container_margin} pr-1",
                div {
                    class: ICON_CONTAINER_CLASS,
                    onclick: move |e| {
                        e.stop_propagation();
                        on_item_move((index, condition_filter, true));
                    },
                    UpArrowIcon { class: ICON_CLASS }
                }
                div {
                    class: ICON_CONTAINER_CLASS,
                    onclick: move |e| {
                        e.stop_propagation();
                        on_item_move((index, condition_filter, false));
                    },
                    DownArrowIcon { class: ICON_CLASS }
                }
                div {
                    class: ICON_CONTAINER_CLASS,
                    onclick: move |e| {
                        e.stop_propagation();
                        on_item_delete(index);
                    },
                    XIcon { class: "{ICON_CLASS}" }
                }
            }
        }
    }

    let filtered = filter_actions(actions, condition_filter);

    rsx! {
        div { class: "flex flex-col",
            for (action , index) in filtered {
                div {
                    class: "flex group flex-grow",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_item_click((action, index));
                    },

                    PopupTrigger { class: "flex-grow",
                        match action {
                            Action::Move(action) => rsx! {
                                ActionMoveItem { action }
                            },
                            Action::Key(action) => rsx! {
                                ActionKeyItem { action }
                            },
                        }
                    }

                    Icons {
                        condition_filter,
                        action,
                        index,
                        on_item_move,
                        on_item_delete,
                    }
                }
            }

            PopupTrigger {
                Button {
                    style: ButtonStyle::Secondary,
                    on_click: move |_| {
                        on_add_click(());
                    },
                    disabled,
                    class: "mt-2 w-full",

                    "Add action"
                }
            }
        }
    }
}

#[component]
fn ActionMoveItem(action: ActionMove) -> Element {
    let ActionMove {
        position:
            Position {
                x,
                x_random_range,
                y,
                allow_adjusting,
            },
        condition,
        wait_after_move_millis,
    } = action;

    let x_min = (x - x_random_range).max(0);
    let x_max = (x + x_random_range).max(0);
    let x = if x_min == x_max {
        format!("{x}")
    } else {
        format!("{x_min}~{x_max}")
    };
    let allow_adjusting = if allow_adjusting { " / Adjust" } else { "" };

    let position = format!("{x}, {y}{allow_adjusting}");
    let linked_action = if matches!(condition, ActionCondition::Linked) {
        ""
    } else {
        "mt-2"
    };
    let wait_secs = format!("⏱︎ {:.2}s", wait_after_move_millis as f32 / 1000.0);

    rsx! {
        div { class: "grid grid-cols-[140px_100px_auto] h-6 text-xs text-secondary-text group-hover:bg-secondary-surface {linked_action}",
            div { class: "{ITEM_BORDER_CLASS} {ITEM_TEXT_CLASS}", "{position}" }
            div { class: "{ITEM_TEXT_CLASS}", "{wait_secs}" }
            div {}
        }
    }
}

#[component]
fn ActionKeyItem(action: ActionKey) -> Element {
    let ActionKey {
        key,
        link_key,
        count,
        position,
        condition,
        direction,
        with,
        queue_to_front,
        wait_before_use_millis,
        wait_after_use_millis,
        ..
    } = action;

    let position = if let Some(Position {
        x,
        y,
        x_random_range,
        allow_adjusting,
    }) = position
    {
        let x_min = (x - x_random_range).max(0);
        let x_max = (x + x_random_range).max(0);
        let x = if x_min == x_max {
            format!("{x}")
        } else {
            format!("{x_min}~{x_max}")
        };
        let allow_adjusting = if allow_adjusting { " / Adjust" } else { "" };

        format!("{x}, {y}{allow_adjusting}")
    } else {
        "ㄨ".to_string()
    };
    let queue_to_front = if queue_to_front.unwrap_or_default() {
        "⇈ / "
    } else {
        ""
    };
    let linked_action = if matches!(condition, ActionCondition::Linked) {
        ""
    } else {
        "mt-2"
    };
    let link_key = match link_key {
        Some(LinkKeyBinding::Before(key)) => format!("{key} ↝ "),
        Some(LinkKeyBinding::After(key)) => format!("{key} ↜ "),
        Some(LinkKeyBinding::AtTheSame(key)) => format!("{key} ↭ "),
        Some(LinkKeyBinding::Along(key)) => format!("{key} ↷ "),
        None => "".to_string(),
    };
    let millis = if let ActionCondition::EveryMillis(millis) = condition {
        format!("⟳ {:.2}s / ", millis as f32 / 1000.0)
    } else {
        "".to_string()
    };
    let wait_before_secs = if wait_before_use_millis > 0 {
        Some(format!("⏱︎ {:.2}s", wait_before_use_millis as f32 / 1000.0))
    } else {
        None
    };
    let wait_after_secs = if wait_after_use_millis > 0 {
        Some(format!("⏱︎ {:.2}s", wait_after_use_millis as f32 / 1000.0))
    } else {
        None
    };
    let wait_secs = match (wait_before_secs, wait_after_secs) {
        (Some(before), None) => format!("{before} - ⏱︎ 0.00s / "),
        (None, None) => "".to_string(),
        (None, Some(after)) => format!("⏱︎ 0.00s - {after} / "),
        (Some(before), Some(after)) => format!("{before} - {after} / "),
    };
    let with = match with {
        ActionKeyWith::Any => "Any",
        ActionKeyWith::Stationary => "Stationary",
        ActionKeyWith::DoubleJump => "Double jump",
    };

    rsx! {
        div { class: "grid grid-cols-[140px_100px_30px_auto] h-6 text-xs text-secondary-text group-hover:bg-secondary-surface {linked_action}",
            div { class: "{ITEM_BORDER_CLASS} {ITEM_TEXT_CLASS}", "{queue_to_front}{position}" }
            div { class: "{ITEM_BORDER_CLASS} {ITEM_TEXT_CLASS}", "{link_key}{key} × {count}" }
            div { class: "{ITEM_BORDER_CLASS} {ITEM_TEXT_CLASS}",
                match direction {
                    ActionKeyDirection::Any => "⇆",
                    ActionKeyDirection::Left => "←",
                    ActionKeyDirection::Right => "→",
                }
            }
            div { class: "pl-1 pr-13 {ITEM_TEXT_CLASS}", "{millis}{wait_secs}{with}" }
        }
    }
}

#[component]
fn ActionsSelect<T: 'static + Clone + PartialEq + Display + IntoEnumIterator>(
    label: &'static str,
    disabled: bool,
    on_selected: Callback<T>,
    selected: ReadOnlySignal<T>,
) -> Element {
    let selected_equal =
        use_callback(move |value: T| discriminant(&selected()) == discriminant(&value));

    rsx! {
        Labeled { label,
            Select::<T> { on_selected, disabled,

                for value in T::iter() {
                    SelectOption::<T> {
                        value: value.clone(),
                        label: value.to_string(),
                        selected: selected_equal(value),
                        disabled,
                    }
                }
            }
        }
    }
}

#[component]
fn ActionsPositionInput(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_icon_click: ReadOnlySignal<Option<Callback>>,
    on_value: Callback<i32>,
    value: i32,
) -> Element {
    rsx! {
        Labeled { label,
            PositionInput {
                disabled,
                on_icon_click,
                on_value,
                value,
            }
        }
    }
}

#[component]
fn ActionsNumberInputI32(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_value: Callback<i32>,
    value: i32,
) -> Element {
    rsx! {
        Labeled { label,
            PrimitiveIntegerInput { disabled, on_value, value }
        }
    }
}

#[component]
fn ActionsNumberInputU32(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_value: Callback<u32>,
    value: u32,
) -> Element {
    rsx! {
        Labeled { label,
            PrimitiveIntegerInput {
                disabled,
                on_value,
                value,
                min_value: 1,
            }
        }
    }
}

#[component]
fn ActionsMillisInput(
    label: &'static str,
    #[props(default)] disabled: bool,
    on_value: Callback<u64>,
    value: u64,
) -> Element {
    rsx! {
        Labeled { label,
            MillisInput { disabled, on_value, value }
        }
    }
}

#[component]
fn ActionsCheckbox(
    label: &'static str,
    #[props(default)] tooltip: Option<String>,
    #[props(default)] disabled: bool,
    on_checked: Callback<bool>,
    checked: bool,
) -> Element {
    rsx! {
        Labeled {
            label,
            tooltip,
            tooltip_side: ContentSide::Left,
            tooltip_align: ContentAlign::End,
            Checkbox { disabled, on_checked, checked }
        }
    }
}

#[component]
fn ActionsKeyBindingInput(
    label: &'static str,
    disabled: bool,
    on_value: Callback<Option<KeyBinding>>,
    value: Option<KeyBinding>,
) -> Element {
    rsx! {
        Labeled { label,
            KeyInput {
                class: "border border-primary-border",
                disabled,
                on_value: move |value: Option<KeyBinding>| {
                    on_value(value);
                },
                value,
            }
        }
    }
}

/// Finds the linked action index range where `action_index` is a non-linked action.
fn find_linked_action_range(actions: &[Action], action_index: usize) -> Option<Range<usize>> {
    if action_index + 1 >= actions.len() {
        return None;
    }
    let start = action_index + 1;
    if !matches!(actions[start].condition(), ActionCondition::Linked) {
        return None;
    }

    let mut end = start + 1;
    while end < actions.len() {
        if !matches!(actions[end].condition(), ActionCondition::Linked) {
            break;
        }
        end += 1;
    }

    Some(start..end)
}

/// Finds the last linked action index of the last action matching `condition_filter`.
fn find_last_linked_action_index(
    actions: &[Action],
    condition_filter: ActionCondition,
) -> Option<usize> {
    let condition_filter = discriminant(&condition_filter);
    let (mut last_index, _) = actions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, action)| condition_filter == discriminant(&action.condition()))?;

    if let Some(range) = find_linked_action_range(actions, last_index) {
        last_index += range.count();
    }

    Some(last_index)
}

/// Filters `actions` to find action with condition matching `condition_filter` including linked
/// action(s) of that matching action.
///
/// Returns a [`Vec<(Action, usize)>`] where [`usize`] is the index of the action inside the
/// original `actions`.
fn filter_actions(actions: Vec<Action>, condition_filter: ActionCondition) -> Vec<(Action, usize)> {
    let condition_filter = discriminant(&condition_filter);
    let mut filtered = Vec::with_capacity(actions.len());
    let mut i = 0;
    while i < actions.len() {
        let action = actions[i];
        if condition_filter != discriminant(&action.condition()) {
            i += 1;
            continue;
        }

        filtered.push((action, i));
        if let Some(range) = find_linked_action_range(&actions, i) {
            filtered.extend(actions[range.clone()].iter().copied().zip(range.clone()));
            i += range.count();
        }
        i += 1;
    }

    filtered
}

#[inline]
fn update_valid_platform_end(platform: &mut Platform) {
    platform.x_end = if platform.x_end <= platform.x_start {
        platform.x_start + 1
    } else {
        platform.x_end
    };
}
