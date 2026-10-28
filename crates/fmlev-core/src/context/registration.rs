//! Live "register anytime" (see native_registration.rs).
use crate::error::{CoreError, Result};
use crate::native_registration::{set_registration_anytime, RegistrationState};
use super::GameContextManager;

impl GameContextManager {
    /// Allows (or restores) squad registration outside the window for the managed human and one
    /// competition, identified by its index and unique id.
    pub fn set_registration_anytime(&mut self, comp_index: i32, comp_uid: i32, enabled: bool) -> Result<RegistrationState> {
        let session = self.session.as_ref().filter(|s| s.bases.game_assembly_base.is_some())
            .ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let key = (comp_index, comp_uid);
        let original = self.registration_gates.get(&key).copied();
        let others_enabled = self.registration_gates.keys().any(|other| *other != key);
        let state = set_registration_anytime(&session.driver, comp_index, comp_uid, enabled, original, others_enabled)?;
        if enabled {
            self.registration_gates.insert(key, state.original);
        } else {
            self.registration_gates.remove(&(comp_index, comp_uid));
        }
        Ok(state)
    }
}

impl GameContextManager {
    /// Read-only registration gates for one competition (no writes).
    pub fn read_registration_gates(&self, comp_index: i32, comp_uid: i32) -> Result<crate::native_registration::RegistrationGates> {
        let session = self.session.as_ref().filter(|s| s.bases.game_assembly_base.is_some())
            .ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        crate::native_registration::read_registration_gates(&session.driver, comp_index, comp_uid)
    }
}
