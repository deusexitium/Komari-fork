use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use dyn_clone::clone_box;
use opencv::{
    core::{ToInputArray, Vector},
    imgcodecs::{IMREAD_COLOR, IMREAD_GRAYSCALE, imdecode, imencode_def},
};
use platforms::{Window, input::InputKind};
use serenity::all::{CreateAttachment, EditInteractionResponse};
use strum::EnumMessage;
use tokio::{
    sync::broadcast::Receiver,
    task::{JoinHandle, spawn, spawn_blocking},
    time::sleep,
};

use crate::{
    ActionKeyDirection, ActionKeyWith, Character, GameState, GameTemplate, KeyBinding,
    LinkKeyBinding, Localization, Minimap, NavigationPath, RequestHandler, RotateKind, Settings,
    bridge::{Capture, DefaultInputReceiver, Input},
    control::{BotAction, BotCommandKind},
    detect::to_base64_from_mat,
    ecs::{Resources, World, WorldEvent},
    navigator::Navigator,
    notification::NotificationKind,
    operation::Operation,
    player::{Chat, ChattingContent, Key, Panic, PanicTo, Panicking, Player, PlayerAction},
    poll_request,
    rotator::Rotator,
    services::{
        character::{CharacterService, DefaultCharacterService},
        control::ControlService,
        game::{DefaultGameService, GameEvent, GameService},
        localization::{DefaultLocalizationService, LocalizationService},
        minimap::{DefaultMinimapService, MinimapService},
        navigator::{DefaultNavigatorService, NavigatorService},
        rotator::{DefaultRotatorService, RotatorService},
        settings::{DefaultSettingsService, SettingsService},
    },
};
#[cfg(debug_assertions)]
use crate::{DebugState, services::debug::DebugService};

mod character;
mod control;
#[cfg(debug_assertions)]
mod debug;
mod game;
mod localization;
mod minimap;
mod navigator;
mod rotator;
mod settings;

#[derive(Debug)]
pub struct DefaultService {
    event_rx: Receiver<WorldEvent>,
    pending_halt: Option<JoinHandle<()>>,
    game: Box<dyn GameService>,
    minimap: Box<dyn MinimapService>,
    character: Box<dyn CharacterService>,
    rotator: Box<dyn RotatorService>,
    navigator: Box<dyn NavigatorService>,
    settings: Box<dyn SettingsService>,
    localization: Box<dyn LocalizationService>,
    bot: ControlService,
    #[cfg(debug_assertions)]
    debug: DebugService,
}

impl DefaultService {
    pub fn new(
        settings: Rc<RefCell<Settings>>,
        localization: Rc<RefCell<Arc<Localization>>>,
        event_rx: Receiver<WorldEvent>,
    ) -> Self {
        let settings_service = DefaultSettingsService::new(settings.clone());
        let window = settings_service.selected_window();
        let input_rx = DefaultInputReceiver::new(window, InputKind::Focused);
        let mut bot = ControlService::default();
        bot.update(&settings_service.settings());

        Self {
            event_rx,
            pending_halt: None,
            game: Box::new(DefaultGameService::new(input_rx)),
            minimap: Box::new(DefaultMinimapService::default()),
            character: Box::new(DefaultCharacterService::default()),
            rotator: Box::new(DefaultRotatorService::default()),
            navigator: Box::new(DefaultNavigatorService),
            settings: Box::new(settings_service),
            localization: Box::new(DefaultLocalizationService::new(localization)),
            bot,
            #[cfg(debug_assertions)]
            debug: DebugService::default(),
        }
    }

    pub fn update_input_and_capture(&mut self, input: &mut dyn Input, capture: &mut dyn Capture) {
        self.settings
            .apply_selected_window(input, self.game.input_receiver_mut(), capture);
    }

    pub fn selected_window(&self) -> Window {
        self.settings.selected_window()
    }

    #[inline]
    pub fn poll(
        &mut self,
        resources: &mut Resources,
        world: &mut World,
        rotator: &mut dyn Rotator,
        navigator: &mut dyn Navigator,
        capture: &mut dyn Capture,
    ) {
        let mut handler = DefaultRequestHandler {
            service: self,
            resources,
            world,
            rotator,
            navigator,
            capture,
        };
        // TODO: Maybe handling 1 by 1 on each tick instead of all at once?
        handler.poll_request();
        handler.poll_game_events();
        handler.poll_context_event();
        handler.poll_bot();
        handler.broadcast_state();
    }
}

