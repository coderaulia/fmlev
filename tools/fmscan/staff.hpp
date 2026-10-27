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
inline Json editor_staff_data(const Memory& m, const NativeEntity& e) {
    Json j; j.str("kind", "staff"); j.str("name", e.name); j.num("id", e.index); j.num("unique_id", e.uid);
    j.addr("record", e.record);
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
    for (auto& b : STAFF_BLOCKS)
        std::cout << "  " << b.name << " +" << hex(b.offset) << ": " << staff_bytes(m, e.record + b.offset, b.length) << " (unmapped)\n";
}
