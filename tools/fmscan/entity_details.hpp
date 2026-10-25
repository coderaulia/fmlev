#pragma once
#include "player_data.hpp"
#include <tuple>

// Numeric layouts below were traced/corroborated on this exact native DLL
// build. A new build must be investigated; range checks alone do not establish
// semantics.
inline bool entity_layout_build(const Memory &m, const NativeCatalog &c) {
  uint32_t pe = 0, stamp = 0;
  return m.read(c.base + 0x3c, pe) && pe <= CHUNK &&
         m.read(c.base + pe + 8, stamp) && stamp == 0x6a229e8a;
}
struct Details {
  Json fields;
  std::vector<std::string> evidence, unavailable;
  std::vector<std::pair<std::string, std::string>> text;
  void missing(const std::string &key, const std::string &why) {
    fields.raw(key, "null");
    Json j;
    j.str("field", key);
    j.str("reason", why);
    unavailable.push_back(j.done());
    text.push_back({key, "Unavailable: " + why});
  }
  void source(const std::string &key, U address, const std::string &proof) {
    Json j;
    j.str("field", key);
    j.addr("address", address);
    j.str("evidence", proof);
    evidence.push_back(j.done());
  }
  template <class T>
  void number(const Memory &m, const std::string &key, U address, int64_t low,
              int64_t high, const std::string &proof, bool allowed = true) {
    T value{};
    if (!allowed) {
      missing(key, "native layout/build/ownership not verified");
      return;
    }
    if (!m.read(address, value) || int64_t(value) < low ||
        int64_t(value) > high) {
      missing(key, "unreadable or outside native field range");
      return;
    }
    fields.raw(key, std::to_string(value));
    text.push_back({key, std::to_string(value)});
    source(key, address, proof);
  }
  void string(const std::string &key, const std::string &value, U address) {
    if (value.empty()) {
      missing(key, "native string missing/invalid");
      return;
    }
    fields.str(key, value);
    text.push_back({key, value});
    source(key, address,
           "native owning pointer/string chain; length-prefixed UTF-8");
  }
  Json done() const {
    Json j = fields;
    j.raw("field_evidence", array(evidence));
    j.raw("unavailable", array(unavailable));
    j.str("verification",
          "guarded native reads; no game-screen or save/reload comparison");
    j.str("write_support", "none");
    return j;
  }
};
inline bool typed_native(const Memory &m, U obj, const std::string &name) {
  std::array<uint32_t, 3> id{};
  auto r = m.region(obj, 20);
  return r && r->heap() && m.read(obj + 8, id) && id[0] <= INT32_MAX && id[1] &&
         id[1] == id[2] && object_type(m, obj).name == name;
}
inline U pointer_field(const Memory &m, U obj, U off) {
  U p = 0;
  m.read(obj + off, p);
  return p;
}
inline std::string direct_string(const Memory &m, U obj, U off) {
  return native_string(m, pointer_field(m, obj, off));
}
inline U first_team(const Memory &m, U container,
                    const std::string &team_type) {
  std::array<U, 3> v{};
  if (!m.read(container + 0x18, v) || !v[0] || v[1] < v[0] || v[2] < v[1] ||
      v[1] - v[0] < 8 || v[2] - v[0] > 0x10000 || (v[1] - v[0]) % 8 ||
      !m.region(v[0], v[1] - v[0]))
    return 0;
  U team = pointer_field(m, v[0], 0);
  return typed_native(m, team, team_type) &&
                 pointer_field(m, team, 0x30) == container
             ? team
             : 0;
}
inline void club_details(const Memory &m, const NativeCatalog &catalog,
                         const NativeEntity &e, Details &d) {
  bool build = entity_layout_build(m, catalog);
  U club = e.address;
  U team = first_team(m, club, ".?AVTEAM@db@@");
  auto nation = read_entity(m, catalog, pointer_field(m, club, 0xd8), "nation");
  auto comp = team ? read_entity(m, catalog, pointer_field(m, team, 0x50),
                                 "competition")
                   : std::nullopt;
  d.string("nation_name", nation ? nation->name : "", club + 0xd8);
  d.string("league_name", comp ? comp->name : "", team ? team + 0x50 : 0);
  d.number<uint16_t>(m, "reputation", team + 0xa8, 1, 10000,
                     "native club serializer: first team +0xa8", build && team);
  d.number<uint16_t>(m, "world_reputation", club + 0x166, 1, 10000,
                     "native world_reputation serializer at RVA 0xef49ba",
                     build);
  U cint = pointer_field(m, club, 0x100), abbrev = pointer_field(m, cint, 0);
  auto r = m.region(cint, 0x128);
  auto short_name = native_string(m, abbrev);
  uint16_t founded = 0;
  bool cint_ok = build && r && r->heap() && !short_name.empty() &&
                 short_name.size() <= 32 && m.read(cint + 0xd0, founded) &&
                 founded >= 1800 && founded <= 2200;
  d.string("abbreviation", cint_ok ? short_name : "", cint);
  d.number<uint16_t>(m, "year_founded", cint + 0xd0, 1800, 2200,
                     "reference layout plus native owner/string/year guards",
                     cint_ok);
  for (auto [key, off, proof] :
       std::vector<std::tuple<std::string, U, std::string>>{
           {"training_facilities", 0x118,
            "native training serializer RVA 0xea6f6d"},
           {"youth_facilities", 0x123,
            "native youth_facilities serializer RVA 0xea7159"},
           {"junior_coaching", 0x124,
            "native youth_coaching serializer RVA 0xea6b05"},
           {"youth_recruitment", 0x125,
            "native youth_recruitment serializer RVA 0xea6e9b"}})
    d.number<uint8_t>(m, key, cint + off, 1, 20, proof, cint_ok);
  U finance = pointer_field(m, club, 0x150);
  auto type = object_type(m, finance).name;
  bool owned =
      finance && pointer_field(m, finance, 8) == club &&
      (type == ".?AVCLUB_FINANCE@db@@" || type == ".?AVCLUB_FINANCE_BASE@db@@");
  bool full = owned && type == ".?AVCLUB_FINANCE@db@@" && build;
  d.fields.str("currency", "GBP");
  d.fields.str("wage_period", "unverified; raw native GBP values only");
  d.number<int32_t>(m, "balance", finance + 0x14, INT32_MIN, INT32_MAX,
                    "existing native finance map with club back-pointer",
                    owned);
  for (auto [key, off] : std::vector<std::pair<std::string, U>>{
           {"transfer_budget", 0x7cc}, {"allocated_transfer_budget", 0x7d0}})
    d.number<int32_t>(m, key, finance + off, INT32_MIN, INT32_MAX,
                      "existing native finance map with club back-pointer",
                      full);
  for (auto [key, off] : std::vector<std::pair<std::string, U>>{
           {"wage_budget_raw_gbp", 0x810},
           {"max_wage_allowance_raw_gbp", 0x814},
           {"wage_used_raw_gbp", 0x818},
           {"wage_with_bonuses_raw_gbp", 0x81c}})
    d.number<int32_t>(m, key, finance + off, 0, INT32_MAX,
                      "reference layout plus full-finance "
                      "RTTI/build/back-pointer guards; wage period unverified",
                      full);
  d.number<uint32_t>(m, "transfer_retention_pct", finance + 0x7d8, 0, 100,
                     "reference layout plus full-finance ownership guards",
                     full);
  d.number<uint8_t>(m, "corporate_facilities", finance + 0x8b5, 1, 20,
                    "native corporate_facilities serializer RVA 0xef46c9",
                    full);
  U stadium = team ? pointer_field(m, team, 0x78) : 0;
  bool stadium_ok = typed_native(m, stadium, ".?AVSTADIUM@db@@");
  auto stadium_name = stadium_ok ? direct_string(m, stadium, 0x40) : "";
  if (stadium_name.empty() && stadium_ok)
    stadium_name = pooled_name(m, pointer_field(m, stadium, 0x40));
  d.string("stadium_name", stadium_name, stadium + 0x40);
  d.number<uint32_t>(m, "stadium_capacity", stadium + 0x7c, 1, 500000,
                     "native stadium_capacity serializer RVA "
                     "0x204dca9/0x204dcbc; main team stadium +0x78",
                     build && stadium_ok);
  d.number<uint32_t>(
      m, "stadium_expansion_cap", stadium + 0x8c, 1, 500000,
      "native expansion_capacity serializer RVA 0x204db63/0x204db79",
      build && stadium_ok);
  U training_ground = pointer_field(m, club, 0x140);
  d.string("training_ground_name",
           typed_native(m, training_ground, ".?AVSTADIUM@db@@")
               ? direct_string(m, training_ground, 0x40)
               : "",
           training_ground + 0x40);
  for (auto key :
       {"data_analysis_facilities", "sugar_daddy_type", "board_patience",
        "fan_confidence", "board_confidence", "affiliates"})
    d.missing(key, "no verified native layout for this editor field");
  if (pointer_field(m, club, 0x100) != cint ||
      pointer_field(m, club, 0x150) != finance)
    throw std::runtime_error("club ownership changed during details read");
}
inline Json national_container_details(const Memory &m, U nation, U container,
                                       bool build) {
  Details d;
  bool valid = build &&
               typed_native(m, container, ".?AVNATIONAL_TEAM_CONTAINER@db@@") &&
               pointer_field(m, container, 0x170) == nation;
  if (valid)
    d.fields.addr("container_address", container);
  d.number<uint16_t>(
      m, "youth_rating", container + 0x864, 1, 200,
      "native youth_rating serializer RVA 0x1198b0d/0x1198b56; WORD not byte",
      valid);
  U team = valid ? first_team(m, container, ".?AVNATIONAL_TEAM@db@@") : 0;
  d.number<uint16_t>(m, "reputation", team + 0xa8, 1, 10000,
                     "native national-container reputation serializer RVA "
                     "0x11992eb; first national team",
                     team != 0);
  for (auto key :
       {"game_importance", "fa_financial_power", "fifa_rank", "fifa_points"})
    d.missing(key, "gender-specific native field not yet mapped");
  return d.done();
}
inline void nation_details(const Memory &m, const NativeCatalog &catalog,
                           const NativeEntity &e, Details &d) {
  U nation = e.address;
  bool build = entity_layout_build(m, catalog);
  d.string("three_letter_code", direct_string(m, nation, 0x28), nation + 0x28);
  U continent = pointer_field(m, nation, 0xf0);
  bool valid = typed_native(m, continent, ".?AVCONTINENT@db@@");
  d.string("continent", valid ? direct_string(m, continent, 0x18) : "",
           continent + 0x18);
  d.fields.raw("men", national_container_details(
                          m, nation, pointer_field(m, nation, 0x108), build)
                          .done());
  d.fields.raw("women", national_container_details(
                            m, nation, pointer_field(m, nation, 0x110), build)
                            .done());
  d.number<uint8_t>(
      m, "economic_factor", nation + 0x1f0, 1, 20,
      "native NATION serializer tag Necf -> BYTE +0x1f0 at RVA 0xf7c7cb",
      build);
  d.number<uint8_t>(m, "state_of_development_native_code", nation + 0x1ec, 0,
                    255,
                    "native NATION serializer tag Nsod -> BYTE +0x1ec at RVA "
                    "0xf7cd6b; enum labels not verified",
                    build);
  for (auto key :
       {"reputation", "youth_rating", "game_importance", "fa_financial_power"})
    d.missing(key, "use gender-specific men/women fields; nation has no single "
                   "verified value");
  for (auto key : {"state_of_development", "tax_rate", "corruption_index",
                   "youth_facilities_avg", "youth_coaching_avg",
                   "youth_recruitment_avg", "population", "spoken_languages"})
    d.missing(key, "no verified screen-independent native layout");
}
inline void competition_details(const Memory &m, const NativeCatalog &catalog,
                                const NativeEntity &e, Details &d) {
  U comp = e.address;
  bool build = entity_layout_build(m, catalog);
  d.string("short_name", direct_string(m, comp, 0x48), comp + 0x48);
  d.string("abbreviation", direct_string(m, comp, 0x50), comp + 0x50);
  auto nation = read_entity(m, catalog, pointer_field(m, comp, 0x60), "nation");
  d.string("nation_name", nation ? nation->name : "", comp + 0x60);
  U continent = pointer_field(m, comp, 0x58);
  bool valid = typed_native(m, continent, ".?AVCONTINENT@db@@");
  d.string("continent", valid ? direct_string(m, continent, 0x18) : "",
           continent + 0x18);
  d.number<uint16_t>(m, "reputation", comp + 0x188, 1, 200,
                     "native competition reputation serializer RVA 0xfa4651; "
                     "native scale 1..200",
                     build);
  d.fields.str(
      "reputation_scale",
      "1..200 native; not multiplied into the UI fixture's 1..10000 scale");
  U act = pointer_field(m, comp, 0xb0), stage = 0, vector = 0, entry = 0;
  uint8_t index = 255;
  bool gate = false;
  if (build && object_type(m, act).name == ".?AVACTUAL_COMP@sicomps@@") {
    stage = pointer_field(m, act, 0x208);
    vector = pointer_field(m, stage, 0x1e0);
    m.read(act + 0x31, index);
    U begin = pointer_field(m, vector, 0x60),
      end = pointer_field(m, vector, 0x68);
    if (begin && end > begin && end - begin <= 0x10000 &&
        (end - begin) % 8 == 0 && index != 255 && index < (end - begin) / 8 &&
        m.region(begin, end - begin)) {
      entry = pointer_field(m, begin, U(index) * 8);
      gate = m.region(entry, 0xce) && m.region(entry, 0xce)->heap();
    }
  }
  d.number<uint8_t>(m, "stage_registration_gate", entry + 0x9a, 0, 1,
                    "existing native stage-rule map; stage gate only, manager "
                    "override not evaluated",
                    gate);
  if (gate) {
    d.fields.addr("rule_entry_address", entry);
    d.fields.num("stage_index", index);
  }
  for (auto key :
       {"max_substitutes", "max_bench_size", "substitution_windows",
        "var_active", "max_squad_size", "max_foreign_players",
        "min_homegrown_club", "min_homegrown_nation", "winner_prize_money",
        "tv_rights_pool", "yellow_cards_for_ban", "number_of_teams"})
    d.missing(key, "active rule/payout/season ownership not verified; editor "
                   "source nodes and UI caches not substituted");
}
inline Details entity_details(const Memory &m, const NativeCatalog &catalog,
                              const NativeEntity &e) {
  Details d;
  d.fields.str("kind", e.kind);
  U name_offset = e.kind == "club" ? 0xc0 : e.kind == "nation" ? 0x18 : 0x40;
  d.string("name", e.name, e.address + name_offset);
  d.fields.num("id", e.index);
  d.fields.num("unique_id", e.uid);
  d.fields.str("layout_build", entity_layout_build(m, catalog)
                                   ? "known PE timestamp 0x6a229e8a"
                                   : "unknown; new numeric layouts disabled");
  if (e.kind == "club")
    club_details(m, catalog, e, d);
  else if (e.kind == "nation")
    nation_details(m, catalog, e, d);
  else if (e.kind == "competition")
    competition_details(m, catalog, e, d);
  auto fresh = read_entity(m, catalog, e.address, e.kind);
  if (!fresh || fresh->index != e.index || fresh->uid != e.uid ||
      fresh->name != e.name)
    throw std::runtime_error("entity identity changed during details read");
  return d;
}
inline Json editor_entity_data(const Memory &m, const NativeCatalog &c,
                               const NativeEntity &e) {
  return entity_details(m, c, e).done();
}
inline void print_entity_editor(const Memory &m, const NativeCatalog &c,
                                const NativeEntity &e) {
  auto d = entity_details(m, c, e);
  std::cout << e.name << " (" << e.kind << ", index " << e.index << ", UID "
            << e.uid << ")\n";
  if (e.kind == "nation")
    std::cout << d.done().done() << "\n";
  else
    for (auto &[key, value] : d.text)
      std::cout << std::left << std::setw(30) << key << value << "\n";
}
