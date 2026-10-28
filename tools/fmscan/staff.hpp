#pragma once
#include "entities.hpp"

// Staff editor view. Offsets matched against three on-screen profiles (Guardiola 79/1120,
// Maldini 2/45, Supriyatna 22412/41002451) through the open-profile UI bindings; each one was the
// only offset consistent with all three (scouting/medical: Guardiola and Supriyatna). Native fields are 1..100 and display as max(1, round(native / 5)).
struct StaffField { const char* group; const char* name; U offset; bool native; };
inline constexpr StaffField STAFF_FIELDS[] = {
    {"coaching", "attacking", 0x32, true}, {"coaching", "defending", 0x33, true},
    {"coaching", "fitness", 0x34, true}, {"coaching", "goalkeeping", 0x2b, true},
    {"coaching", "possession", 0x35, true}, {"coaching", "set_pieces", 0x43, true},
    {"coaching", "tactical", 0x37, true}, {"coaching", "technical", 0x36, true},
    {"coaching", "working_with_youngsters", 0x1c, false},
    {"mental", "adaptability", 0x170, false}, {"mental", "authority", 0x14, false},
    {"mental", "determination", 0x1d, true}, {"mental", "motivating", 0x2f, true},
    {"mental", "people_management", 0x2e, true},
    {"knowledge", "judging_player_ability", 0x2c, true}, {"knowledge", "judging_player_potential", 0x2d, true},
    {"knowledge", "judging_staff_ability", 0x42, true}, {"knowledge", "negotiating", 0x41, true},
    {"knowledge", "tactical_knowledge", 0x31, true},
    {"scouting", "analyzing_data", 0x3c, true},
    {"medical", "physiotherapy", 0x30, true}, {"medical", "sports_science", 0x3f, true},
};
inline uint8_t staff_display(uint8_t native) { return std::max<uint8_t>(1, uint8_t((native + 2) / 5)); }
// Raw context blocks. Native bytes +0x2a, +0x38..+0x3b, +0x3d, +0x3e, +0x40 are 1..100 across all staff but are
// not shown on any staff attribute tab; their meaning is unknown.
struct StaffBlock { const char* name; U offset, length; };
inline constexpr StaffBlock STAFF_BLOCKS[] = {
    {"record_0x10_small_values", 0x10, 0x40},
    {"record_0x170_small_values", 0x170, 8},
};
inline std::string staff_bytes(const Memory& m, U at, U n) {
    std::vector<uint8_t> b(n);
    if (!m.read(at, b.data(), n)) return "null";
    std::vector<std::string> v; for (auto x : b) v.push_back(std::to_string(x));
    return array(v);
}
// Information-tab candidates (staff F2/Information). These offsets came from the task brief, NOT from a
// local cross-check: no on-screen comparison and no live range survey has been done in this repository, so
// every value is reported as `candidate_unverified`. Promote one only after (a) a live range check over all
// staff records and (b) a match against the open staff profile for several people.
struct StaffInfoField { const char* name; U offset; U size; uint32_t lo, hi; const char* meaning; };
inline constexpr StaffInfoField STAFF_INFO_CANDIDATES[] = {
    {"reputation_0xd4", 0xd4, 2, 0, 10000, "meaning unknown (brief: first of three u16 reputations)"},
    {"reputation_0xd6", 0xd6, 2, 0, 10000, "meaning unknown (brief: second of three u16 reputations)"},
    {"reputation_0xd8", 0xd8, 2, 0, 10000, "meaning unknown (brief: third of three u16 reputations)"},
    {"current_ability", 0xda, 2, 0, 200, "staff CA"},
    {"potential_ability", 0xdc, 2, 0, 200, "staff PA"},
    {"birth_year", 0x18a, 2, 1900, 2100, "year of birth only; day and month not located"},
};
inline constexpr U STAFF_RECORD_SIZE = 0x240;
inline Json staff_information(const Memory& m, const NativeEntity& e) {
    std::vector<std::string> rows;
    for (auto& f : STAFF_INFO_CANDIDATES) {
        uint16_t v = 0; Json r; r.str("field", f.name); r.num("offset", f.offset); r.addr("address", e.record + f.offset);
        r.str("meaning", f.meaning); r.str("status", "candidate_unverified");
        if (!m.read(e.record + f.offset, v)) { r.raw("value", "null"); r.str("reason", "unreadable"); }
        else if (v < f.lo || v > f.hi) { r.raw("value", "null"); r.str("reason", "out of range " + std::to_string(v)); }
        else r.num("value", v);
        rows.push_back(r.done());
    }
    Json j; j.raw("fields", array(rows));
    j.str("status", "offsets_from_task_brief_not_cross_checked");
    return j;
}
// Read-only pointer survey of the staff record, looking for contract/job objects. Layout of a player's
// FULL_CONTRACT (wage +0x20, expiry +0x48, person back-pointer +8) is reused ONLY as a decode attempt.
inline Json staff_contract_candidate(const Memory& m, const NativeEntity& e, U contract) {
    Json c; c.addr("address", contract);
    uint32_t wage = 0, expiry = 0; U owner = 0, team = 0;
    if (!m.read(contract + 8, owner) || !m.read(contract + 0x10, team) || !m.read(contract + 0x20, wage) || !m.read(contract + 0x48, expiry)) {
        c.str("status", "unreadable"); return c;
    }
    c.str("status", "player_layout_decode_not_verified_for_staff");
    c.raw("person_back_pointer_matches_staff", owner == e.address ? "true" : "false");
    c.addr("person_back_pointer", owner); c.num("weekly_wage_raw_u32", wage);
    unsigned year = expiry >> 16, day = expiry & 0x1ff;
    Json d; d.addr("raw", expiry);
    if (year >= 2000 && year <= 2200 && day >= 1 && day <= 366) { d.num("year", year); d.num("day_of_year", day); d.str("status", "date_shape_only"); }
    else d.str("status", "invalid_or_sentinel");
    c.raw("expiry_candidate", d.done());
    auto t = object_type(m, team);
    c.str("team_pointer_rtti", t.name);
    return c;
}
inline Json staff_pointer_links(const Memory& m, const NativeEntity& e) {
    std::array<uint8_t, STAFF_RECORD_SIZE> b{};
    Json out;
    if (!m.read(e.record, b.data(), b.size())) { out.str("status", "unreadable"); return out; }
    std::vector<std::string> rows; U heap_pointers = 0, unnamed = 0; bool truncated = false;
    for (U off = 0; off + 8 <= b.size(); off += 8) {
        U v = word(b.data() + off);
        auto r = v ? m.region(v, 8) : nullptr;
        if (!r || !r->heap() || (v >= e.record && v < e.record + STAFF_RECORD_SIZE)) continue;
        ++heap_pointers;
        auto direct = object_type(m, v); U node_first = 0; TypeInfo via_node;
        if (direct.name.empty() && m.read(v, node_first)) {
            auto nr = node_first ? m.region(node_first, 8) : nullptr;
            if (nr && nr->heap()) via_node = object_type(m, node_first);
        }
        const TypeInfo& t = direct.name.empty() ? via_node : direct;
        if (t.name.empty()) { ++unnamed; continue; }
        if (rows.size() >= 64) { truncated = true; continue; }
        Json l; l.num("offset", off); l.addr("slot", e.record + off); l.addr("target", v);
        l.str("pattern", direct.name.empty() ? "node_first_qword" : "direct"); l.str("rtti", t.name);
        U object = direct.name.empty() ? node_first : v, owner = 0;
        l.raw("person_back_pointer_matches_staff", m.read(object + 8, owner) && owner == e.address ? "true" : "false");
        if (t.name == ".?AVFULL_CONTRACT@db@@") l.raw("contract_candidate", staff_contract_candidate(m, e, object).done());
        rows.push_back(l.done());
    }
    out.str("status", "heuristic_pointer_survey_of_first_0x240_bytes");
    out.num("heap_pointers", heap_pointers); out.num("named_by_rtti", rows.size()); out.num("unnamed", unnamed);
    out.raw("truncated", truncated ? "true" : "false"); out.raw("links", array(rows));
    out.str("note", "a slot is a contract/job lead only when RTTI names it and the +8 person back-pointer matches; none is verified");
    return out;
}
inline Json editor_staff_data(const Memory& m, const NativeEntity& e) {
    Json j; j.str("kind", "staff"); j.str("name", e.name); j.num("id", e.index); j.num("unique_id", e.uid);
    j.addr("record", e.record);
    j.raw("information", staff_information(m, e).done());
    j.raw("contract_discovery", staff_pointer_links(m, e).done());
    std::vector<std::string> fields, bad;
    for (auto& f : STAFF_FIELDS) {
        uint8_t v = 0; Json r; r.str("group", f.group); r.str("field", f.name); r.addr("address", e.record + f.offset);
        if (!m.read(e.record + f.offset, v)) { r.raw("value", "null"); r.str("reason", "unreadable"); bad.push_back(f.name); }
        else if (f.native ? (v < 1 || v > 100) : (v < 1 || v > 20)) { r.raw("value", "null"); r.str("reason", "out of range " + std::to_string(v)); bad.push_back(f.name); }
        else {
            r.num("value", f.native ? staff_display(v) : v);
            if (f.native) r.num("native", v);
            r.str("scale", f.native ? "native_1_100_display_round_div5" : "raw_1_20");
        }
        fields.push_back(r.done());
    }
    j.raw("attributes", array(fields));
    std::vector<std::string> blocks;
    for (auto& b : STAFF_BLOCKS) {
        Json r; r.str("block", b.name); r.addr("address", e.record + b.offset); r.num("offset", b.offset);
        r.raw("bytes", staff_bytes(m, e.record + b.offset, b.length));
        r.str("status", "unmapped_raw_bytes_heuristic");
        blocks.push_back(r.done());
    }
    j.raw("raw_blocks", array(blocks));
    j.str("attribute_evidence", "22 attributes (head coach, scouting and medical tabs) matched to on-screen profiles via UI bindings");
    j.str("verification", "identity by ACTUAL_NON_PLAYER sub-objects at +0x100/+0x128 and duplicate UID; no screen comparison");
    j.str("write_support", "none");
    return j;
}
inline void print_staff_editor(const Memory& m, const NativeEntity& e) {
    std::cout << e.name << " (staff, index " << e.index << ", UID " << e.uid << ", record " << hex(e.record) << ")\n";
    for (auto& f : STAFF_FIELDS) {
        uint8_t v = 0; if (!m.read(e.record + f.offset, v)) continue;
        std::cout << "  " << f.group << "." << f.name << " = " << int(f.native ? staff_display(v) : v);
        if (f.native) std::cout << " (native " << int(v) << ")";
        std::cout << "\n";
    }
    for (auto& f : STAFF_INFO_CANDIDATES) {
        uint16_t v = 0; std::cout << "  information." << f.name << " (+" << hex(f.offset) << ", candidate_unverified) = ";
        if (!m.read(e.record + f.offset, v)) std::cout << "unreadable\n";
        else if (v < f.lo || v > f.hi) std::cout << "out of range " << v << "\n";
        else std::cout << v << "\n";
    }
    std::cout << "  contract_discovery: " << staff_pointer_links(m, e).done() << "\n";
    for (auto& b : STAFF_BLOCKS)
        std::cout << "  " << b.name << " +" << hex(b.offset) << ": " << staff_bytes(m, e.record + b.offset, b.length) << " (unmapped)\n";
}
