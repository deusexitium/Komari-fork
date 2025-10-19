use std::fmt::Display;

use backend::{
    DatabaseEvent, NavigationPath, NavigationPaths, NavigationPoint, NavigationTransition,
    create_navigation_path, database_event_receiver, delete_navigation_paths,
    navigation_snapshot_as_grayscale, query_navigation_paths, recapture_navigation_path,
    upsert_minimap, upsert_navigation_paths,
};
use dioxus::prelude::*;
use futures_util::StreamExt;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    AppState,
    components::{
        button::{Button, ButtonStyle},
        checkbox::Checkbox,
        icons::{DetailsIcon, XIcon},
        labeled::Labeled,
        named_select::NamedSelect,
        popup::{PopupContent, PopupContext, PopupTrigger},
        position::PositionInput,
        section::Section,
        select::{Select, SelectOption},
    },
};

type PathsIdIndex = Option<(i64, usize)>;

#[derive(Debug)]
enum NavigationUpdate {
    Update(NavigationPaths),
    Create(String),
    Delete,
    Attach(PathsIdIndex),
}

#[derive(Clone, Copy, PartialEq)]
struct NavigationContext {
    selected_path_group: Signal<Option<NavigationPaths>>,
    path_groups: Memo<Vec<NavigationPaths>>,
    path_group_names: Memo<Vec<String>>,
}

#[component]
pub fn NavigationScreen() -> Element {
    let mut minimap = use_context::<AppState>().minimap;

    let mut path_groups = use_resource(async || query_navigation_paths().await.unwrap_or_default());
    let path_groups_view = use_memo(move || path_groups().unwrap_or_default());
    let path_group_names_view = use_memo(move || {
        path_groups_view()
            .into_iter()
            .map(|paths| paths.name)
            .collect::<Vec<_>>()
    });

    let mut selected_path_group = use_signal(|| None);

    use_coroutine(
        move |mut rx: UnboundedReceiver<NavigationUpdate>| async move {
            while let Some(message) = rx.next().await {
                match message {
                    NavigationUpdate::Update(paths) => {
                        if let Some(paths) = upsert_navigation_paths(paths).await {
                            selected_path_group.set(Some(paths));
                        };
                    }
                    NavigationUpdate::Create(name) => {
                        let paths = NavigationPaths {
                            name,
                            ..NavigationPaths::default()
                        };
                        if let Some(paths) = upsert_navigation_paths(paths).await {
                            selected_path_group.set(Some(paths));
                        };
                    }
                    NavigationUpdate::Delete => {
                        let Some(paths) = selected_path_group() else {
                            continue;
                        };

                        if delete_navigation_paths(paths).await {
                            selected_path_group.set(None);
                        }
                    }
                    NavigationUpdate::Attach(paths_id_index) => {
                        let Some(mut current_minimap) = minimap() else {
                            continue;
                        };
                        current_minimap.paths_id_index = paths_id_index;
                        if let Some(current_minimap) = upsert_minimap(current_minimap).await {
                            minimap.set(Some(current_minimap));
                        }
                    }
                }
            }
        },
    );

    use_context_provider(|| NavigationContext {
        selected_path_group,
        path_groups: path_groups_view,
        path_group_names: path_group_names_view,
    });

    use_effect(move || {
        let groups = path_groups_view();
        if !groups.is_empty() && selected_path_group.peek().is_none() {
            let minimap_group_id = minimap
                .peek()
                .as_ref()
                .and_then(|minimap| minimap.paths_id_index)
                .map(|(id, _)| id);

            if let Some(group) = groups
                .iter()
                .find(|group| group.id == minimap_group_id)
                .cloned()
            {
                selected_path_group.set(Some(group));
            } else {
                selected_path_group.set(groups.into_iter().next());
            }
        }
    });

    use_future(move || async move {
        let mut rx = database_event_receiver();
        loop {
            let event = match rx.recv().await {
                Ok(value) => value,
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => continue,
            };
            if matches!(
                event,
                DatabaseEvent::NavigationPathsUpdated | DatabaseEvent::NavigationPathsDeleted
            ) {
                path_groups.restart();
            }
        }
    });

    rsx! {
        div { class: "flex flex-col h-full overflow-y-auto",
            SectionSelectedMap {}
            SectionPaths {}
        }
    }
}