#[derive(Debug)]
struct DefaultRequestHandler<'a> {
    service: &'a mut DefaultService,
    resources: &'a mut Resources,
    world: &'a mut World,
    rotator: &'a mut dyn Rotator,
    navigator: &'a mut dyn Navigator,
    capture: &'a mut dyn Capture,
}

impl DefaultRequestHandler<'_> {
    fn poll_request(&mut self) {
        poll_request(self);
    }

    fn poll_game_events(&mut self) {
        let events = self.service.game.poll_events(
            self.service
                .minimap
                .minimap()
                .and_then(|character| character.id),
            self.service
                .character
                .character()
                .and_then(|character| character.id),
            &self.service.settings.settings(),
        );
        for event in events {
            match event {
                GameEvent::ToggleOperation => {
                    let kind = if self.resources.operation.halting() {
                        RotateKind::Run
                    } else {
                        RotateKind::TemporaryHalt
                    };
                    self.update_halting(kind);
                }
                GameEvent::MinimapUpdated(minimap) => {
                    self.on_update_minimap(self.service.minimap.preset(), minimap)
                }
                GameEvent::CharacterUpdated(character) => self.on_update_character(character),
                GameEvent::SettingsUpdated(settings) => {
                    self.service.settings.update_settings(settings);
                    self.service.settings.apply_settings(
                        &mut self.resources.operation,
                        self.resources.input.as_mut(),
                        self.service.game.input_receiver_mut(),
                        self.capture,
                    );
                    self.service.bot.update(&self.service.settings.settings());
                    self.service.rotator.apply(
                        self.rotator,
                        self.service.minimap.minimap(),
                        self.service.character.character(),
                        &self.service.settings.settings(),
                    );
                }
                GameEvent::LocalizationUpdated(localization) => {
                    self.service.localization.update_localization(localization)
                }
                GameEvent::NavigationPathsUpdated => self.navigator.mark_dirty(true),
            }
        }

        #[cfg(debug_assertions)]
        self.service.debug.poll(self.resources);
    }

    fn poll_context_event(&mut self) {
        const PENDING_HALT_SECS: u64 = 12;

        if self
            .service
            .pending_halt
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            self.service.pending_halt = None;
            if !self.navigator.was_last_point_available_or_completed() {
                self.update_halt_or_panic(true, true);
            }
        }

        let Some(event) = self.service.event_rx.try_recv().ok() else {
            return;
        };
        match event {
            WorldEvent::CycledToHalt => {
                self.update_halt_or_panic(false, true);
            }
            WorldEvent::PlayerDied => {
                if self.service.settings.settings().stop_on_player_die {
                    self.update_halt_or_panic(true, false);
                }
            }
            WorldEvent::MinimapChanged => {
                if self.resources.operation.halting()
                    | !self.service.settings.settings().stop_on_fail_or_change_map
                {
                    return;
                }

                let player_panicking = matches!(
                    self.world.player.state,
                    Player::Panicking(Panicking {
                        to: PanicTo::Channel,
                        ..
                    })
                );
                if player_panicking {
                    return;
                }
                self.service.pending_halt = Some(spawn(async move {
                    sleep(Duration::from_secs(PENDING_HALT_SECS)).await;
                }));
            }
            WorldEvent::CaptureFailed => {
                if self.resources.operation.halting() {
                    return;
                }

                if self.service.settings.settings().stop_on_fail_or_change_map {
                    self.update_halt_or_panic(true, false);
                }
                let _ = self
                    .resources
                    .notification
                    .schedule_notification(NotificationKind::FailOrMapChange);
            }
        }
    }

    fn poll_bot(&mut self) {
        if let Some(command) = self.service.bot.poll() {
            match command.kind {
                BotCommandKind::Start => {
                    if !self.resources.operation.halting() {
                        let _ = command
                            .sender
                            .send(EditInteractionResponse::new().content("Bot already running."));
                        return;
                    }
                    if self.service.minimap.minimap().is_none()
                        || self.service.character.character().is_none()
                    {
                        let _ = command.sender.send(
                            EditInteractionResponse::new().content("No map or character data set."),
                        );
                        return;
                    }
                    let _ = command
                        .sender
                        .send(EditInteractionResponse::new().content("Bot started running."));
                    self.on_rotate_actions(RotateKind::Run);
                }
                BotCommandKind::Stop { go_to_town } => {
                    let _ = command
                        .sender
                        .send(EditInteractionResponse::new().content("Bot stopped running."));
                    self.update_halt_or_panic(true, go_to_town);
                }
                BotCommandKind::Suspend => {
                    let _ = command
                        .sender
                        .send(EditInteractionResponse::new().content("Bot attempted to suspend."));
                    self.update_halting(RotateKind::TemporaryHalt);
                }
                BotCommandKind::Status => {
                    let provider = state_and_frame_provider(self.resources, self.world);

                    spawn_blocking(move || {
                        let (status, frame) = provider();
                        let attachment =
                            frame.map(|bytes| CreateAttachment::bytes(bytes, "image.webp"));

                        let mut builder = EditInteractionResponse::new().content(status);
                        if let Some(attachment) = attachment {
                            builder = builder.new_attachment(attachment);
                        }

                        let _ = command.sender.send(builder);
                    });
                }
                BotCommandKind::Chat { content } => {
                    if content.chars().count() >= ChattingContent::MAX_LENGTH {
                        let builder = EditInteractionResponse::new().content(format!(
                            "Message length must be less than {} characters.",
                            ChattingContent::MAX_LENGTH
                        ));
                        let _ = command.sender.send(builder);
                        return;
                    }

                    let _ = command
                        .sender
                        .send(EditInteractionResponse::new().content("Queued a chat action."));
                    let action = PlayerAction::Chat(Chat { content });
                    self.rotator.inject_action(action);
                }
                BotCommandKind::Action { action, count } => {
                    // Emulate these actions through keys instead to avoid requiring position
                    let player_action = match action {
                        BotAction::Jump => PlayerAction::Key(Key {
                            key: self.world.player.context.config.jump_key.into(),
                            link_key: None,
                            count,
                            position: None,
                            direction: ActionKeyDirection::Any, // Must always be Any
                            with: ActionKeyWith::Any,           // Must always be Any
                            wait_before_use_ticks: 0,
                            wait_before_use_ticks_random_range: 5,
                            wait_after_use_ticks: 15,
                            wait_after_use_ticks_random_range: 0,
                        }),
                        BotAction::DoubleJump => {
                            PlayerAction::Key(Key {
                                key: self.world.player.context.config.jump_key.into(),
                                link_key: Some(LinkKeyBinding::Before(
                                    self.world.player.context.config.jump_key.into(),
                                )),
                                count,
                                position: None,
                                direction: ActionKeyDirection::Any, // Must always be Any
                                with: ActionKeyWith::Any,           // Must always be Any
                                wait_before_use_ticks: 0,
                                wait_before_use_ticks_random_range: 0,
                                wait_after_use_ticks: 0,
                                wait_after_use_ticks_random_range: 55,
                            })
                        }
                        BotAction::Crouch => {
                            PlayerAction::Key(Key {
                                key: KeyBinding::Down,
                                link_key: Some(LinkKeyBinding::Along(KeyBinding::Down)),
                                count,
                                position: None,
                                direction: ActionKeyDirection::Any, // Must always be Any
                                with: ActionKeyWith::Any,           // Must always be Any
                                wait_before_use_ticks: 0,
                                wait_before_use_ticks_random_range: 0,
                                wait_after_use_ticks: 10,
                                wait_after_use_ticks_random_range: 0,
                            })
                        }
                    };
                    self.rotator.inject_action(player_action.clone());
                    let _ = command
                        .sender
                        .send(EditInteractionResponse::new().content(format!(
                            "Queued `{}` x {count}",
                            action.get_message().expect("has message")
                        )));
                }
            }
        }
    }

    fn broadcast_state(&self) {
        self.service.game.broadcast_state(
            self.resources,
            self.world,
            self.service.minimap.minimap(),
        );
    }

    fn update_halting(&mut self, kind: RotateKind) {
        let settings = self.service.settings.settings();
        let operation = self.resources.operation;

        self.resources.operation = operation.update_from_rotate_kind_and_mode(
            kind,
            settings.cycle_run_stop,
            settings.cycle_run_duration_millis,
            settings.cycle_stop_duration_millis,
        );
        if matches!(kind, RotateKind::Halt | RotateKind::TemporaryHalt) {
            self.rotator.reset_queue();
            self.world.player.context.clear_actions_aborted(true);
            if let Some(handle) = self.service.pending_halt.take() {
                handle.abort();
            }
        }
    }

    fn update_halt_or_panic(&mut self, should_halt: bool, should_panic: bool) {
        self.rotator.reset_queue();
        self.world
            .player
            .context
            .clear_actions_aborted(!should_panic);
        if should_halt {
            if let Some(handle) = self.service.pending_halt.take() {
                handle.abort();
            }
            self.resources.operation = Operation::Halting;
        }
        if should_panic {
            self.rotator
                .inject_action(PlayerAction::Panic(Panic { to: PanicTo::Town }));
        }
    }
}

