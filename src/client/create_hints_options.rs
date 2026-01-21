use crate::protocol::HintStatus;

/// A builder for options for
/// [Client::create_hints_with_options](crate::Client::create_hints_with_options).
#[derive(Debug, Clone, Default)]
pub struct CreateHintsOptions {
    pub(crate) slot: Option<u32>,
    pub(crate) status: HintStatus,
}

impl CreateHintsOptions {
    /// Creates a [CreateHintsOptions] with all default option values.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the slot number for which the hints should be created. This is the
    /// slot whose world contains the locations being hinted.
    ///
    /// By default, this is the current player's slot
    pub fn slot(mut self, slot: u32) -> Self {
        self.slot = Some(slot);
        self
    }

    /// Sets the status for the newly-created hint.
    ///
    /// By default, this is [HintStatus::Unspecified].
    pub fn status(mut self, status: HintStatus) -> Self {
        self.status = status;
        self
    }
}
