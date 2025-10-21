use std::fmt::Debug;

#[cfg(test)]
use mockall::{automock, concretize};
use strum::IntoEnumIterator;

use crate::rotator::Rotator;
use crate::{
    Action, Character, KeyBinding, Minimap, RotationMode, RotatorMode, Settings, buff::BuffKind,
    rotator::RotatorBuildArgs,
};
use crate::{
    ActionCondition, ActionConfigurationCondition, ActionKey, KeyBindingConfiguration, PotionMode,
};

/// A service to handle [`Rotator`]-related incoming requests.
#[cfg_attr(test, automock)]
pub trait RotatorService: Debug {
    /// Builds a new actions list to be used.
    fn update_actions<'a>(
        &mut self,
        minimap: Option<&'a Minimap>,
        preset: Option<String>,
        character: Option<&'a Character>,
    );

    /// Builds a new buffs list to be used.
    #[cfg_attr(test, concretize)]
    fn update_buffs(&mut self, character: Option<&Character>);

    /// Updates `rotator` with data from `minimap`, `character`, `settings`, and the currently
    /// in-use actions and buffs.
    fn apply<'a>(
        &self,
        rotator: &mut dyn Rotator,
        minimap: Option<&'a Minimap>,
        character: Option<&'a Character>,
        settings: &Settings,
    );
}

// TODO: Whether to use Rc<RefCell<Rotator>> like Settings
#[derive(Debug, Default)]
pub struct DefaultRotatorService {
    actions: Vec<Action>,
    buffs: Vec<(BuffKind, KeyBinding)>,
}

impl RotatorService for DefaultRotatorService {
    fn update_actions<'a>(
        &mut self,
        minimap: Option<&'a Minimap>,
        preset: Option<String>,
        character: Option<&'a Character>,
    ) {
        let character_actions = character.map(actions_from).unwrap_or_default();
        let minimap_actions = minimap
            .zip(preset)
            .and_then(|(minimap, preset)| minimap.actions.get(&preset).cloned())
            .unwrap_or_default();

        self.actions = [character_actions, minimap_actions].concat();
    }

    #[cfg_attr(test, concretize)]
    fn update_buffs(&mut self, character: Option<&Character>) {
        self.buffs = character.map(buffs_from).unwrap_or_default();
    }

    fn apply<'a>(
        &self,
        rotator: &mut dyn Rotator,
        minimap: Option<&'a Minimap>,
        character: Option<&'a Character>,
        settings: &Settings,
    ) {
        let mode = rotator_mode_from(minimap);
        let reset_normal_actions_on_erda = minimap
            .map(|minimap| minimap.actions_any_reset_on_erda_condition)
            .unwrap_or_default();
        let familiar_essence_key = character
            .map(|character| character.familiar_essence_key.key)
            .unwrap_or_default();
        let elite_boss_behavior = character
            .map(|character| character.elite_boss_behavior)
            .unwrap_or_default();
        let elite_boss_behavior_key = character
            .map(|character| character.elite_boss_behavior_key)
            .unwrap_or_default();
        let hexa_booster_exchange_condition = character
            .map(|character| character.hexa_booster_exchange_condition)
            .unwrap_or_default();
        let hexa_booster_exchange_amount = character
            .map(|character| character.hexa_booster_exchange_amount)
            .unwrap_or(1);
        let hexa_booster_exchange_all = character
            .map(|character| character.hexa_booster_exchange_all)
            .unwrap_or_default();
        let enable_using_vip_booster = character
            .map(|character| character.vip_booster_key.enabled)
            .unwrap_or_default();
        let enable_using_hexa_booster = character
            .map(|character| character.hexa_booster_key.enabled)
            .unwrap_or_default();
        let args = RotatorBuildArgs {
            mode,
            actions: &self.actions,
            buffs: &self.buffs,
            familiar_essence_key,
            familiar_swappable_slots: settings.familiars.swappable_familiars,
            familiar_swappable_rarities: &settings.familiars.swappable_rarities,
            familiar_swap_check_millis: settings.familiars.swap_check_millis,
            elite_boss_behavior,
            elite_boss_behavior_key,
            hexa_booster_exchange_condition,
            hexa_booster_exchange_amount,
            hexa_booster_exchange_all,
            enable_panic_mode: settings.enable_panic_mode,
            enable_rune_solving: settings.enable_rune_solving,
            enable_familiars_swapping: settings.familiars.enable_familiars_swapping,
            enable_reset_normal_actions_on_erda: reset_normal_actions_on_erda,
            enable_using_vip_booster,
            enable_using_hexa_booster,
        };

        rotator.build_actions(args);
    }
}

