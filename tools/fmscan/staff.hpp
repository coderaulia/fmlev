#pragma once
#include "entities.hpp"

// Staff editor view. Attribute bytes are NOT mapped yet: FM26 shows staff abilities only as labels,
// so the raw candidate blocks are printed for mapping work and labelled as such.
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
    std::vector<std::string> blocks;
    for (auto& b : STAFF_BLOCKS) {
        Json r; r.str("block", b.name); r.addr("address", e.record + b.offset); r.num("offset", b.offset);
        r.raw("bytes", staff_bytes(m, e.record + b.offset, b.length));
        r.str("status", "unmapped_raw_bytes_heuristic");
        blocks.push_back(r.done());
    }
    j.raw("raw_blocks", array(blocks));
    j.str("attributes", "unavailable: staff attribute offsets are not mapped (screen shows labels only)");
    j.str("verification", "identity by ACTUAL_NON_PLAYER sub-objects at +0x100/+0x128 and duplicate UID; no screen comparison");
    j.str("write_support", "none");
    return j;
}
inline void print_staff_editor(const Memory& m, const NativeEntity& e) {
    std::cout << e.name << " (staff, index " << e.index << ", UID " << e.uid << ", record " << hex(e.record) << ")\n";
    for (auto& b : STAFF_BLOCKS)
        std::cout << "  " << b.name << " +" << hex(b.offset) << ": " << staff_bytes(m, e.record + b.offset, b.length) << " (unmapped)\n";
}
