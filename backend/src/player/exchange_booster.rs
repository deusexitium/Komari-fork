use opencv::core::Rect;
use rand_distr::num_traits::clamp;

use super::{Player, timeout::Timeout};
use crate::{
    array::Array,
    bridge::{KeyKind, MouseKind},
    ecs::Resources,
    player::{
        PlayerEntity, next_action,
        timeout::{Lifecycle, next_timeout_lifecycle},
    },
    transition, transition_from_action, transition_if, try_ok_transition, try_some_transition,
};

/// States of exchanging HEXA booster.
#[derive(Debug, Clone, Copy)]
enum State {
    OpenHexaMenu(Timeout),
    OpenExchangingMenu(Timeout, Rect),
    OpenBoosterMenu(Timeout, Rect),
    Exchanging(Timeout, Rect),
    Confirming(Timeout, Rect),
    Completing(Timeout, bool),
}

#[derive(Debug, Clone, Copy)]
pub struct ExchangingBooster {
    state: State,
    amount: Option<ExchangeAmount>,
}

impl ExchangingBooster {
    // TODO: These args should probably be represented by an enum?
    pub fn new(amount: u32, all: bool) -> Self {
        let amount = if all {
            None
        } else {
            let amount = clamp(amount, 1, 20);
            let str = amount.to_string();

            let mut keys =
                ExchangeAmountContent::from_iter([KeyKind::Backspace, KeyKind::Backspace]);
            let keys_from_chars = str.chars().map(|char| match char {
                '0' => KeyKind::Zero,
                '1' => KeyKind::One,
                '2' => KeyKind::Two,
                '3' => KeyKind::Three,
                '4' => KeyKind::Four,
                '5' => KeyKind::Five,
                '6' => KeyKind::Six,
                '7' => KeyKind::Seven,
                '8' => KeyKind::Eight,
                '9' => KeyKind::Nine,
                _ => unreachable!(),
            });
            for key in keys_from_chars {
                keys.push(key);
            }

            Some(ExchangeAmount { index: 0, keys })
        };

        Self {
            state: State::OpenHexaMenu(Timeout::default()),
            amount,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ExchangeAmount {
    index: usize,
    keys: ExchangeAmountContent,
}

impl ExchangeAmount {
    fn increment_index(mut self) -> ExchangeAmount {
        self.index += 1;
        self
    }
}

type ExchangeAmountContent = Array<KeyKind, 4>;

/// Updates [`Player::ExchangingBooster`] contextual state.
pub fn update_exchanging_booster_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::ExchangingBooster(mut exchanging) = player.state else {
        panic!("state is not exchanging booster")
    };

    match exchanging.state {
        State::OpenHexaMenu(_) => update_open_hexa_menu(resources, &mut exchanging),
        State::OpenExchangingMenu(_, _) => update_open_exchanging_menu(resources, &mut exchanging),
        State::OpenBoosterMenu(_, _) => update_open_booster_menu(resources, &mut exchanging),
        State::Exchanging(_, _) => update_exchanging(resources, &mut exchanging),
        State::Confirming(_, _) => update_confirming(resources, &mut exchanging),
        State::Completing(_, _) => update_completing(resources, &mut exchanging),
    };

    let player_next_state = if matches!(exchanging.state, State::Completing(_, true)) {
        Player::Idle
    } else {
        Player::ExchangingBooster(exchanging)
    };
    let is_terminal = matches!(player_next_state, Player::Idle);

    match next_action(&player.context) {
        Some(_) => transition_from_action!(player, player_next_state, is_terminal),
        None => transition!(
            player,
            Player::Idle // Force cancel if it is not initiated from an action
        ),
    }
}

fn update_open_hexa_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenHexaMenu(timeout) = exchanging.state else {
        panic!("exchanging booster state is not opening hexa menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            let (x, y) = try_some_transition!(
                exchanging,
                State::Completing(Timeout::default(), true),
                resources
                    .detector()
                    .detect_hexa_quick_menu()
                    .ok()
                    .map(bbox_click_point)
            );

            transition!(exchanging, State::OpenHexaMenu(timeout), {
                resources.input.send_mouse(x, y, MouseKind::Click);
            });
        }
        Lifecycle::Ended => {
            let bbox = try_ok_transition!(
                exchanging,
                State::Completing(Timeout::default(), false),
                resources.detector().detect_hexa_erda_conversion_button()
            );

            transition!(
                exchanging,
                State::OpenExchangingMenu(Timeout::default(), bbox)
            )
        }
        Lifecycle::Updated(timeout) => transition!(exchanging, State::OpenHexaMenu(timeout)),
    }
}