#[inline]
fn rotator_mode_from(minimap: Option<&Minimap>) -> RotatorMode {
    minimap
        .map(|minimap| match minimap.rotation_mode {
            RotationMode::StartToEnd => RotatorMode::StartToEnd,
            RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
            RotationMode::AutoMobbing => RotatorMode::AutoMobbing(
                minimap.rotation_mobbing_key,
                minimap.rotation_auto_mob_bound,
            ),
            RotationMode::PingPong => RotatorMode::PingPong(
                minimap.rotation_mobbing_key,
                minimap.rotation_ping_pong_bound,
            ),
        })
        .unwrap_or_default()
}

fn actions_from(character: &Character) -> Vec<Action> {
    fn make_key_action(key: KeyBinding, millis: u64, count: u32) -> Action {
        Action::Key(ActionKey {
            key,
            count,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        })
    }

    let mut vec = Vec::new();

    if let KeyBindingConfiguration { key, enabled: true } = character.feed_pet_key {
        vec.push(make_key_action(
            key,
            character.feed_pet_millis,
            character.feed_pet_count,
        ));
    }

    if let KeyBindingConfiguration { key, enabled: true } = character.potion_key
        && let PotionMode::EveryMillis(millis) = character.potion_mode
    {
        vec.push(make_key_action(key, millis, 1));
    }

    let mut iter = character.actions.clone().into_iter().peekable();
    while let Some(action) = iter.next() {
        if !action.enabled || matches!(action.condition, ActionConfigurationCondition::Linked) {
            continue;
        }

        vec.push(action.into());
        while let Some(next) = iter.peek() {
            if !matches!(next.condition, ActionConfigurationCondition::Linked) {
                break;
            }

            vec.push((*next).into());
            iter.next();
        }
    }

    vec
}

