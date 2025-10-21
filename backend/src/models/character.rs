// TODO: Move related character models here

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString};

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum ExchangeHexaBoosterCondition {
    #[default]
    None,
    Full,
    AtLeastOne,
}
