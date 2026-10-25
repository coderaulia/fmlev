//! Native baselines for nation and competition editors. The UI fills controls from binding caches
//! and fallbacks, so a save must write only fields the user changed away from what was loaded,
//! and the editors must load the verified native values.
use crate::error::{CoreError, Result};
use crate::models::{CompetitionDetails, NationDetails};
use crate::native_competition::NativeCompetitionRecord;
use crate::native_nation::NativeNationRecord;
use super::GameContextManager;

fn nation_overlay(
    driver: &fmlev_mem::LinuxMemoryDriver,
    record: &NativeNationRecord,
    mut nation: NationDetails,
) -> NationDetails {
    if let Ok(v) = record.read_men_youth_rating(driver) {
        if (1..=200).contains(&v) { nation.youth_rating = v as u8; }
    }
    if let Ok(v) = record.read_men_reputation(driver) {
        if v > 0 { nation.reputation = v; }
    }
    if let Ok(v) = record.read_economic_factor(driver) {
        if v > 0 { nation.economic_factor = v; }
    }
    if let Ok(v) = record.read_state_of_development(driver) { nation.state_of_development = v; }
    if let Ok(v) = record.read_population(driver) {
        if v > 0 { nation.population = v; }
    }
    nation
}

impl GameContextManager {
    /// Reads verified native nation values into the registry copy (read-only on the game).
    pub fn read_nation_native(&mut self, nation_id: i32) -> Result<NationDetails> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let loaded = self.nation_registry.get(&nation_id).cloned()
            .or_else(|| (self.context.nation.id == nation_id).then(|| self.context.nation.clone()))
            .ok_or_else(|| CoreError::InvalidValue(format!("Nation {nation_id} is not loaded; open one of its pages in FM26")))?;
        let record = NativeNationRecord::discover(&session.driver, nation_id)?;
        let nation = nation_overlay(&session.driver, &record, loaded);
        self.nation_registry.insert(nation_id, nation.clone());
        if self.context.nation.id == nation_id { self.context.nation = nation.clone(); }
        Ok(nation)
    }

    /// Reads the verified native competition reputation into the registry copy.
    pub fn read_competition_native(&mut self, comp_id: i32) -> Result<CompetitionDetails> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let mut comp = self.competition_registry.get(&comp_id).cloned()
            .or_else(|| (self.context.competition.id == comp_id).then(|| self.context.competition.clone()))
            .ok_or_else(|| CoreError::InvalidValue(format!("Competition {comp_id} is not loaded; open one of its pages in FM26")))?;
        let record = NativeCompetitionRecord::discover(&session.driver, comp_id)?;
        let reputation = record.read_reputation(&session.driver)?;
        if reputation > 0 { comp.reputation = reputation; }
        self.competition_registry.insert(comp_id, comp.clone());
        if self.context.competition.id == comp_id { self.context.competition = comp.clone(); }
        Ok(comp)
    }

    pub(super) fn update_live_nation_checked(&mut self, nation: NationDetails) -> Result<()> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let nation_id = nation.id;
        let record = NativeNationRecord::discover(&session.driver, nation_id)?;
        let loaded = self.nation_registry.get(&nation_id).cloned().unwrap_or_else(|| nation.clone());
        let native = nation_overlay(&session.driver, &record, loaded.clone());
        let mut requested = nation.clone();
        macro_rules! keep_native {
            ($($f:ident),*) => {$( if requested.$f == loaded.$f { requested.$f = native.$f; } )*};
        }
        keep_native!(youth_rating, reputation, economic_factor, state_of_development);
        // Population has no in-game verification and the form default is a fallback: never written live.
        requested.population = native.population;
        record.write_nation_edits(&session.driver, &native, &requested)?;
        self.context.nation = nation.clone();
        self.nation_registry.insert(nation_id, nation);
        let _ = self.read_nation_native(nation_id);
        Ok(())
    }

    pub(super) fn update_live_competition_checked(&mut self, comp: CompetitionDetails) -> Result<()> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let comp_id = comp.id;
        let record = NativeCompetitionRecord::discover(&session.driver, comp_id)?;
        let loaded = self.competition_registry.get(&comp_id).cloned().unwrap_or_else(|| comp.clone());
        let mut native = loaded.clone();
        let reputation = record.read_reputation(&session.driver)?;
        if reputation > 0 { native.reputation = reputation; }
        let mut requested = comp.clone();
        if requested.reputation == loaded.reputation { requested.reputation = native.reputation; }
        record.write_competition_edits(&session.driver, &native, &requested)?;
        self.context.competition = comp.clone();
        self.competition_registry.insert(comp_id, comp);
        let _ = self.read_competition_native(comp_id);
        Ok(())
    }
}