impl RequestHandler for DefaultRequestHandler<'_> {
    fn on_rotate_actions(&mut self, kind: RotateKind) {
        if self.service.minimap.minimap().is_none() || self.service.character.character().is_none()
        {
            return;
        }
        self.update_halting(kind);
    }

    fn on_create_minimap(&self, name: String) -> Option<Minimap> {
        self.service.minimap.create(self.world.minimap.state, name)
    }

    fn on_update_minimap(&mut self, preset: Option<String>, minimap: Option<Minimap>) {
        self.service.minimap.update_minimap_preset(minimap, preset);
        self.service.minimap.apply(
            &mut self.world.minimap.context,
            &mut self.world.player.context,
        );
        let minimap = self.service.minimap.minimap();
        let character = self.service.character.character();

        self.service
            .rotator
            .update_actions(minimap, self.service.minimap.preset(), character);

        self.navigator
            .mark_dirty_with_destination(minimap.and_then(|minimap| minimap.paths_id_index));

        self.service.rotator.apply(
            self.rotator,
            minimap,
            character,
            &self.service.settings.settings(),
        );
    }

    fn on_create_navigation_path(&self) -> Option<NavigationPath> {
        self.service
            .navigator
            .create_path(self.resources, self.world.minimap.state)
    }

    fn on_recapture_navigation_path(&self, path: NavigationPath) -> NavigationPath {
        self.service
            .navigator
            .recapture_path(self.resources, self.world.minimap.state, path)
    }

    fn on_navigation_snapshot_as_grayscale(&self, base64: String) -> String {
        self.service
            .navigator
            .navigation_snapshot_as_grayscale(base64)
    }

    fn on_update_character(&mut self, character: Option<Character>) {
        self.service.character.update_character(character);
        self.service
            .character
            .apply_character(&mut self.world.player.context);

        let character = self.service.character.character();
        let minimap = self.service.minimap.minimap();
        let preset = self.service.minimap.preset();
        let settings = self.service.settings.settings();

        self.service
            .rotator
            .update_actions(minimap, preset, character);
        self.service.rotator.update_buffs(character);
        if let Some(character) = character {
            self.world.buffs.iter_mut().for_each(|buff| {
                buff.context.update_enabled_state(character, &settings);
            });
        }
        self.service
            .rotator
            .apply(self.rotator, minimap, character, &settings);
    }

    fn on_redetect_minimap(&mut self) {
        self.service.minimap.redetect(&mut self.world.minimap);
        self.navigator.mark_dirty(true);
    }

    fn on_game_state_receiver(&self) -> Receiver<GameState> {
        self.service.game.subscribe_state()
    }

    fn on_key_receiver(&self) -> Receiver<KeyBinding> {
        self.service.game.subscribe_key()
    }

    fn on_refresh_capture_handles(&mut self) {
        self.service.settings.update_windows();
        self.on_select_capture_handle(None);
    }

    fn on_query_capture_handles(&self) -> (Vec<String>, Option<usize>) {
        (
            self.service.settings.window_names(),
            self.service.settings.selected_window_index(),
        )
    }

    fn on_select_capture_handle(&mut self, index: Option<usize>) {
        self.service.settings.update_selected_window(index);
        self.service.settings.apply_selected_window(
            self.resources.input.as_mut(),
            self.service.game.input_receiver_mut(),
            self.capture,
        );
    }

    fn on_query_template(&self, template: GameTemplate) -> String {
        self.service.localization.template(template)
    }

    fn on_convert_image_to_base64(&self, image: Vec<u8>, is_grayscale: bool) -> Option<String> {
        let flag = if is_grayscale {
            IMREAD_GRAYSCALE
        } else {
            IMREAD_COLOR
        };
        let vector = Vector::<u8>::from_iter(image);
        let mat = imdecode(&vector, flag).ok()?;

        to_base64_from_mat(&mat).ok()
    }

    fn on_save_capture_image(&self, is_grayscale: bool) {
        self.service
            .localization
            .save_capture_image(self.resources, is_grayscale);
    }

    #[cfg(debug_assertions)]
    fn on_debug_state_receiver(&self) -> Receiver<DebugState> {
        self.service.debug.subscribe_state()
    }

    #[cfg(debug_assertions)]
    fn on_auto_save_rune(&self, auto_save: bool) {
        self.service
            .debug
            .set_auto_save_rune(self.resources, auto_save);
    }

    #[cfg(debug_assertions)]
    fn on_infer_rune(&mut self) {
        self.service.debug.infer_rune();
    }

    #[cfg(debug_assertions)]
    fn on_infer_minimap(&self) {
        self.service.debug.infer_minimap(self.resources);
    }

    #[cfg(debug_assertions)]
    fn on_record_images(&mut self, start: bool) {
        self.service.debug.record_images(start);
    }

    #[cfg(debug_assertions)]
    fn on_test_spin_rune(&self) {
        self.service.debug.test_spin_rune();
    }
}