fn update_open_exchanging_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenExchangingMenu(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not opening exchanging menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            transition!(exchanging, State::OpenExchangingMenu(timeout, bbox), {
                let (x, y) = bbox_click_point(bbox);
                resources.input.send_mouse(x, y, MouseKind::Click);
            });
        }
        Lifecycle::Ended => {
            let bbox = try_ok_transition!(
                exchanging,
                State::Completing(Timeout::default(), false),
                resources.detector().detect_hexa_booster_button()
            );

            transition!(exchanging, State::OpenBoosterMenu(Timeout::default(), bbox))
        }
        Lifecycle::Updated(timeout) => {
            transition!(exchanging, State::OpenExchangingMenu(timeout, bbox))
        }
    }
}

fn update_open_booster_menu(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::OpenBoosterMenu(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not opening booster menu")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            transition!(exchanging, State::OpenBoosterMenu(timeout, bbox), {
                let (x, y) = bbox_click_point(bbox);
                resources.input.send_mouse(x, y, MouseKind::Click);
            })
        }
        Lifecycle::Ended => {
            let bbox = try_ok_transition!(
                exchanging,
                State::Completing(Timeout::default(), false),
                resources.detector().detect_hexa_max_button()
            );

            transition!(exchanging, State::Exchanging(Timeout::default(), bbox))
        }
        Lifecycle::Updated(timeout) => {
            transition!(exchanging, State::OpenBoosterMenu(timeout, bbox))
        }
    }
}

fn update_exchanging(resources: &Resources, exchanging: &mut ExchangingBooster) {
    const TYPE_INTERVAL: u32 = 10;

    let State::Exchanging(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not exchanging")
    };
    let amount = exchanging.amount;
    let max_timeout = if amount.is_none() { 20 } else { 60 };

    match next_timeout_lifecycle(timeout, max_timeout) {
        Lifecycle::Started(timeout) => {
            transition!(exchanging, State::Exchanging(timeout, bbox), {
                let (mut x, y) = bbox_click_point(bbox);
                if amount.is_none() {
                    x += 30; // Clicking the input box
                }

                resources.input.send_mouse(x, y, MouseKind::Click);
            })
        }
        Lifecycle::Ended => {
            let bbox = try_ok_transition!(
                exchanging,
                State::Completing(Timeout::default(), false),
                resources.detector().detect_hexa_convert_button()
            );

            transition!(exchanging, State::Confirming(Timeout::default(), bbox))
        }
        Lifecycle::Updated(timeout) => {
            if let Some(amount) = amount
                && timeout.current.is_multiple_of(TYPE_INTERVAL)
            {
                transition_if!(
                    exchanging,
                    State::Exchanging(timeout, bbox),
                    amount.index < amount.keys.len(),
                    {
                        exchanging.amount = Some(amount.increment_index());
                        resources.input.send_key(amount.keys[amount.index]);
                    }
                );
            }

            transition!(exchanging, State::Exchanging(timeout, bbox))
        }
    }
}
fn update_confirming(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::Confirming(timeout, bbox) = exchanging.state else {
        panic!("exchanging booster state is not confirming")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) => {
            transition!(exchanging, State::Confirming(timeout, bbox), {
                let (x, y) = bbox_click_point(bbox);

                resources.input.send_mouse(x, y, MouseKind::Click);
            })
        }
        Lifecycle::Ended => transition!(exchanging, State::Completing(Timeout::default(), false)),
        Lifecycle::Updated(timeout) => {
            transition!(exchanging, State::Confirming(timeout, bbox))
        }
    }
}

fn update_completing(resources: &Resources, exchanging: &mut ExchangingBooster) {
    let State::Completing(timeout, completed) = exchanging.state else {
        panic!("exchanging booster state is not completing")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            transition!(exchanging, State::Completing(timeout, completed))
        }
        Lifecycle::Ended => transition!(exchanging, State::Completing(timeout, true), {
            let detector = resources.detector();
            if detector.detect_esc_settings() {
                resources.input.send_key(KeyKind::Esc);
            }
        }),
    }
}

#[inline]
fn bbox_click_point(bbox: Rect) -> (i32, i32) {
    let x = bbox.x + bbox.width / 2;
    let y = bbox.y + bbox.height / 2;
    (x, y)
}

#[cfg(test)]
mod tests {}
