//! Storage management for canister upgrades.

use candid::Principal;

use crate::state::{self, ALLOWED_CALLERS};

/// Pre-upgrade hook to save state before canister upgrade
pub fn pre_upgrade() {}

/// Post-upgrade hook to restore state after canister upgrade
pub fn post_upgrade() {
    // Re-initialize state
    state::init();

    // Set caller whitelist
    ALLOWED_CALLERS.with_borrow_mut(|callers| {
        callers.insert(
            Principal::from_text("fsy6j-kps5i-iwcrb-3abrj-jjice-5tjlo-g4tz2-f3g7c-25b5f-hlug6-jae")
                .unwrap(),
            true,
        );
    })
}