fn state_and_frame_provider(
    resources: &Resources,
    world: &World,
) -> impl FnOnce() -> (String, Option<Vec<u8>>) + Send + 'static {
    #[inline]
    fn frame_from(mat: &impl ToInputArray) -> Option<Vec<u8>> {
        let mut vector = Vector::new();
        imencode_def(".webp", mat, &mut vector).ok()?;
        Some(Vec::from_iter(vector))
    }

    let detector = resources
        .detector
        .as_ref()
        .map(|detector| clone_box(detector.as_ref()));
    let state = world.player.state.to_string();
    let operation = resources.operation.to_string();

    move || {
        let frame = detector.and_then(|detector| frame_from(detector.mat()));
        let info = [
            format!("- State: ``{state}``"),
            format!("- Operation: ``{operation}``"),
        ]
        .join("\n");
        (info, frame)
    }
}

// #[cfg(test)]
// mod tests {
//     use std::cell::RefCell;

//     use mockall::Sequence;
//     use tokio::sync::broadcast::channel;

//     use super::*;
//     use crate::{
//         Action, Character, KeyBindingConfiguration,
//         bridge::MockCapture,
//         buff::BuffKind,
//         database::Minimap as MinimapData,
//         navigator::MockNavigator,
//         player::PlayerContext,
//         rotator::MockRotator,
//         services::{
//             character::MockCharacterService, game::MockGameService, minimap::MockMinimapService,
//             rotator::MockRotatorService, settings::MockSettingsService,
//         },
//     };