#[component]
fn SectionSelectedMap() -> Element {
    let minimap = use_context::<AppState>().minimap;
    let coroutine = use_coroutine_handle::<NavigationUpdate>();

    let context = use_context::<NavigationContext>();
    let path_groups = context.path_groups;
    let path_group_names = context.path_group_names;

    let minimap_path_group_id_index =
        use_memo(move || minimap().and_then(|minimap| minimap.paths_id_index));
    let minimap_path_group_index = use_memo(move || {
        let paths = path_groups();
        minimap_path_group_id_index().and_then(|(id, _)| {
            paths.into_iter().enumerate().find_map(|(index, path)| {
                if path.id == Some(id) {
                    Some(index + 1) // + 1 for "None"
                } else {
                    None
                }
            })
        })
    });
    let minimap_path_group_paths = use_memo(move || {
        minimap_path_group_index()
            .map(|index| index - 1)
            .and_then(|index| {
                path_groups
                    .peek()
                    .get(index)
                    .map(|group| 0..group.paths.len())
            })
            .unwrap_or_default()
            .map(|index| format!("Path {}", index + 1))
            .collect::<Vec<_>>()
    });

    let select_group = use_callback(move |index: usize| {
        let next_group_id = if index == 0 {
            None
        } else {
            let index = index - 1;
            let groups = path_groups.peek();
            groups
                .get(index)
                .and_then(|group: &NavigationPaths| group.id)
        };
        let next_paths_id_index = next_group_id.map(|id| (id, 0));

        coroutine.send(NavigationUpdate::Attach(next_paths_id_index));
    });

    let select_path = use_callback(move |index: usize| {
        let Some((id, _)) = *minimap_path_group_id_index.peek() else {
            return;
        };

        coroutine.send(NavigationUpdate::Attach(Some((id, index))));
    });

    let minimap_path_group_index_or_default = minimap_path_group_id_index()
        .map(|(_, index)| index)
        .unwrap_or_default();

    rsx! {
        Section { title: "Selected map",
            div { class: "grid grid-cols-2 gap-3",
                Labeled { label: "Attached path group",
                    NavigationSelect::<String> {
                        disabled: minimap().is_none(),
                        options: [vec!["None".to_string()], path_group_names()].concat(),
                        on_selected: select_group,
                        selected: minimap_path_group_index().unwrap_or_default(),
                    }
                }

                Labeled { label: "Attached path",
                    NavigationSelect::<String> {
                        placeholder: "None",
                        disabled: minimap_path_group_index().is_none(),
                        options: minimap_path_group_paths(),
                        on_selected: select_path,
                        selected: minimap_path_group_index_or_default,
                    }
                }
            }
        }
    }
}