fn buffs_from(character: &Character) -> Vec<(BuffKind, KeyBinding)> {
    BuffKind::iter()
        .filter_map(|kind| {
            let enabled_key = match kind {
                BuffKind::Rune => None, // Internal buff
                BuffKind::Familiar => character
                    .familiar_buff_key
                    .enabled
                    .then_some(character.familiar_buff_key.key),
                BuffKind::SayramElixir => character
                    .sayram_elixir_key
                    .enabled
                    .then_some(character.sayram_elixir_key.key),
                BuffKind::AureliaElixir => character
                    .aurelia_elixir_key
                    .enabled
                    .then_some(character.aurelia_elixir_key.key),
                BuffKind::ExpCouponX2 => character
                    .exp_x2_key
                    .enabled
                    .then_some(character.exp_x2_key.key),
                BuffKind::ExpCouponX3 => character
                    .exp_x3_key
                    .enabled
                    .then_some(character.exp_x3_key.key),
                BuffKind::ExpCouponX4 => character
                    .exp_x4_key
                    .enabled
                    .then_some(character.exp_x4_key.key),
                BuffKind::BonusExpCoupon => character
                    .bonus_exp_key
                    .enabled
                    .then_some(character.bonus_exp_key.key),
                BuffKind::LegionLuck => character
                    .legion_luck_key
                    .enabled
                    .then_some(character.legion_luck_key.key),
                BuffKind::LegionWealth => character
                    .legion_wealth_key
                    .enabled
                    .then_some(character.legion_wealth_key.key),
                BuffKind::WealthAcquisitionPotion => character
                    .wealth_acquisition_potion_key
                    .enabled
                    .then_some(character.wealth_acquisition_potion_key.key),
                BuffKind::ExpAccumulationPotion => character
                    .exp_accumulation_potion_key
                    .enabled
                    .then_some(character.exp_accumulation_potion_key.key),
                BuffKind::SmallWealthAcquisitionPotion => character
                    .small_wealth_acquisition_potion_key
                    .enabled
                    .then_some(character.small_wealth_acquisition_potion_key.key),
                BuffKind::SmallExpAccumulationPotion => character
                    .small_exp_accumulation_potion_key
                    .enabled
                    .then_some(character.small_exp_accumulation_potion_key.key),
                BuffKind::ForTheGuild => character
                    .for_the_guild_key
                    .enabled
                    .then_some(character.for_the_guild_key.key),
                BuffKind::HardHitter => character
                    .hard_hitter_key
                    .enabled
                    .then_some(character.hard_hitter_key.key),
                BuffKind::ExtremeRedPotion => character
                    .extreme_red_potion_key
                    .enabled
                    .then_some(character.extreme_red_potion_key.key),
                BuffKind::ExtremeBluePotion => character
                    .extreme_blue_potion_key
                    .enabled
                    .then_some(character.extreme_blue_potion_key.key),
                BuffKind::ExtremeGreenPotion => character
                    .extreme_green_potion_key
                    .enabled
                    .then_some(character.extreme_green_potion_key.key),
                BuffKind::ExtremeGoldPotion => character
                    .extreme_gold_potion_key
                    .enabled
                    .then_some(character.extreme_gold_potion_key.key),
            };
            Some(kind).zip(enabled_key)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;
    use std::collections::HashSet;

    use strum::IntoEnumIterator;

    use super::*;
    use crate::{ActionCondition, ActionConfiguration, ActionConfigurationCondition, ActionKey};
    use crate::{
        Bound, EliteBossBehavior, FamiliarRarity, KeyBindingConfiguration, SwappableFamiliars,
        rotator::MockRotator,
    };

    #[test]
    fn update_rotator_mode() {
        let mut minimap = Minimap {
            rotation_auto_mob_bound: Bound {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            },
            rotation_ping_pong_bound: Bound {
                x: 1,
                y: 1,
                width: 1,
                height: 1,
            },
            ..Default::default()
        };
        let character = Character::default();
        let service = DefaultRotatorService::default();

        for mode in RotationMode::iter() {
            minimap.rotation_mode = mode;
            let mut rotator = MockRotator::new();
            rotator
                .expect_build_actions()
                .withf(move |args| {
                    let mut key_bound = None;
                    let original_mode = match args.mode {
                        RotatorMode::StartToEnd => RotationMode::StartToEnd,
                        RotatorMode::StartToEndThenReverse => RotationMode::StartToEndThenReverse,
                        RotatorMode::AutoMobbing(key, bound) => {
                            key_bound = Some((key, bound));
                            RotationMode::AutoMobbing
                        }
                        RotatorMode::PingPong(key, bound) => {
                            key_bound = Some((key, bound));
                            RotationMode::PingPong
                        }
                    };
                    let key_bound_match = match key_bound {
                        Some((key, bound)) => {
                            let bound_match = if original_mode == RotationMode::AutoMobbing {
                                bound == minimap.rotation_auto_mob_bound
                            } else {
                                bound == minimap.rotation_ping_pong_bound
                            };
                            key == minimap.rotation_mobbing_key && bound_match
                        }
                        None => true,
                    };

                    mode == original_mode && key_bound_match
                })
                .once()
                .return_const(());

            service.apply(
                &mut rotator,
                Some(&minimap),
                Some(&character),
                &Settings::default(),
            );
        }
    }

    #[test]
    fn update_with_buffs() {
        let buffs = vec![(BuffKind::SayramElixir, KeyBinding::F1)];

        let buffs_clone = buffs.clone();
        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(move |args| args.buffs == buffs_clone)
            .once()
            .return_const(());

        let service = DefaultRotatorService {
            buffs,
            ..Default::default()
        };
        service.apply(&mut rotator, None, None, &Settings::default());
    }

    #[test]
    fn update_with_familiar_essence_key() {
        let character = Character {
            familiar_essence_key: KeyBindingConfiguration {
                key: KeyBinding::Z,
                enabled: true,
            },
            ..Default::default()
        };

        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(|args| args.familiar_essence_key == KeyBinding::Z)
            .once()
            .return_const(());

        let service = DefaultRotatorService::default();
        service.apply(&mut rotator, None, Some(&character), &Settings::default());
    }

    #[test]
    fn update_with_familiar_swap_config() {
        let mut settings = Settings::default();
        settings.familiars.swappable_familiars = SwappableFamiliars::SecondAndLast;
        settings.familiars.swappable_rarities =
            HashSet::from_iter([FamiliarRarity::Epic, FamiliarRarity::Rare]);
        settings.familiars.swap_check_millis = 5000;
        settings.familiars.enable_familiars_swapping = true;

        let settings_clone = settings.clone();
        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(move |args| {
                args.familiar_swappable_slots == SwappableFamiliars::SecondAndLast
                    && args.familiar_swappable_rarities == &settings.familiars.swappable_rarities
                    && args.familiar_swap_check_millis == 5000
                    && args.enable_familiars_swapping
            })
            .once()
            .return_const(());

        let service = DefaultRotatorService::default();
        service.apply(&mut rotator, None, None, &settings_clone);
    }

    #[test]
    fn update_with_elite_boss_behavior() {
        let character = Character {
            elite_boss_behavior: EliteBossBehavior::CycleChannel,
            elite_boss_behavior_key: KeyBinding::X,
            ..Default::default()
        };

        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(|args| {
                args.elite_boss_behavior == EliteBossBehavior::CycleChannel
                    && args.elite_boss_behavior_key == KeyBinding::X
            })
            .once()
            .return_const(());

        let service = DefaultRotatorService::default();
        service.apply(&mut rotator, None, Some(&character), &Settings::default());
    }

    #[test]
    fn update_with_reset_normal_actions_on_erda() {
        let minimap = Minimap {
            actions_any_reset_on_erda_condition: true,
            ..Default::default()
        };

        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(|args| args.enable_reset_normal_actions_on_erda)
            .once()
            .return_const(());

        let service = DefaultRotatorService::default();
        service.apply(&mut rotator, Some(&minimap), None, &Settings::default());
    }

    #[test]
    fn update_with_panic_mode_and_rune_solving() {
        let settings = Settings {
            enable_panic_mode: true,
            enable_rune_solving: true,
            ..Default::default()
        };

        let mut rotator = MockRotator::new();
        rotator
            .expect_build_actions()
            .withf(|args| args.enable_panic_mode && args.enable_rune_solving)
            .once()
            .return_const(());

        let service = DefaultRotatorService::default();
        service.apply(&mut rotator, None, None, &settings);
    }

    #[test]
    fn update_combine_actions_and_fixed_actions() {
        let actions = vec![
            Action::Key(ActionKey {
                key: KeyBinding::A,
                ..Default::default()
            }),
            Action::Key(ActionKey {
                key: KeyBinding::B,
                ..Default::default()
            }),
        ];
        let character = Character {
            actions: vec![
                ActionConfiguration {
                    key: KeyBinding::C,
                    enabled: true,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::D,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::E,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::F,
                    enabled: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut minimap = Minimap::default();
        minimap.actions.insert("preset".to_string(), actions);
        let mut service = DefaultRotatorService::default();

        service.update_actions(Some(&minimap), Some("preset".to_string()), Some(&character));

        assert_matches!(
            service.actions.as_slice(),
            [
                Action::Key(ActionKey {
                    key: KeyBinding::C,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::D,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::E,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::F,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::A,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::B,
                    ..
                }),
            ]
        );
    }

    #[test]
    fn update_include_actions_while_fixed_actions_disabled() {
        let actions = vec![
            Action::Key(ActionKey {
                key: KeyBinding::A,
                ..Default::default()
            }),
            Action::Key(ActionKey {
                key: KeyBinding::B,
                ..Default::default()
            }),
        ];
        let character = Character {
            actions: vec![
                ActionConfiguration {
                    key: KeyBinding::C,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::D,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::E,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::F,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut minimap = Minimap::default();
        minimap.actions.insert("preset".to_string(), actions);
        let mut service = DefaultRotatorService::default();

        service.update_actions(Some(&minimap), Some("preset".to_string()), Some(&character));

        assert_matches!(
            service.actions.as_slice(),
            [
                Action::Key(ActionKey {
                    key: KeyBinding::A,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::B,
                    ..
                }),
            ]
        );
    }

    #[test]
    fn update_character_actions_only() {
        let character = Character {
            actions: vec![
                ActionConfiguration {
                    key: KeyBinding::C,
                    enabled: true,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::D,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::E,
                    condition: ActionConfigurationCondition::Linked,
                    ..Default::default()
                },
                ActionConfiguration {
                    key: KeyBinding::F,
                    enabled: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut service = DefaultRotatorService::default();

        service.update_actions(None, None, Some(&character));

        assert_matches!(
            service.actions.as_slice(),
            [
                Action::Key(ActionKey {
                    key: KeyBinding::C,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::D,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::E,
                    condition: ActionCondition::Linked,
                    ..
                }),
                Action::Key(ActionKey {
                    key: KeyBinding::F,
                    ..
                }),
            ]
        );
    }
}
