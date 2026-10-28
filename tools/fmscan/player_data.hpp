#pragma once
#include "entities.hpp"
#include "ppm.hpp"

struct AttributeSpec {
  const char *name;
  size_t offset;
  unsigned scale;
};
inline const std::vector<AttributeSpec> ATTRIBUTE_SPECS{
    {"crossing", 0x17, 5},         {"dribbling", 0x18, 5},
    {"finishing", 0x19, 5},        {"heading", 0x1a, 5},
    {"long_shots", 0x1b, 5},       {"off_the_ball", 0x1d, 5},
    {"passing", 0x1e, 5},          {"penalty_taking", 0x1f, 5},
    {"tackling", 0x20, 5},         {"vision", 0x21, 5},
    {"anticipation", 0x28, 5},     {"decisions", 0x29, 5},
    {"positioning", 0x2b, 5},      {"first_touch", 0x2d, 5},
    {"technique", 0x2e, 5},        {"flair", 0x31, 5},
    {"corners", 0x32, 5},          {"teamwork", 0x33, 5},
    {"work_rate", 0x34, 5},        {"acceleration", 0x39, 5},
    {"free_kicks", 0x3a, 5},       {"strength", 0x3b, 5},
    {"stamina", 0x3c, 5},          {"pace", 0x3d, 5},
    {"jumping_reach", 0x3e, 5},    {"leadership", 0x3f, 5},
    {"balance", 0x41, 5},          {"bravery", 0x42, 5},
    {"aggression", 0x44, 5},       {"agility", 0x45, 5},
    {"natural_fitness", 0x49, 5},  {"determination", 0x4a, 5},
    {"composure", 0x4b, 5},        {"concentration", 0x4c, 5},
    {"marking", 0x1c, 5},          {"handling", 0x22, 5},
    {"aerial_reach", 0x23, 5},     {"command_of_area", 0x24, 5},
    {"communication", 0x25, 5},    {"kicking", 0x26, 5},
    {"throwing", 0x27, 5},         {"one_on_ones", 0x2a, 5},
    {"reflexes", 0x2c, 5},         {"long_throws", 0x35, 5},
    {"eccentricity", 0x36, 5},     {"rushing_out", 0x37, 5},
    {"punching", 0x38, 5},         {"dirtiness", 0x40, 5},
    {"consistency", 0x43, 5},      {"important_matches", 0x46, 5},
    {"injury_proneness", 0x47, 5}, {"versatility", 0x48, 5},
    {"adaptability", 0x1b0, 1},    {"ambition", 0x1b1, 1},
    {"loyalty", 0x1b2, 1},         {"pressure", 0x1b3, 1},
    {"professionalism", 0x1b4, 1}, {"sportsmanship", 0x1b5, 1},
    {"temperament", 0x1b6, 1},     {"controversy", 0x1b7, 1}};
inline const std::array<const char *, 15> POSITION_KEYS{
    "gk", "sw",  "dl",  "dc",  "dr", "dm",  "ml", "mc",
    "mr", "aml", "amc", "amr", "st", "wbl", "wbr"};
