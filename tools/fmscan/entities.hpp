#pragma once
#include "catalog.hpp"

struct NativeEntity {
    std::string kind, name; U address = 0, record = 0; uint32_t index = 0, uid = 0;
    const NativeType* type = nullptr;
};
inline Json editor_player_data(const Memory&, const NativeCatalog&, const NativeEntity&);
inline void print_player_editor(const Memory&, const NativeCatalog&, const NativeEntity&);
inline Json editor_entity_data(const Memory&, const NativeCatalog&, const NativeEntity&);
inline void print_entity_editor(const Memory&, const NativeCatalog&, const NativeEntity&);
inline Json editor_staff_data(const Memory&, const NativeEntity&);
inline void print_staff_editor(const Memory&, const NativeEntity&);
// Staff (ACTUAL_NON_PLAYER) records are found by their +0x100 sub-object; `address` is that sub-object.
inline constexpr U STAFF_SUBOBJECT = 0x100, STAFF_SECOND_SUBOBJECT = 0x128;
inline std::optional<NativeEntity> read_staff(const Memory& m, const NativeCatalog& catalog, U address) {
    auto type = catalog.at(m, address, "staff", false);
    if (!type || type->subobject_offset != STAFF_SUBOBJECT || address < STAFF_SUBOBJECT) return {};
    auto second = catalog.at(m, address + (STAFF_SECOND_SUBOBJECT - STAFF_SUBOBJECT), "staff", false);
    if (!second || second->subobject_offset != STAFF_SECOND_SUBOBJECT) return {};
    std::array<uint32_t, 3> ids{};
    if (!m.read(address + 8, ids) || ids[0] > INT32_MAX || !ids[1] || ids[1] != ids[2]) return {};
    std::string name;
    for (U off : {0x50, 0x58}) {
        U ref = 0; if (!m.read(address + off, ref)) return {};
        auto part = pooled_name(m, ref); if (ref && part.empty()) return {};
        if (!part.empty()) { if (!name.empty()) name += " "; name += part; }
    }
    if (name.empty()) return {};
    return NativeEntity{"staff", name, address, address - STAFF_SUBOBJECT, ids[0], ids[1], type};
}
inline std::optional<NativeEntity> read_entity(const Memory& m, const NativeCatalog& catalog, U address, const std::string& kind) {
    if (kind == "staff") return read_staff(m, catalog, address);
    auto type = catalog.at(m, address, kind); if (!type) return {};
    // NativePlayerRecord's calibrated attribute/person view is embedded at ACTUAL_PLAYER+0x148.
    U record = kind == "player" ? address + 0x148 : address;
    U identity = kind == "player" ? record + 0x148 : address + 8;
    std::array<uint32_t, 3> ids{};
    if (!m.read(identity, ids) || ids[0] > INT32_MAX || !ids[1] || (kind != "team" && ids[1] != ids[2]) || (kind == "team" && !ids[2])) return {};
    std::string name;
    if (kind == "player") {
        auto person = catalog.at(m, record + 0x140, "player", false);
        if (!person || person->subobject_offset != 0x288 || person->rtti.name != type->rtti.name) return {};
        std::array<uint8_t, 15> positions{}; uint8_t left = 0, right = 0; U player_vt = 0, function = 0;
        if (!m.read(record, player_vt) || !m.read(player_vt, function)) return {};
        auto vt_region = m.region(player_vt, 8), code = m.region(function);
        if (!vt_region || vt_region->perms[1] == 'w' || !code || code->perms[2] != 'x') return {};
        if (!m.read(record + 8, positions) || !m.read(record + 0x2f, left) || !m.read(record + 0x30, right) ||
            !left || left > 100 || left % 5 || !right || right > 100 || right % 5 ||
            !std::all_of(positions.begin(), positions.end(), [](uint8_t p) { return p >= 1 && p <= 20; })) return {};
        for (U off : {0x190, 0x198}) {
            U ref = 0; if (!m.read(record + off, ref)) return {};
            auto part = pooled_name(m, ref); if (ref && part.empty()) return {};
            if (!part.empty()) { if (!name.empty()) name += " "; name += part; }
        }
    } else if (kind != "team") {
        U off = kind == "club" ? 0xc0 : kind == "nation" ? 0x18 : 0x40, p = 0;
        if (!m.read(address + off, p)) return {};
        name = native_string(m, p);
    }
    if (kind != "team" && name.empty()) return {};
    return NativeEntity{kind, name, address, record, ids[0], ids[1], type};
}
inline Json entity_json(const NativeEntity& e) {
    Json j; j.str("kind", e.kind); j.str("name", e.name); j.num("index", e.index); j.num("uid", e.uid);
    j.addr("address", e.address); j.addr("record", e.record); j.raw("rtti", type_json(e.type->rtti).done());
    j.str("identity_status", "rtti_and_duplicate_uid_validated"); return j;
}
struct EntityRelations {
    const Memory& m; const NativeCatalog& catalog;
    std::vector<std::string> nodes, edges, unavailable;
    std::set<U> seen;
    std::vector<NativeEntity> records{};
    std::optional<NativeEntity> link(U source, U offset, const std::string& kind, const std::string& label) {
        U target = 0;
        auto entity = m.read(source + offset, target) && target ? read_entity(m, catalog, target, kind) : std::nullopt;
        Json edge; edge.str("relation", label); edge.addr("source", source); edge.num("offset", offset); edge.addr("target", target);
        if (!entity) { edge.str("status", target ? "invalid_or_unmapped_target" : "absent"); unavailable.push_back(edge.done()); return {}; }
        edge.str("status", "native_pointer_and_identity_validated"); edges.push_back(edge.done());
        if (seen.insert(target).second) { nodes.push_back(entity_json(*entity).done()); records.push_back(*entity); }
        return entity;
    }
    void club(const NativeEntity& c) {
        link(c.address, 0xd8, "nation", "club_nation");
        // A club's first-team vector provides team -> current domestic competition.
        std::array<U, 3> v{};
        if (!m.read(c.address + 0x18, v) || !v[0] || v[1] < v[0] || v[2] < v[1] ||
            v[1] - v[0] < 8 || v[2] - v[0] > 0x10000 || (v[1] - v[0]) % 8 || !m.region(v[0], v[1] - v[0])) {
            Json j; j.str("relation", "club_first_team"); j.str("status", "invalid_or_empty_team_vector"); unavailable.push_back(j.done()); return;
        }
        U first = 0, owner = 0;
        if (!m.read(v[0], first) || !m.read(first + 0x30, owner) || owner != c.address) {
            Json j; j.str("relation", "club_first_team_owner"); j.str("status", "back_pointer_mismatch"); unavailable.push_back(j.done()); return;
        }
        Json edge; edge.str("relation", "club_teams_vector"); edge.addr("source", c.address); edge.num("offset", 0x18); edge.addr("target", v[0]);
        edge.str("status", "bounds_and_team_owner_validated"); edges.push_back(edge.done());
        auto t = link(v[0], 0, "team", "club_first_team");
        if (t) team(*t);
    }
    void team(const NativeEntity& t) {
        auto comp = link(t.address, 0x50, "competition", "team_domestic_competition");
        if (comp) link(comp->address, 0x60, "nation", "competition_nation");
    }
    void player(const NativeEntity& p) {
        U contract = 0, owner = 0;
        if (!m.read(p.record + 0x1e8, contract) || !contract || !m.read(contract + 8, owner) || owner != p.record + 0x140 ||
            object_type(m, contract).name != ".?AVFULL_CONTRACT@db@@") {
            Json j; j.str("relation", "parent_contract"); j.str("status", "absent_or_invalid_contract"); unavailable.push_back(j.done()); return;
        }
        Json edge; edge.str("relation", "parent_contract"); edge.addr("source", p.record); edge.num("offset", 0x1e8); edge.addr("target", contract);
        edge.str("status", "rtti_and_person_back_pointer_validated"); edges.push_back(edge.done());
        auto t = link(contract, 0x10, "team", "contract_team");
        if (t) {
            auto c = link(t->address, 0x30, "club", "contract_team_club");
            if (c) link(c->address, 0xd8, "nation", "club_nation");
            team(*t);
        }
    }
    Json json() const {
        Json j; j.raw("entities", array(nodes)); j.raw("edges", array(edges)); j.raw("unavailable", array(unavailable));
        j.str("scope", "parent_contract_club_and_current_domestic_competition; loan_destination_not_inferred"); return j;
    }
};
inline int cmd_entity(const Memory& m, const Args& a) {
    a.allow({"--name", "--id", "--uid", "--max", "--near", "--related", "--editor"}, 3);
    const auto& kind = a.pos[2];
    if (kind != "player" && kind != "club" && kind != "nation" && kind != "competition" && kind != "staff") throw std::runtime_error("entity kind must be player|club|nation|competition|staff");
    if (kind == "staff" && a.has("--related")) throw std::runtime_error("--related is not supported for staff");
    U max = a.val("--max", 20), id = a.val("--id", 0), uid = a.val("--uid", 0);
    if (!max || max > 1000 || id > INT32_MAX || uid > UINT32_MAX || (a.has("--name") && a.get("--name").empty())) throw std::runtime_error("invalid entity selector or max (1..1000)");
    NativeCatalog catalog(m);
    std::set<U> tables; for (auto& [vt, type] : catalog.tables) if (type.kind == kind && type.subobject_offset == (kind == "staff" ? STAFF_SUBOBJECT : 0)) tables.insert(vt);
    if (tables.empty()) throw std::runtime_error("requested native entity type not found in DLL RTTI");
    std::vector<NativeEntity> found; U examined = 0, total = 0;
    std::map<uint32_t, U> indices, uids;
    std::string query = folded(a.get("--name"));
    auto stats = scan(m, Near(a), [&](U base, const uint8_t* bytes, size_t n) {
        for (size_t i = 0; i + 8 <= n; i += 8) {
            if (!tables.contains(word(bytes + i))) continue;
            if (stopped) throw std::runtime_error("interrupted");
            auto e = read_entity(m, catalog, base + i, kind); if (!e) continue;
            ++examined;
            if ((a.has("--id") && e->index != id) || (a.has("--uid") && e->uid != uid) || (!query.empty() && folded(e->name).find(query) == std::string::npos)) continue;
            ++total; ++indices[e->index]; ++uids[e->uid];
            if (found.size() < max) found.push_back(*e);
        }
    });
    require_scan(stats);
    // Reject ambiguous native identities rather than silently selecting one address.
    for (auto& e : found) if (indices[e.index] > 1 || uids[e.uid] > 1) throw std::runtime_error("duplicate matching native entity identity; discovery is ambiguous");
    std::vector<std::string> rows;
    for (auto& e : found) {
        auto fresh = read_entity(m, catalog, e.address, e.kind);
        if (!fresh || fresh->index != e.index || fresh->uid != e.uid || fresh->name != e.name) throw std::runtime_error("native identity changed during scan; retry");
        Json j = entity_json(e);
        if (a.has("--editor")) j.raw("editor_data",(kind=="player"?editor_player_data(m,catalog,e):kind=="staff"?editor_staff_data(m,e):editor_entity_data(m,catalog,e)).done());
        if (a.has("--editor") && !a.json) { if(kind=="player")print_player_editor(m,catalog,e);else if(kind=="staff")print_staff_editor(m,e);else print_entity_editor(m,catalog,e); }
        if (a.has("--related")) {
            EntityRelations links{m, catalog, {}, {}, {}, {}};
            if (e.kind == "player") links.player(e);
            else if (e.kind == "club") links.club(e);
            else if (e.kind == "competition") links.link(e.address, 0x60, "nation", "competition_nation");
            j.raw("related", links.json().done());
        }
        rows.push_back(j.done());
    }
    Json out; out.str("command", "entity"); out.num("pid", m.pid); out.str("kind", kind); out.str("query", a.get("--name"));
    out.str("source", "native_database_memory"); out.raw("requires_open_screen", "false"); out.num("validated_records", examined);
    out.num("total_matches", total); out.num("scanned_bytes", stats.bytes); out.num("unreadable_chunks", stats.unreadable);
    out.raw("truncated", total > found.size() ? "true" : "false"); out.raw("matches", array(rows));
    if (a.has("--editor") && !a.json) std::cout << total << " matching " << kind << " record(s)\n";
    else emit(a, out, out.done());
    return 0;
}
