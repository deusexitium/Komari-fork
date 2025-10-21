use std::fmt;

use opencv::core::{Point, Rect};
use strum::Display;

use super::{Player, PlayerContext, use_key::UseKey};
use crate::{
    Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, FamiliarRarity, KeyBinding,
    Position, SwappableFamiliars, array::Array, bridge::KeyKind, database::LinkKeyBinding,
    ecs::Resources, minimap::Minimap, player::PlayerEntity, run::MS_PER_TICK, transition,
    transition_if,
};

/// The minimum x distance required to transition to [`Player::UseKey`] in auto mob action.
pub const AUTO_MOB_USE_KEY_X_THRESHOLD: i32 = 16;

/// The minimum y distance required to transition to [`Player::UseKey`] in auto mob action.
pub const AUTO_MOB_USE_KEY_Y_THRESHOLD: i32 = 8;

/// Represents the fixed key action.
///
/// Converted from [`ActionKey`] without fields used by [`Rotator`]
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub key: KeyBinding,
    pub link_key: Option<LinkKeyBinding>,
    pub count: u32,
    pub position: Option<Position>,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_ticks: u32,
    pub wait_before_use_ticks_random_range: u32,
    pub wait_after_use_ticks: u32,
    pub wait_after_use_ticks_random_range: u32,
}

impl From<ActionKey> for Key {
    fn from(
        ActionKey {
            key,
            link_key,
            count,
            position,
            direction,
            with,
            wait_before_use_millis,
            wait_before_use_millis_random_range,
            wait_after_use_millis,
            wait_after_use_millis_random_range,
            ..
        }: ActionKey,
    ) -> Self {
        let count = count.max(1);
        let wait_before_use_ticks = (wait_before_use_millis / MS_PER_TICK) as u32;
        let wait_before_use_ticks_random_range =
            (wait_before_use_millis_random_range / MS_PER_TICK) as u32;
        let wait_after_use_ticks = (wait_after_use_millis / MS_PER_TICK) as u32;
        let wait_after_use_ticks_random_range =
            (wait_after_use_millis_random_range / MS_PER_TICK) as u32;

        Self {
            key,
            link_key,
            count,
            position,
            direction,
            with,
            wait_before_use_ticks,
            wait_before_use_ticks_random_range,
            wait_after_use_ticks,
            wait_after_use_ticks_random_range,
        }
    }
}

/// Represents the fixed move action.
///
/// Converted from [`ActionMove`] without fields used by [`Rotator`].
#[derive(Clone, Copy, Debug)]
pub struct Move {
    pub position: Position,
    pub wait_after_move_ticks: u32,
}

impl From<ActionMove> for Move {
    fn from(
        ActionMove {
            position,
            wait_after_move_millis,
            ..
        }: ActionMove,
    ) -> Self {
        Self {
            position,
            wait_after_move_ticks: (wait_after_move_millis / MS_PER_TICK) as u32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct AutoMob {
    pub key: KeyBinding,
    pub link_key: Option<LinkKeyBinding>,
    pub count: u32,
    pub with: ActionKeyWith,
    pub wait_before_ticks: u32,
    pub wait_before_ticks_random_range: u32,
    pub wait_after_ticks: u32,
    pub wait_after_ticks_random_range: u32,
    pub position: Position,
    pub is_pathing: bool,
}

impl fmt::Display for AutoMob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.position.x, self.position.y)
    }
}