#[component]
fn SectionPaths() -> Element {
    #[derive(PartialEq, Clone)]
    enum PopupContent {
        None,
        AddPoint {
            path_index: usize,
            point: NavigationPoint,
        },
        EditPoint {
            path_index: usize,
            point: NavigationPoint,
            point_index: usize,
        },
        EditPath {
            path: NavigationPath,
            path_index: usize,
        },
    }

    let position = use_context::<AppState>().position;
    let coroutine = use_coroutine_handle::<NavigationUpdate>();

    let context = use_context::<NavigationContext>();
    let path_groups = context.path_groups;
    let path_group_names = context.path_group_names;
    let mut selected_path_group = context.selected_path_group;

    let selected_path_group_index = use_memo(move || {
        selected_path_group().and_then(|group: NavigationPaths| {
            path_groups()
                .into_iter()
                .enumerate()
                .find_map(|(index, group_iter)| (group_iter.id == group.id).then_some(index))
        })
    });
    let select_path_group = use_callback(move |index: usize| {
        selected_path_group.set(path_groups.peek().get(index).cloned());
    });

    let add_point = use_callback::<(usize, NavigationPoint), _>(move |(path_index, point)| {
        let Some(mut current_paths) = selected_path_group.peek().clone() else {
            return;
        };
        let Some(current_path) = current_paths.paths.get_mut(path_index) else {
            return;
        };

        current_path.points.push(point);
        coroutine.send(NavigationUpdate::Update(current_paths));
    });

    let edit_point = use_callback::<(usize, NavigationPoint, usize), _>(
        move |(path_index, point, point_index)| {
            let Some(mut current_paths) = selected_path_group.peek().clone() else {
                return;
            };
            let Some(current_path) = current_paths.paths.get_mut(path_index) else {
                return;
            };
            let Some(current_point) = current_path.points.get_mut(point_index) else {
                return;
            };

            *current_point = point;
            coroutine.send(NavigationUpdate::Update(current_paths));
        },
    );

    let delete_point = use_callback::<(usize, usize), _>(move |(path_index, point_index)| {
        let Some(mut current_paths) = selected_path_group.peek().clone() else {
            return;
        };
        let Some(current_path) = current_paths.paths.get_mut(path_index) else {
            return;
        };

        current_path.points.remove(point_index);
        coroutine.send(NavigationUpdate::Update(current_paths));
    });

    let select_point_next = use_callback::<(usize, usize, PathsIdIndex), _>(
        move |(path_index, point_index, next_paths_id_index)| {
            let Some(mut current_paths) = selected_path_group.peek().clone() else {
                return;
            };
            let Some(current_path) = current_paths.paths.get_mut(path_index) else {
                return;
            };
            let Some(current_point) = current_path.points.get_mut(point_index) else {
                return;
            };

            current_point.next_paths_id_index = next_paths_id_index;
            coroutine.send(NavigationUpdate::Update(current_paths));
        },
    );

    let create_path = use_callback(move |_| async move {
        let Some(mut current_paths) = selected_path_group() else {
            return;
        };
        let Some(path) = create_navigation_path().await else {
            return;
        };

        current_paths.paths.push(path);
        coroutine.send(NavigationUpdate::Update(current_paths));
    });

    let edit_path = use_callback::<(NavigationPath, usize), _>(move |(path, path_index)| {
        let Some(mut current_paths) = selected_path_group() else {
            return;
        };
        let Some(current_path) = current_paths.paths.get_mut(path_index) else {
            return;
        };

        *current_path = path;
        coroutine.send(NavigationUpdate::Update(current_paths));
    });

    let delete_path = use_callback::<usize, _>(move |path_index| {
        let Some(mut group) = selected_path_group.peek().clone() else {
            return;
        };

        group.paths.remove(path_index);
        coroutine.send(NavigationUpdate::Update(group));
    });

    let mut popup_content = use_signal(|| PopupContent::None);
    let mut popup_open = use_signal(|| false);

    rsx! {
        PopupContext {
            open: popup_open,
            on_open: move |open| {
                popup_open.set(open);
            },

            Section { title: "Paths",
                NamedSelect {
                    class: "w-full",
                    on_create: move |name| {
                        coroutine.send(NavigationUpdate::Create(name));
                    },
                    on_delete: move |_| {
                        coroutine.send(NavigationUpdate::Delete);
                    },
                    delete_disabled: path_groups().is_empty(),

                    NavigationSelect {
                        class: "w-full",
                        options: path_group_names(),
                        placeholder: "Create a path group...",
                        disabled: path_groups().is_empty(),
                        on_selected: move |index| {
                            select_path_group(index);
                        },
                        selected: selected_path_group_index().unwrap_or_default(),
                    }
                }

                if let Some(paths) = selected_path_group() {
                    div { class: "flex flex-col gap-3",
                        for (path_index , path) in paths.paths.into_iter().enumerate() {
                            NavigationPathItem {
                                groups: path_groups,
                                group_names: path_group_names,
                                path,
                                path_index,
                                on_add_point: move |_| {
                                    popup_content
                                        .set(PopupContent::AddPoint {
                                            path_index,
                                            point: NavigationPoint {
                                                next_paths_id_index: None,
                                                x: position.peek().0,
                                                y: position.peek().1,
                                                transition: NavigationTransition::Portal,
                                            },
                                        });
                                },
                                on_delete_point: move |point_index| {
                                    delete_point((path_index, point_index));
                                },
                                on_edit_point: move |(point, point_index)| {
                                    popup_content
                                        .set(PopupContent::EditPoint {
                                            path_index,
                                            point,
                                            point_index,
                                        });
                                },
                                on_select_point_next: move |(point_index, path_ids_index)| {
                                    select_point_next((path_index, point_index, path_ids_index));
                                },
                                on_delete_path: move |_| {
                                    delete_path(path_index);
                                },
                                on_edit_path: move |path: NavigationPath| {
                                    popup_content
                                        .set(PopupContent::EditPath {
                                            path,
                                            path_index,
                                        });
                                },
                            }
                        }
                    }
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async move {
                            create_path(()).await;
                        },
                        class: "mt-4",

                        "Add path"
                    }
                }
            }

            match popup_content() {
                PopupContent::None => rsx! {},
                PopupContent::EditPoint { path_index, point, point_index } => {
                    rsx! {
                        PopupPointContent {
                            value: point,
                            on_cancel: move |_| {
                                popup_open.set(false);
                            },
                            on_save: move |point| {
                                edit_point((path_index, point, point_index));
                                popup_open.set(false);
                            },
                        }
                    }
                }
                PopupContent::AddPoint { path_index, point } => {
                    rsx! {
                        PopupPointContent {
                            value: point,
                            on_cancel: move |_| {
                                popup_open.set(false);
                            },
                            on_save: move |point| {
                                add_point((path_index, point));
                                popup_open.set(false);
                            },
                        }
                    }
                }
                PopupContent::EditPath { path, path_index } => rsx! {
                    PopupPathContent {
                        value: path,
                        on_save: move |path| {
                            edit_path((path, path_index));
                            popup_open.set(false);
                        },
                        on_cancel: move |_| {
                            popup_open.set(false);
                        },
                    }
                },
            }
        }
    }
}

