use super::{Player, timeout::Timeout};
use crate::{
    bridge::KeyKind,
    ecs::Resources,
    player::{
        Booster, PlayerEntity, next_action,
        timeout::{Lifecycle, next_timeout_lifecycle},
    },
    transition, transition_from_action, transition_if,
};

/// States of using booster.
#[derive(Debug, Clone, Copy)]
enum State {
    Using(Timeout),
    Confirming(Timeout),
    Completing {
        timeout: Timeout,
        completed: bool,
        failed: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct UsingBooster {
    state: State,
    kind: Booster,
}

impl UsingBooster {
    pub fn new(kind: Booster) -> Self {
        Self {
            state: State::Using(Timeout::default()),
            kind,
        }
    }
}

/// Updates [`Player::UsingBooster`] contextual state.
pub fn update_using_booster_state(resources: &Resources, player: &mut PlayerEntity) {
    let Player::UsingBooster(mut using) = player.state else {
        panic!("state is not using booster")
    };
    let key = match using.kind {
        Booster::Vip => player.context.config.vip_booster_key,
        Booster::Hexa => player.context.config.hexa_booster_key,
    };

    match using.state {
        State::Using(_) => update_using(resources, &mut using, key),
        State::Confirming(_) => update_confirming(resources, &mut using),
        State::Completing { .. } => update_completing(resources, &mut using),
    };

    let player_next_state = if matches!(
        using.state,
        State::Completing {
            completed: true,
            ..
        }
    ) {
        Player::Idle
    } else {
        Player::UsingBooster(using)
    };
    let is_terminal = matches!(player_next_state, Player::Idle);
    if is_terminal {
        if matches!(using.state, State::Completing { failed: true, .. }) {
            player.context.track_booster_fail_count(using.kind);
        } else {
            player.context.clear_booster_fail_count(using.kind);
        }
    }

    match next_action(&player.context) {
        Some(_) => transition_from_action!(player, player_next_state, is_terminal),
        None => transition!(
            player,
            Player::Idle // Force cancel if it is not initiated from an action
        ),
    }
}

fn update_using(resources: &Resources, using: &mut UsingBooster, key: KeyKind) {
    const PRESS_KEY_AT: u32 = 30;

    let State::Using(timeout) = using.state else {
        panic!("using booster state is not using")
    };

    match next_timeout_lifecycle(timeout, 60) {
        Lifecycle::Started(timeout) => transition!(using, State::Using(timeout)),
        Lifecycle::Ended => transition_if!(
            using,
            State::Confirming(Timeout::default()),
            State::Completing {
                timeout: Timeout::default(),
                completed: false,
                failed: true
            },
            resources.detector().detect_admin_visible()
        ),
        Lifecycle::Updated(timeout) => transition!(using, State::Using(timeout), {
            if timeout.current == PRESS_KEY_AT {
                resources.input.send_key(key);
            }
        }),
    }
}

fn update_confirming(resources: &Resources, using: &mut UsingBooster) {
    let State::Confirming(timeout) = using.state else {
        panic!("using booster state is not confirming")
    };

    match next_timeout_lifecycle(timeout, 30) {
        Lifecycle::Started(timeout) => transition!(using, State::Confirming(timeout), {
            resources.input.send_key(KeyKind::Left);
        }),
        Lifecycle::Ended => transition!(
            using,
            State::Completing {
                timeout: Timeout::default(),
                completed: false,
                failed: false
            },
            {
                resources.input.send_key(KeyKind::Enter);
            }
        ),
        Lifecycle::Updated(timeout) => {
            transition!(using, State::Confirming(timeout), {
                if timeout.current == 15 {
                    resources.input.send_key(KeyKind::Left);
                }
            });
        }
    }
}

fn update_completing(resources: &Resources, using: &mut UsingBooster) {
    let State::Completing {
        timeout,
        completed,
        failed,
    } = using.state
    else {
        panic!("using booster state is not completing")
    };

    match next_timeout_lifecycle(timeout, 20) {
        Lifecycle::Started(timeout) | Lifecycle::Updated(timeout) => {
            transition!(
                using,
                State::Completing {
                    timeout,
                    completed,
                    failed
                }
            )
        }
        Lifecycle::Ended => transition!(
            using,
            State::Completing {
                timeout,
                completed: true,
                failed,
            },
            {
                if resources.detector().detect_esc_settings() {
                    resources.input.send_key(KeyKind::Esc);
                }
            }
        ),
    }
}

#[cfg(test)]
mod tests {}