/// Represents a ping pong action.
///
/// This is a type of action that moves in one direction and spams a fixed key. Once the player hits
/// either edges determined by [`Self::bound`] or close enough, the action is completed.
/// The [`Rotator`] then rotates the next action in the reverse direction.
///
/// This action forces the player to always stay inside the bound.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct PingPong {
    pub key: KeyBinding,
    pub link_key: Option<LinkKeyBinding>,
    pub count: u32,
    pub with: ActionKeyWith,
    pub wait_before_ticks: u32,
    pub wait_before_ticks_random_range: u32,
    pub wait_after_ticks: u32,
    pub wait_after_ticks_random_range: u32,
    /// Bound of ping pong action.
    ///
    /// This bound is in player relative coordinate.
    pub bound: Rect,
    pub direction: PingPongDirection,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub enum PingPongDirection {
    #[cfg_attr(test, default)]
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct FamiliarsSwap {
    pub swappable_slots: SwappableFamiliars,
    pub swappable_rarities: Array<FamiliarRarity, 2>,
}

#[derive(Clone, Copy, Debug)]
pub struct Panic {
    pub to: PanicTo,
}

#[derive(Clone, Copy, Debug)]
pub enum PanicTo {
    Town,
    Channel,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub content: String,
}

#[derive(Clone, Copy, Debug)]
pub enum Booster {
    Vip,
    Hexa,
}

#[derive(Clone, Copy, Debug)]
pub struct UseBooster {
    pub kind: Booster,
}

#[derive(Clone, Copy, Debug)]
pub struct ExchangeBooster {
    pub amount: u32,
    pub all: bool,
}

/// Represents an action the [`Rotator`] can use.
#[derive(Clone, Debug, Display)]
pub enum PlayerAction {
    /// Fixed key action provided by the user.
    Key(Key),
    /// Fixed move action provided by the user.
    Move(Move),
    /// Solves rune action.
    SolveRune,
    /// Auto-mobbing action.
    #[strum(to_string = "AutoMob({0})")]
    AutoMob(AutoMob),
    /// Ping pong action.
    PingPong(PingPong),
    /// Swaps familiars action.
    FamiliarsSwap(FamiliarsSwap),
    /// Panics to town or another channel action.
    Panic(Panic),
    /// Chats in-game action.
    Chat(Chat),
    /// Use VIP or HEXA booster action.
    UseBooster(UseBooster),
    /// Exchange HEXA booster action.
    ExchangeBooster(ExchangeBooster),
    /// Unstucking by pressing ESC.
    Unstuck,
}

impl From<Action> for PlayerAction {
    fn from(action: Action) -> Self {
        match action {
            Action::Move(action) => PlayerAction::Move(action.into()),
            Action::Key(action) => PlayerAction::Key(action.into()),
        }
    }
}

#[macro_export]
macro_rules! transition_to_moving {
    ($player:expr, $moving:expr) => {{
        $player.state = Player::Moving($moving.dest, $moving.exact, $moving.intermediates);
        return;
    }};

    ($player:expr, $moving:expr, $block:block) => {{
        $block
        $player.state = Player::Moving($moving.dest, $moving.exact, $moving.intermediates);
        return;
    }};
}

#[macro_export]
macro_rules! transition_to_moving_if {
    ($player:expr, $moving:expr, $cond:expr) => {{
        if $cond {
            $player.state = Player::Moving($moving.dest, $moving.exact, $moving.intermediates);
            return;
        }
    }};

    ($player:expr, $moving:expr, $cond:expr, $block:block) => {{
        if $cond {
            $block
            $player.state = Player::Moving($moving.dest, $moving.exact, $moving.intermediates);
            return;
        }
    }};
}

#[macro_export]
macro_rules! transition_from_action {
    ($player:expr, $state:expr) => {{
        use $crate::{
            database::Position,
            player::{Key, PlayerAction},
        };

        match next_action(&$player.context).expect("has action") {
            PlayerAction::SolveRune
            | PlayerAction::PingPong(_)
            | PlayerAction::Move(_)
            | PlayerAction::Key(Key {
                position: Some(Position { .. }),
                ..
            }) => {
                $player.context.clear_unstucking(false);
            }
            _ => (),
        }
        $player.context.clear_action_completed();
        $player.state = $state;
        return;
    }};

    ($player:expr, $state:expr, $is_terminal:expr) => {{
        use $crate::database::Position;
        use $crate::player::Key;
        use $crate::player::PlayerAction;

        if $is_terminal {
            match next_action(&$player.context).expect("has action") {
                PlayerAction::SolveRune
                | PlayerAction::PingPong(_)
                | PlayerAction::Move(_)
                | PlayerAction::Key(Key {
                    position: Some(Position { .. }),
                    ..
                }) => {
                    $player.context.clear_unstucking(false);
                }
                _ => (),
            }
            $player.context.clear_action_completed();
        }
        $player.state = $state;
        return;
    }};
}

#[inline]
pub(super) fn next_action(context: &PlayerContext) -> Option<PlayerAction> {
    context
        .priority_action
        .clone()
        .or(context.normal_action.clone())
}

#[inline]
pub(super) fn update_from_ping_pong_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    ping_pong: PingPong,
    cur_pos: Point,
) {
    let direction = ping_pong.direction;
    let bound = ping_pong.bound;
    let hit_x_bound_edge = match direction {
        PingPongDirection::Left => cur_pos.x - bound.x <= 0,
        PingPongDirection::Right => cur_pos.x - bound.x - bound.width >= 0,
    };
    if hit_x_bound_edge {
        transition_from_action!(player, Player::Idle);
    }

    release_arrow_keys(resources);
    let minimap_width = match minimap_state {
        Minimap::Idle(idle) => idle.bbox.width,
        _ => unreachable!(),
    };
    let y = cur_pos.y; // y doesn't matter in ping pong
    let moving = match direction {
        PingPongDirection::Left => Player::Moving(Point::new(0, y), false, None),
        PingPongDirection::Right => Player::Moving(Point::new(minimap_width, y), false, None),
    };
    transition!(player, moving)
}

/// Checks proximity in [`PlayerAction::AutoMob`] for transitioning to [`Player::UseKey`].
///
/// If `state` is [`Some`], this function will attempt to use key when auto mob is currently
/// pathing.
///
/// This is common logics shared with other contextual states when there is auto mob action.
#[inline]
pub(super) fn update_from_auto_mob_action(
    resources: &Resources,
    player: &mut PlayerEntity,
    minimap_state: Minimap,
    mob: AutoMob,
    x_distance: i32,
    x_direction: i32,
    y_distance: i32,
) {
    let direction = match x_direction {
        direction if direction > 0 => ActionKeyDirection::Right,
        direction if direction < 0 => ActionKeyDirection::Left,
        _ => ActionKeyDirection::Any,
    };
    let should_terminate =
        x_distance <= AUTO_MOB_USE_KEY_X_THRESHOLD && y_distance <= AUTO_MOB_USE_KEY_Y_THRESHOLD;
    let should_check_pathing = matches!(
        player.state,
        Player::DoubleJumping(_) | Player::Adjusting(_)
    );

    transition_if!(
        player,
        Player::UseKey(UseKey::from_auto_mob(mob, direction, should_terminate)),
        should_check_pathing
            && player
                .context
                .auto_mob_pathing_should_use_key(resources, minimap_state),
        {
            release_arrow_keys(resources);
        }
    );
    transition_if!(
        player,
        Player::UseKey(UseKey::from_auto_mob(mob, direction, should_terminate)),
        should_terminate,
        {
            player.context.last_known_direction = ActionKeyDirection::Any;
            release_arrow_keys(resources);
        }
    );
}

fn release_arrow_keys(resources: &Resources) {
    resources.input.send_key_up(KeyKind::Down);
    resources.input.send_key_up(KeyKind::Up);
    resources.input.send_key_up(KeyKind::Left);
    resources.input.send_key_up(KeyKind::Right);
}