#[component]
fn PopupPathContent(
    value: ReadOnlySignal<NavigationPath>,
    on_save: Callback<NavigationPath>,
    on_cancel: Callback,
) -> Element {
    let mut path = use_signal(&*value);
    let mut minimap_base64_current = use_signal(|| path().minimap_snapshot_base64);

    use_effect(move || {
        path.set(value.cloned());
    });
    use_effect(move || {
        let path = path();
        let base64 = path.minimap_snapshot_base64;
        let use_grayscale = path.minimap_snapshot_grayscale;

        spawn(async move {
            if use_grayscale {
                minimap_base64_current.set(navigation_snapshot_as_grayscale(base64).await);
            } else {
                minimap_base64_current.set(base64);
            }
        });
    });

    rsx! {
        PopupContent { title: "Path snapshots",

            div { class: "flex flex-col gap-3 pr-2 pb-10 overflow-y-auto min-w-sm",
                div { class: "flex flex-col",
                    Button {
                        class: "w-full",
                        style: ButtonStyle::Secondary,
                        on_click: move |_| async move {
                            path.set(recapture_navigation_path(path()).await);
                        },

                        "Re-capture"
                    }
                    div { class: "border-b border-secondary-border" }
                }
                p { class: "text-xs text-primary-text", "Name" }
                img {
                    src: format!("data:image/png;base64,{}", path().name_snapshot_base64),
                    class: "w-full h-full p-1 border border-secondary-border",
                }
                p { class: "text-xs text-primary-text", "Map" }
                img {
                    src: format!("data:image/png;base64,{}", minimap_base64_current()),
                    class: "w-full h-full p-1 border border-primary-border",
                }
                NavigationCheckbox {
                    label: "Use grayscale for map",
                    on_checked: move |minimap_snapshot_grayscale| {
                        path.with_mut(|path| {
                            path.minimap_snapshot_grayscale = minimap_snapshot_grayscale;
                        })
                    },
                    checked: path().minimap_snapshot_grayscale,
                }
            }
            div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlinePrimary,
                    on_click: move |_| {
                        on_save(path());
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
fn PopupPointContent(
    value: ReadOnlySignal<NavigationPoint>,
    on_save: Callback<NavigationPoint>,
    on_cancel: Callback,
) -> Element {
    let position = use_context::<AppState>().position;
    let mut xy = use_signal(&*value);

    use_effect(move || {
        xy.set(value.cloned());
    });

    rsx! {
        PopupContent { title: "Point",

            div { class: "grid grid-cols-2 gap-3 pb-10",
                NavigationPositionInput {
                    label: "X",
                    on_icon_click: move |_| {
                        xy.write().x = position.peek().0;
                    },
                    on_value: move |x| {
                        xy.write().x = x;
                    },
                    value: xy().x,
                }
                NavigationPositionInput {
                    label: "Y",
                    on_icon_click: move |_| {
                        xy.write().y = position.peek().1;
                    },
                    on_value: move |y| {
                        xy.write().y = y;
                    },
                    value: xy().y,
                }
            }

            div { class: "flex w-full gap-3 absolute bottom-0 py-2 bg-secondary-surface",
                Button {
                    class: "flex-grow",
                    style: ButtonStyle::OutlinePrimary,
                    on_click: move |_| {
                        on_save(*xy.peek());
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
fn NavigationPathItem(
    groups: Memo<Vec<NavigationPaths>>,
    group_names: Memo<Vec<String>>,
    path: ReadOnlySignal<NavigationPath>,
    path_index: usize,
    on_add_point: Callback,
    on_edit_point: Callback<(NavigationPoint, usize)>,
    on_delete_point: Callback<usize>,
    on_select_point_next: Callback<(usize, PathsIdIndex)>,
    on_delete_path: Callback,
    on_edit_path: Callback<NavigationPath>,
) -> Element {
    #[component]
    fn Icons(on_edit_path: Option<Callback>, on_delete: Callback) -> Element {
        const ICON_CONTAINER_CLASS: &str = "flex items-center";

        rsx! {
            div { class: "invisible group-hover:visible flex gap-1",
                div { class: "flex-grow" }
                if let Some(on_edit_path) = on_edit_path {
                    div {
                        class: ICON_CONTAINER_CLASS,
                        onclick: move |_| {
                            on_edit_path(());
                        },
                        PopupTrigger { DetailsIcon {} }
                    }
                }
                div {
                    class: ICON_CONTAINER_CLASS,
                    onclick: move |_| {
                        on_delete(());
                    },
                    XIcon { class: "size-3" }
                }
            }
        }
    }

    let get_point_group_index = use_callback(move |group_id| {
        groups()
            .iter()
            .enumerate()
            .find_map(|(index, group)| {
                if group.id == Some(group_id) {
                    Some(index + 1)
                } else {
                    None
                }
            })
            .unwrap_or_default()
    });
    let get_point_group_paths = use_callback(move |group_id| {
        groups()
            .into_iter()
            .find_map(|group| {
                if group.id == Some(group_id) {
                    Some(group.paths)
                } else {
                    None
                }
            })
            .unwrap_or_default()
    });
    let get_point_group_path_names = use_callback(move |paths_id| {
        (0..get_point_group_paths(paths_id).len())
            .map(|index| format!("Path {}", index + 1))
            .collect::<Vec<String>>()
    });
    // For avoiding too long line
    let get_point_path_index = use_callback(move |paths_id_index: PathsIdIndex| {
        paths_id_index.map(|(_, index)| index).unwrap_or_default()
    });

    rsx! {
        div { class: "mt-3",
            div { class: "grid grid-cols-2 gap-x-3 group",
                div { class: "border-b border-primary-border p-1",
                    img {
                        width: path().name_snapshot_width,
                        height: path().name_snapshot_height,
                        src: format!("data:image/png;base64,{}", path().name_snapshot_base64),
                    }
                }
                div { class: "grid grid-cols-3 gap-x-2 group",
                    p { class: "col-span-2 text-xs text-primary-text flex items-center border-b border-primary-border",
                        {format!("Path {}", path_index + 1)}
                    }
                    Icons {
                        on_edit_path: move |_| {
                            on_edit_path(path.peek().clone());
                        },
                        on_delete: move |_| {
                            on_delete_path(());
                        },
                    }
                }
            }

            for (index , point) in path().points.into_iter().enumerate() {
                div { class: "grid grid-cols-2 gap-x-3 group mt-2",
                    PopupTrigger {
                        div {
                            class: "grid grid-cols-[32px_auto] gap-x-2 group/info",
                            onclick: move |_| {
                                on_edit_point((point, index));
                            },
                            div { class: "h-full border-l-2 border-primary-border" }
                            p { class: "text-xxs text-secondary-text h-full flex items-center justify-centers group-hover/info:border-b group-hover/info:border-primary-border",
                                {format!("X / {}, Y / {} using {}", point.x, point.y, point.transition)}
                            }
                        }
                    }

                    div { class: "grid grid-cols-3 gap-x-2",
                        NavigationSelect::<String> {
                            options: [vec!["None".to_string()], group_names()].concat(),
                            on_selected: move |paths_index| {
                                let next_paths_id = if paths_index == 0 {
                                    None
                                } else {
                                    let index = paths_index - 1;
                                    let paths = groups.peek();
                                    paths.get(index).and_then(|path: &NavigationPaths| path.id)
                                };
                                let next_paths_id_index = next_paths_id.map(|id| (id, 0));
                                on_select_point_next((index, next_paths_id_index));
                            },
                            selected: point
                                .next_paths_id_index
                                .map(|(id, _)| get_point_group_index(id))
                                .unwrap_or_default(),
                        }
                        NavigationSelect::<String> {
                            placeholder: "None",
                            options: point
                                .next_paths_id_index
                                .map(|(id, _)| get_point_group_path_names(id))
                                .unwrap_or_default(),
                            on_selected: move |path_index| {
                                let next_paths_id_index = point
                                    .next_paths_id_index
                                    .map(|(id, _)| (id, path_index));
                                on_select_point_next((index, next_paths_id_index));
                            },
                            selected: get_point_path_index(point.next_paths_id_index),
                        }
                        Icons {
                            on_delete: move |_| {
                                on_delete_point(index);
                            },
                        }
                    }
                }
            }
            div { class: "grid grid-cols-2 gap-x-3 mt-2",
                PopupTrigger {
                    Button {
                        style: ButtonStyle::Secondary,
                        on_click: move |_| {
                            on_add_point(());
                        },
                        class: "w-full",

                        "Add point"
                    }
                }
                div {}
            }
        }
    }
}

#[component]
fn NavigationPositionInput(
    label: &'static str,
    value: i32,
    on_value: Callback<i32>,
    on_icon_click: Callback,
) -> Element {
    rsx! {
        Labeled { label,
            PositionInput { value, on_value, on_icon_click }
        }
    }
}

#[component]
fn NavigationCheckbox(label: &'static str, on_checked: Callback<bool>, checked: bool) -> Element {
    rsx! {
        Labeled { label,
            Checkbox { checked, on_checked }
        }
    }
}

#[component]
fn NavigationSelect<T: 'static + Clone + PartialEq + Display>(
    options: Vec<T>,
    #[props(default)] placeholder: String,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    on_selected: Callback<usize>,
    selected: ReadOnlySignal<usize>,
) -> Element {
    rsx! {
        Select::<usize> {
            on_selected,
            placeholder,
            disabled,
            class,

            for (i , value) in options.into_iter().enumerate() {
                SelectOption::<usize> {
                    value: i,
                    label: value.to_string(),
                    selected: i == selected(),
                }
            }
        }
    }
}
