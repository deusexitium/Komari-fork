use std::{cell::RefCell, fmt::Debug, rc::Rc, sync::Arc};

use crate::{
    GameTemplate, Localization,
    detect::{
        CASH_SHOP_TEMPLATE, CHANGE_CHANNEL_TEMPLATE, FAMILIAR_LEVEL_BUTTON_TEMPLATE,
        FAMILIAR_SAVE_BUTTON_TEMPLATE, FAMILIAR_SETUP_BUTTON_TEMPLATE, POPUP_CANCEL_NEW_TEMPLATE,
        POPUP_CANCEL_OLD_TEMPLATE, POPUP_CONFIRM_TEMPLATE, POPUP_END_CHAT_TEMPLATE,
        POPUP_NEXT_TEMPLATE, POPUP_OK_NEW_TEMPLATE, POPUP_OK_OLD_TEMPLATE, POPUP_YES_TEMPLATE,
        TIMER_TEMPLATE, to_base64_from_mat,
    },
    ecs::Resources,
    utils::{self, DatasetDir},
};

/// A service for handling localization-related incoming requests.
pub trait LocalizationService: Debug {
    /// Retrieves the default base64-encoded PNG for template `template`.
    fn template(&self, template: GameTemplate) -> String;

    /// Updates the currently in use [`Localization`] with new `localization`.
    fn update_localization(&mut self, localization: Localization);

    /// Saves the currently captured image to the `datasets` folder.
    fn save_capture_image(&self, resources: &Resources, is_grayscale: bool);
}

#[derive(Debug)]
pub struct DefaultLocalizationService {
    localization: Rc<RefCell<Arc<Localization>>>,
}

impl DefaultLocalizationService {
    pub fn new(localization: Rc<RefCell<Arc<Localization>>>) -> Self {
        Self { localization }
    }
}

impl LocalizationService for DefaultLocalizationService {
    fn template(&self, template: GameTemplate) -> String {
        match template {
            GameTemplate::CashShop => to_base64_from_mat(&CASH_SHOP_TEMPLATE),
            GameTemplate::ChangeChannel => to_base64_from_mat(&CHANGE_CHANNEL_TEMPLATE),
            GameTemplate::Timer => to_base64_from_mat(&TIMER_TEMPLATE),
            GameTemplate::PopupConfirm => to_base64_from_mat(&POPUP_CONFIRM_TEMPLATE),
            GameTemplate::PopupYes => to_base64_from_mat(&POPUP_YES_TEMPLATE),
            GameTemplate::PopupNext => to_base64_from_mat(&POPUP_NEXT_TEMPLATE),
            GameTemplate::PopupEndChat => to_base64_from_mat(&POPUP_END_CHAT_TEMPLATE),
            GameTemplate::PopupOkNew => to_base64_from_mat(&POPUP_OK_NEW_TEMPLATE),
            GameTemplate::PopupOkOld => to_base64_from_mat(&POPUP_OK_OLD_TEMPLATE),
            GameTemplate::PopupCancelNew => to_base64_from_mat(&POPUP_CANCEL_NEW_TEMPLATE),
            GameTemplate::PopupCancelOld => to_base64_from_mat(&POPUP_CANCEL_OLD_TEMPLATE),
            GameTemplate::FamiliarsLevelSort => to_base64_from_mat(&FAMILIAR_LEVEL_BUTTON_TEMPLATE),
            GameTemplate::FamiliarsSaveButton => to_base64_from_mat(&FAMILIAR_SAVE_BUTTON_TEMPLATE),
            GameTemplate::FamiliarsSetupButton => {
                to_base64_from_mat(&FAMILIAR_SETUP_BUTTON_TEMPLATE)
            }
        }
        .expect("convert successfully")
    }

    fn update_localization(&mut self, localization: Localization) {
        *self.localization.borrow_mut() = Arc::new(localization);
    }

    fn save_capture_image(&self, resources: &Resources, is_grayscale: bool) {
        if let Some(detector) = resources.detector.as_ref() {
            if is_grayscale {
                utils::save_image_to_default(detector.grayscale_mat(), DatasetDir::Root);
            } else {
                utils::save_image_to_default(detector.mat(), DatasetDir::Root);
            }
        }
    }
}