//     fn mock_poll_args(
//         (context, player, minimap, buffs, rotator, navigator, capture): &mut (
//             Context,
//             PlayerContext,
//             MinimapState,
//             Vec<BuffState>,
//             MockRotator,
//             MockNavigator,
//             MockCapture,
//         ),
//     ) -> PollArgs<'_> {
//         PollArgs {
//             resources: context,
//             player,
//             minimap,
//             buffs,
//             rotator,
//             navigator,
//             capture,
//         }
//     }

//     fn mock_states() -> (
//         Context,
//         PlayerContext,
//         MinimapState,
//         Vec<BuffState>,
//         MockRotator,
//         MockNavigator,
//         MockCapture,
//     ) {
//         let context = Context::new(None, None);
//         let player = PlayerContext::default();
//         let minimap = MinimapState::default();
//         let buffs = vec![];
//         let rotator = MockRotator::default();
//         let navigator = MockNavigator::default();
//         let capture = MockCapture::default();

//         (context, player, minimap, buffs, rotator, navigator, capture)
//     }

//     #[test]
//     fn on_update_minimap_triggers_all_services() {
//         let mut states = mock_states();

//         let minimap_data = Box::leak(Box::new(MinimapData::default()));
//         let character_data = Box::leak(Box::new(Character::default()));
//         let settings_data = Box::leak(Box::new(RefCell::new(Settings::default())));
//         let actions = Vec::<Action>::new();
//         let buffs = Vec::<(BuffKind, KeyBinding)>::new();

//         let mut game = MockGameService::default();
//         let mut character = MockCharacterService::default();
//         let mut minimap = MockMinimapService::default();
//         let mut rotator = MockRotatorService::default();
//         let navigator = Box::new(DefaultNavigatorService);
//         let mut settings = MockSettingsService::default();
//         let mut sequence = Sequence::new();

//         minimap
//             .expect_set_minimap_preset()
//             .once()
//             .in_sequence(&mut sequence);
//         minimap.expect_update().once().in_sequence(&mut sequence);
//         minimap
//             .expect_minimap()
//             .once()
//             .return_const(Some(&*minimap_data))
//             .in_sequence(&mut sequence);

