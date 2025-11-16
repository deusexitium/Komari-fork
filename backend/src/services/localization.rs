use std::{cell::RefCell, fmt::Debug, rc::Rc, sync::Arc};

use crate::{
    GameTemplate, Localization,
    detect::{
        CASH_SHOP_TEMPLATE, CHANGE_CHANNEL_TEMPLATE, FAMILIAR_LEVEL_BUTTON_TEMPLATE,
        FAMILIAR_SAVE_BUTTON_TEMPLATE, HEXA_BOOSTER_BUTTON_TEMPLATE, HEXA_CONVERT_BUTTON_TEMPLATE,
        HEXA_ERDA_CONVERSION_BUTTON_TEMPLATE, HEXA_MAX_BUTTON_TEMPLATE, POPUP_CANCEL_NEW_TEMPLATE,
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
        let template = match template {
            GameTemplate::CashShop => &CASH_SHOP_TEMPLATE,
            GameTemplate::ChangeChannel => &CHANGE_CHANNEL_TEMPLATE,
            GameTemplate::Timer => &TIMER_TEMPLATE,
            GameTemplate::PopupConfirm => &POPUP_CONFIRM_TEMPLATE,
            GameTemplate::PopupYes => &POPUP_YES_TEMPLATE,
            GameTemplate::PopupNext => &POPUP_NEXT_TEMPLATE,
            GameTemplate::PopupEndChat => &POPUP_END_CHAT_TEMPLATE,
            GameTemplate::PopupOkNew => &POPUP_OK_NEW_TEMPLATE,
            GameTemplate::PopupOkOld => &POPUP_OK_OLD_TEMPLATE,
            GameTemplate::PopupCancelNew => &POPUP_CANCEL_NEW_TEMPLATE,
            GameTemplate::PopupCancelOld => &POPUP_CANCEL_OLD_TEMPLATE,
            GameTemplate::FamiliarsLevelSort => &FAMILIAR_LEVEL_BUTTON_TEMPLATE,
            GameTemplate::FamiliarsSaveButton => &FAMILIAR_SAVE_BUTTON_TEMPLATE,
            GameTemplate::HexaErdaConversionButton => &HEXA_ERDA_CONVERSION_BUTTON_TEMPLATE,
            GameTemplate::HexaBoosterButton => &HEXA_BOOSTER_BUTTON_TEMPLATE,
            GameTemplate::HexaMaxButton => &HEXA_MAX_BUTTON_TEMPLATE,
            GameTemplate::HexaConvertButton => &HEXA_CONVERT_BUTTON_TEMPLATE,
        };

        to_base64_from_mat(template).expect("convert successfully")
    }

    fn update_localization(&mut self, localization: Localization) {
        *self.localization.borrow_mut() = Arc::new(localization);
    }

    fn save_capture_image(&self, resources: &Resources, is_grayscale: bool) {
        if let Some(detector) = resources.detector.as_ref() {
            if is_grayscale {
                utils::save_image_to_default(detector.grayscale(), DatasetDir::Root);
            } else {
                utils::save_image_to_default(detector.mat(), DatasetDir::Root);
            }
        }
    }
}