template <class T, size_t N>
inline T field(const std::array<uint8_t, N> &b, size_t off) {
  T v;
  memcpy(&v, b.data() + off, sizeof(T));
  return v;
}
struct PlayerData {
  U record = 0;
  uint32_t index = 0, uid = 0;
  std::string name;
  std::vector<unsigned> attributes;
  std::array<unsigned, 17> positions{};
  uint16_t ca = 0, pa = 0;
  std::optional<uint32_t> wage, expiry;
  std::optional<unsigned> number;
  Json json() const {
    Json j;
    j.num("id", index);
    j.num("unique_id", uid);
    j.str("name", name);
    j.raw("current_ability", ca ? std::to_string(ca) : "null");
    j.raw("potential_ability", pa ? std::to_string(pa) : "null");
    Json attrs;
    for (size_t i = 0; i < attributes.size(); ++i)
      attrs.num(ATTRIBUTE_SPECS[i].name, attributes[i]);
    j.raw("attributes", attrs.done());
    Json pos;
    for (size_t i = 0; i < 15; ++i)
      pos.num(POSITION_KEYS[i], positions[i]);
    pos.num("left_foot", positions[15]);
    pos.num("right_foot", positions[16]);
    j.raw("positions", pos.done());
    Json c;
    c.raw("wage_per_week", wage ? std::to_string(*wage) : "null");
    c.str("currency", "GBP");
    c.raw("expiry_year", expiry ? std::to_string(*expiry >> 16) : "null");
    c.raw("expiry_packed", expiry ? quote(hex(*expiry)) : "null");
    c.raw("squad_number", number ? std::to_string(*number) : "null");
    for (auto k : {"release_clause", "squad_status", "happiness"})
      c.raw(k, "null");
    j.raw("contract", c.done());
    return j;
  }
  U checksum() const {
    U n = U(index) + uid + ca + pa + wage.value_or(0) + expiry.value_or(0) +
          number.value_or(0);
    for (auto v : attributes)
      n += v;
    for (auto v : positions)
      n += v;
    for (unsigned char c : name)
      n += c;
    return n;
  }
};
inline bool native_vtable(const Memory &m, U vt) {
  auto r = m.region(vt, 8);
  U function = 0;
  if (!r || r->perms[0] != 'r' || r->perms[1] == 'w' || !m.read(vt, function))
    return false;
  auto code = m.region(function);
  return code && code->perms[2] == 'x';
}
inline PlayerData read_player_data(const Memory &m, U record, uint32_t index,
                                   uint32_t uid) {
  std::array<uint8_t, 0x200> b{};
  if (!m.read(record, b))
    throw std::runtime_error("unreadable player snapshot");
  auto region = m.region(record, b.size());
  if (!region || !region->heap() || field<uint32_t>(b, 0x148) != index ||
      field<uint32_t>(b, 0x14c) != uid || field<uint32_t>(b, 0x150) != uid ||
      !native_vtable(m, field<U>(b, 0)) ||
      !native_vtable(m, field<U>(b, 0x140)))
    throw std::runtime_error("native snapshot identity/type mismatch");
  PlayerData p;
  p.record = record;
  p.index = index;
  p.uid = uid;
  for (auto spec : ATTRIBUTE_SPECS) {
    unsigned raw = b[spec.offset];
    if (raw > spec.scale * 20)
      throw std::runtime_error("invalid native attribute: " +
                               std::string(spec.name));
    p.attributes.push_back(spec.scale == 5 ? std::max(1u, (raw + 2) / 5) : raw);
  }
  for (size_t i = 0; i < 15; ++i) {
    p.positions[i] = b[8 + i];
    if (!p.positions[i] || p.positions[i] > 20)
      throw std::runtime_error("invalid native position");
  }
  for (size_t i = 0; i < 2; ++i) {
    unsigned v = b[0x2f + i];
    if (!v || v > 100 || v % 5)
      throw std::runtime_error("invalid native foot");
    p.positions[15 + i] = v / 5;
  }
  p.ca = field<uint16_t>(b, 0x11c);
  p.pa = field<uint16_t>(b, 0x11e);
  if (p.ca > 200)
    p.ca = 0;
  if (p.pa > 200)
    p.pa = 0;
  for (U off : {0x190, 0x198}) {
    U ref = field<U>(b, off);
    auto s = pooled_name(m, ref);
    if (ref && s.empty())
      throw std::runtime_error("invalid pooled player name");
    if (!s.empty()) {
      if (!p.name.empty())
        p.name += ' ';
      p.name += s;
    }
  }
  if (p.name.empty())
    throw std::runtime_error("missing player name");
  U contract = field<U>(b, 0x1e8);
  if (contract) {
    std::array<uint8_t, 0x60> c{};
    if (!m.read(contract, c) || !native_vtable(m, field<U>(c, 0)) ||
        field<U>(c, 8) != record + 0x140)
      throw std::runtime_error("invalid native contract ownership/type");
    uint32_t expiry = field<uint32_t>(c, 0x48), year = expiry >> 16,
             day = expiry & 0x1ff;
    bool leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    if (year < 2000 || year > 2200 || !day || day > (leap ? 366u : 365u))
      throw std::runtime_error("invalid contract date");
    p.wage = field<uint32_t>(c, 0x20);
    p.expiry = expiry;
    if (c[0x5d] != 255)
      p.number = c[0x5d];
  }
  std::array<uint32_t, 3> check{};
  U check_contract = 0;
  if (!m.read(record + 0x148, check) ||
      check != std::array<uint32_t, 3>{index, uid, uid} ||
      !m.read(record + 0x1e8, check_contract) || check_contract != contract)
    throw std::runtime_error("player changed during snapshot");
  return p;
}
inline std::pair<U, ScanStats> locate_player_data(const Memory &m,
                                                  const Args &a, uint32_t index,
                                                  uint32_t uid) {
  std::vector<U> found;
  auto stats = scan(m, Near(a), [&](U base, const uint8_t *b, size_t n) {
    for (size_t i = 0; i + 8 <= n; i += 8) {
      if (word(b + i) != (U(uid) << 32 | index))
        continue;
      uint32_t duplicate = 0;
      if (i + 12 <= n)
        memcpy(&duplicate, b + i + 8, 4);
      else if (!m.read(base + i + 8, duplicate))
        continue;
      if (duplicate != uid || base + i < 0x148)
        continue;
      U candidate = base + i - 0x148;
      try {
        read_player_data(m, candidate, index, uid);
        found.push_back(candidate);
      } catch (const std::runtime_error &) {
      }
    }
  });
  require_scan(stats);
  if (found.size() != 1)
    throw std::runtime_error(
        "expected one native player snapshot candidate, found " +
        std::to_string(found.size()));
  return {found[0], stats};
}
inline U elapsed_ns(std::chrono::steady_clock::time_point start) {
  return std::chrono::duration_cast<std::chrono::nanoseconds>(
             std::chrono::steady_clock::now() - start)
      .count();
}
inline int cmd_player_data(const Memory &m, const Args &a) {
  a.allow({"--record", "--iterations", "--near"}, 4);
  U index = number(a.pos[2]), uid = number(a.pos[3]),
    iterations = a.val("--iterations", 1);
  if (!index || index > INT32_MAX || !uid || uid > UINT32_MAX || !iterations ||
      iterations > 1000000)
    throw std::runtime_error("invalid player-data identity/iterations");
  U record = 0, scan_ns = 0;
  ScanStats stats;
  if (a.has("--record"))
    record = number(a.get("--record"), 16);
  else {
    auto start = std::chrono::steady_clock::now();
    auto result = locate_player_data(m, a, index, uid);
    record = result.first;
    stats = result.second;
    scan_ns = elapsed_ns(start);
  }
  PlayerData p;
  U checksum = 0;
  auto start = std::chrono::steady_clock::now();
  for (U i = 0; i < iterations; ++i) {
    p = read_player_data(m, record, index, uid);
    checksum += p.checksum();
  }
  U ns = elapsed_ns(start);
  Json j;
  j.str("command", "player-data");
  j.addr("record", record);
  j.raw("player", p.json().done());
  j.num("scan_ns", scan_ns);
  j.num("read_ns", ns);
  j.num("iterations", iterations);
  j.num("checksum", checksum);
  j.num("scanned_bytes", stats.bytes);
  j.num("unreadable_chunks", stats.unreadable);
  emit(a, j, j.done());
  return 0;
}
inline Json editor_player_data(const Memory &m, const NativeCatalog &catalog,
                               const NativeEntity &e) {
  auto p = read_player_data(m, e.record, e.index, e.uid);
  Json j = p.json();
  U nationality = 0;
  std::optional<NativeEntity> nation;
  if (m.read(e.record + 0x1a8, nationality))
    nation = read_entity(m, catalog, nationality, "nation");
  j.raw("nationality", nation ? quote(nation->name) : "null");
  EntityRelations links{m, catalog, {}, {}, {}, {}};
  links.player(e);
  bool has_club = false;
  // Resolve club separately through the validated contract-team chain, never
  // the birth/reserve club slot.
  U contract = 0, team = 0, club = 0, owner = 0;
  if (m.read(e.record + 0x1e8, contract) && m.read(contract + 8, owner) &&
      owner == e.record + 0x140 && m.read(contract + 0x10, team) &&
      catalog.at(m, team, "team") && m.read(team + 0x30, club)) {
    auto c = read_entity(m, catalog, club, "club");
    if (c) {
      j.str("club_name", c->name);
      j.num("club_id", c->index);
      has_club = true;
    }
  }
  if (!has_club) {
    j.raw("club_name", "null");
    j.raw("club_id", "null");
  }
  for (auto k : {"age", "condition", "sharpness", "morale", "morale_label",
                 "is_injured", "is_suspended"})
    j.raw(k, "null");
  std::string best;
  for (size_t i = 0; i < 15; ++i)
    if (p.positions[i] >= 18) {
      if (!best.empty())
        best += ", ";
      best += POSITION_KEYS[i];
    }
  j.str("position", best);
  j.str("position_status", "derived_from_aptitudes_at_least_18");
  j.raw("preferred_moves", ppm_json(read_ppm(m, e.record)).done());
  j.raw("related", links.json().done());
  j.str("unavailable_reason", "null fields lack a verified screen-independent "
                              "native layout; no UI defaults substituted");
  j.str("verification",
        "native schema parity only; no in-game screen comparison performed");
  return j;
}
inline void print_player_editor(const Memory &m, const NativeCatalog &catalog,
                                const NativeEntity &e) {
  auto p = read_player_data(m, e.record, e.index, e.uid);
  std::cout << p.name << "\nIndex: " << p.index << "  UID: " << p.uid
            << "\nRecord: " << hex(p.record) << "\nCA: "
            << (p.ca ? std::to_string(p.ca) : "Unavailable")
            << "  PA: " << (p.pa ? std::to_string(p.pa) : "Unavailable") << "\n";
  U nat = 0;
  std::optional<NativeEntity> nationality;
  if (m.read(e.record + 0x1a8, nat))
    nationality = read_entity(m, catalog, nat, "nation");
  std::cout << "Nationality: "
            << (nationality ? nationality->name : "Unavailable") << "\n";
  EntityRelations links{m, catalog, {}, {}, {}, {}};
  links.player(e);
  for (const auto &linked : links.records) {
    if (linked.kind != "team")
      std::cout << linked.kind << ": " << linked.name << " (index "
                << linked.index << ", UID " << linked.uid << ")\n";
  }
  if (!links.unavailable.empty())
    std::cout << "Unavailable links: " << array(links.unavailable) << "\n";
  std::cout << "\nAttributes (1..20)\n";
  for (size_t i = 0; i < p.attributes.size(); ++i)
    std::cout << std::left << std::setw(22) << ATTRIBUTE_SPECS[i].name
              << p.attributes[i] << "\n";
  std::cout << "\nPosition aptitudes (1..20)\n";
  for (size_t i = 0; i < 15; ++i)
    std::cout << std::left << std::setw(22) << POSITION_KEYS[i]
              << p.positions[i] << "\n";
  std::cout
      << "Left foot: " << p.positions[15] << "  Right foot: " << p.positions[16]
      << "\n\nPreferred moves (bit table incomplete): "
      << ppm_text(read_ppm(m, e.record)) << "\n\nContract (parent club)\n"
      << "Weekly wage GBP: "
      << (p.wage ? std::to_string(*p.wage) : "Unavailable") << "\nExpiry year: "
      << (p.expiry ? std::to_string(*p.expiry >> 16) : "Unavailable")
      << "\nSquad number: "
      << (p.number ? std::to_string(*p.number) : "None/unavailable") << "\n"
      << "Unavailable: age, condition, sharpness, morale/label, "
         "injury/suspension status, release clause, squad status, happiness.\n"
      << "Native schema read; no in-game display comparison.\n";
}