//         character
//             .expect_character()
//             .once()
//             .return_const(Some(&*character_data))
//             .in_sequence(&mut sequence);

//         minimap
//             .expect_preset()
//             .once()
//             .return_const(Some("preset".to_string()))
//             .in_sequence(&mut sequence);

//         game.expect_update_actions()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);

//         states
//             .5
//             .expect_mark_dirty_with_destination()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);
//         settings
//             .expect_settings()
//             .once()
//             .returning_st(|| settings_data.borrow());
//         game.expect_actions().once().return_const(actions);
//         game.expect_buffs().once().return_const(buffs);
//         rotator
//             .expect_update()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);

//         let (_tx, rx) = channel(1);
//         let args = mock_poll_args(&mut states);
//         let mut service = DefaultService {
//             event_rx: rx,
//             pending_halt: None,
//             game: Box::new(game),
//             minimap: Box::new(minimap),
//             character: Box::new(character),
//             rotator: Box::new(rotator),
//             navigator,
//             settings: Box::new(settings),
//             bot: BotService::default(),
//             #[cfg(debug_assertions)]
//             debug: crate::services::debug::DebugService::default(),
//         };
//         let mut handler = DefaultRequestHandler {
//             service: &mut service,
//             args,
//         };

//         handler.on_update_minimap(Some("preset".into()), Some(minimap_data.clone()));
//     }

//     #[test]
//     fn on_update_character_calls_dependencies() {
//         let mut states = mock_states();
//         states.3.push(BuffState::new(BuffKind::Familiar));
//         states.3.push(BuffState::new(BuffKind::SayramElixir));

//         let args = mock_poll_args(&mut states);
//         let minimap_data = Box::leak(Box::new(MinimapData::default()));
//         let character_data = Box::leak(Box::new(Character {
//             sayram_elixir_key: KeyBindingConfiguration {
//                 key: KeyBinding::C,
//                 enabled: true,
//             },
//             familiar_buff_key: KeyBindingConfiguration {
//                 key: KeyBinding::B,
//                 enabled: true,
//             },
//             ..Default::default()
//         }));
//         let settings_data = Box::leak(Box::new(RefCell::new(Settings::default())));
//         let actions = Vec::<Action>::new();
//         let buffs = Vec::<(BuffKind, KeyBinding)>::new();

//         let mut game = MockGameService::default();
//         let mut character = MockCharacterService::default();
//         let mut minimap = MockMinimapService::default();
//         let mut rotator = MockRotatorService::default();
//         let navigator = Box::new(DefaultNavigatorService);
//         let mut settings = MockSettingsService::default();
//         let mut sequence = Sequence::new();

//         character
//             .expect_set_character()
//             .once()
//             .in_sequence(&mut sequence);
//         character.expect_update().once().in_sequence(&mut sequence);

//         character
//             .expect_character()
//             .once()
//             .return_const(Some(&*character_data))
//             .in_sequence(&mut sequence);
//         minimap
//             .expect_minimap()
//             .once()
//             .return_const(Some(&*minimap_data))
//             .in_sequence(&mut sequence);
//         minimap
//             .expect_preset()
//             .once()
//             .return_const(Some("preset".to_string()))
//             .in_sequence(&mut sequence);
//         settings
//             .expect_settings()
//             .once()
//             .returning_st(|| settings_data.borrow());

//         game.expect_update_actions()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);
//         game.expect_update_buffs()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);

//         game.expect_actions()
//             .once()
//             .return_const(actions)
//             .in_sequence(&mut sequence);
//         game.expect_buffs()
//             .once()
//             .return_const(buffs)
//             .in_sequence(&mut sequence);
//         rotator
//             .expect_update()
//             .once()
//             .return_const(())
//             .in_sequence(&mut sequence);

//         let (_tx, rx) = channel(1);
//         let mut service = DefaultService {
//             event_rx: rx,
//             pending_halt: None,
//             game: Box::new(game),
//             minimap: Box::new(minimap),
//             character: Box::new(character),
//             rotator: Box::new(rotator),
//             navigator,
//             settings: Box::new(settings),
//             bot: BotService::default(),
//             #[cfg(debug_assertions)]
//             debug: crate::services::debug::DebugService::default(),
//         };
//         let mut handler = DefaultRequestHandler {
//             service: &mut service,
//             args,
//         };

//         handler.on_update_character(Some(character_data.clone()));

//         // TODO: Assert buffs
//     }
// }
