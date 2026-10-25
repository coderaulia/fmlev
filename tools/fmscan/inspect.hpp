#pragma once
#include "common.hpp"

struct TypeInfo { U vtable = 0, locator = 0, base = 0; std::string name; };
// x64 MSVC COL: signature, offset, cdOffset, type RVA, hierarchy RVA, self RVA.
inline TypeInfo resolve_vtable(const Memory& m, U vt) {
    TypeInfo t; t.vtable = vt;
    if (vt < 8 || !m.region(vt - 8, 16) || !m.read(vt - 8, t.locator)) return t;
    std::array<uint32_t, 6> col{};
    if (!m.region(t.locator, 24) || !m.read(t.locator, col) || col[0] != 1 || col[5] > t.locator) return t;
    t.base = t.locator - col[5];
    // Proton maps PE sections anonymously; identify the named header at the COL-derived base.
    const Region* module = m.region(t.base);
    if (!module || module->start != t.base || module->offset || module->path.find("game_plugin.dll") == std::string::npos) return t;
    uint16_t mz = 0, magic = 0; uint32_t pe_offset = 0, signature = 0, image_size = 0;
    if (!m.read(t.base, mz) || mz != 0x5a4d || !m.read(t.base + 0x3c, pe_offset) || pe_offset > CHUNK ||
        !m.read(t.base + pe_offset, signature) || signature != 0x4550 ||
        !m.read(t.base + pe_offset + 24, magic) || magic != 0x20b ||
        !m.read(t.base + pe_offset + 80, image_size) || image_size < 24) return t;
    auto in_image = [&](U p, U len) { return p >= t.base && p - t.base < image_size && len <= image_size - (p - t.base); };
    if (!in_image(vt, 8) || !in_image(t.locator, 24)) return t;
    U descriptor = t.base + col[3];
    if (!in_image(descriptor, 17) || !m.region(descriptor, 17)) return t;
    std::string name;
    for (size_t i = 0; i < 512; ++i) {
        char c; if (!in_image(descriptor + 16 + i, 1) || !m.read(descriptor + 16 + i, c)) return t;
        if (!c) { if (name.starts_with(".?A")) t.name = name; return t; }
        if (static_cast<unsigned char>(c) < 32 || static_cast<unsigned char>(c) > 126) return t;
        name += c;
    }
    return t;
}
inline TypeInfo object_type(const Memory& m, U a) { U vt = 0; m.read(a, vt); return resolve_vtable(m, vt); }
inline Json type_json(const TypeInfo& t) {
    Json j; j.addr("vtable", t.vtable); j.addr("locator", t.locator); j.addr("module_base", t.base); j.str("type_name", t.name); return j;
}
inline Json annotate(const Memory& m, U address, U value) {
    Json j; j.addr("address", address); j.addr("value", value);
    if (auto r = m.region(value); value && r && r->perms[0] == 'r') {
        j.str("kind", r->heap() ? "heap_pointer" : "mapped_pointer"); j.str("region", r->path);
        U first; if (m.read(value, first)) { j.addr("pointee_first", first); auto t = resolve_vtable(m, first); if (!t.name.empty()) j.raw("rtti", type_json(t).done()); }
    } else {
        j.str("kind", value <= 0xffff ? "small_int" : "word");
        std::vector<std::string> halves;
        for (int shift : {0, 32}) {
            uint32_t raw = value >> shift; Json h; h.num("bit_offset", shift); h.num("u32", raw);
            h.raw("i32", std::to_string(std::bit_cast<int32_t>(raw)));
            float f = std::bit_cast<float>(raw);
            if (std::isfinite(f)) { std::ostringstream s; s << std::setprecision(9) << f; h.raw("f32", s.str()); }
            unsigned year = raw >> 16, day = raw & 0x1ff;
            if (year >= 2000 && year <= 2200 && day >= 1 && day <= 366) {
                h.num("date_year", year); h.num("date_day", day); h.str("date_status", "shape_only");
            }
            halves.push_back(h.done());
        }
        j.raw("halves", array(halves));
    }
    return j;
}
inline int cmd_struct(const Memory& m, const Args& a) {
    a.allow({"--depth", "--max"}, 3);
    U root = number(a.pos[2], 16), depth = a.val("--depth", 0), max = a.val("--max", 64);
    if (depth > 8 || !max || max > 4096) throw std::runtime_error("depth must be 0..8 and max 1..4096 objects");
    std::set<U> visited; std::vector<std::string> rows; bool truncated = false;
    std::function<void(U, U)> visit = [&](U obj, U level) {
        if (visited.contains(obj)) return;
        if (visited.size() >= max) { truncated = true; return; }
        visited.insert(obj); std::vector<std::string> fields; std::vector<U> children;
        U unreadable = 0;
        for (U off = 0; off < 0x400; off += 8) {
            U v; if (obj > UINT64_MAX - off || !m.read(obj + off, v)) { ++unreadable; continue; }
            Json field = annotate(m, obj + off, v); field.num("offset", off); fields.push_back(field.done());
            auto r = m.region(v); if (v && r && r->heap()) children.push_back(v);
        }
        if (obj == root && unreadable == 128) throw std::runtime_error("unreadable object");
        Json row; row.addr("address", obj); row.num("depth", level); row.num("unreadable_words", unreadable);
        row.raw("fields", array(fields)); rows.push_back(row.done());
        if (!a.json) {
            std::cout << hex(obj) << " depth=" << level << " unreadable_words=" << unreadable << std::endl;
            for (auto& f : fields) std::cout << "  " << f << std::endl;
        }
        if (level < depth) for (U child : children) visit(child, level + 1);
    };
    visit(root, 0); Json j; j.str("command", "struct"); j.raw("objects", array(rows)); j.raw("truncated", truncated ? "true" : "false");
    if (a.json) std::cout << j.done() << std::endl;
    else if (truncated) std::cout << "object limit reached\n";
    return 0;
}
inline int cmd_rtti(const Memory& m, const Args& a) {
    a.allow({}, 3); U addr = number(a.pos[2], 16);
    TypeInfo t = resolve_vtable(m, addr);
    if (t.name.empty()) t = object_type(m, addr);
    if (t.name.empty()) throw std::runtime_error("no valid game_plugin.dll MSVC x64 RTTI at object/vtable");
    Json j = type_json(t); j.str("command", "rtti"); j.addr("input", addr);
    emit(a, j, hex(t.vtable) + " " + t.name + " module=" + hex(t.base)); return 0;
}
inline int cmd_vec(const Memory& m, const Args& a) {
    a.allow({"--stride", "--max"}, 3); U addr = number(a.pos[2], 16);
    std::array<U, 3> v{}; if (!m.read(addr, v)) throw std::runtime_error("unreadable vector header");
    auto [begin, end, cap] = v;
    if (begin > end || end > cap || (!begin && cap) || (begin && !m.region(begin, cap == begin ? 1 : cap - begin))) throw std::runtime_error("invalid vector bounds or capacity outside mapping");
    U bytes = end - begin, stride = a.val("--stride", 0), max = a.val("--max", 64);
    if (a.has("--stride") && !stride) throw std::runtime_error("stride must be positive");
    if (stride > CHUNK || max > 4096) throw std::runtime_error("stride <= 1048576 and max <= 4096 required");
    std::vector<std::string> guesses;
    for (U n : {1, 2, 4, 8, 16, 24, 32}) if (bytes && bytes % n == 0 && (cap - begin) % n == 0) guesses.push_back(std::to_string(n));
    // Width cannot be inferred uniquely from a triple. Raw qwords are the default view.
    U width = stride ? stride : 8, count = bytes / width;
    if (stride && (bytes % stride || (cap - begin) % stride)) throw std::runtime_error("stride does not divide size/capacity");
    std::vector<std::string> rows;
    for (U i = 0; i < std::min(count, max); ++i) {
        size_t n = std::min<U>(width, 64); std::vector<uint8_t> b(n);
        if (!m.read(begin + i * width, b.data(), n)) throw std::runtime_error("unreadable vector element");
        Json j; j.num("index", i); j.addr("address", begin + i * width);
        std::ostringstream s; for (auto byte : b) s << std::hex << std::setw(2) << std::setfill('0') << unsigned(byte);
        j.str("bytes", s.str()); j.raw("element_truncated", width > n ? "true" : "false");
        if (n >= 8) j.raw("first_word", annotate(m, begin + i * width, word(b.data())).done());
        rows.push_back(j.done());
    }
    Json j; j.str("command", "vec"); j.addr("begin", begin); j.addr("end", end); j.addr("capacity_end", cap); j.num("size_bytes", bytes);
    j.raw("stride_candidates", array(guesses)); j.num("view_stride", width); j.str("stride_status", stride ? "explicit" : "raw_qword_view");
    if (stride) j.num("element_count", count); else j.raw("element_count", "null");
    j.num("trailing_bytes", bytes % width); j.raw("elements", array(rows)); j.raw("truncated", count > max ? "true" : "false");
    emit(a, j, j.done()); return 0;
}
struct Owner { U addr = 0; TypeInfo type; };
inline Owner nearest_owner(const Memory& m, U slot) {
    auto r = m.region(slot, 8); if (!r) return {};
    U distance = std::min<U>(0x400, slot - r->start);
    distance -= distance % 8;
    std::array<uint8_t, 0x408> bytes{};
    if (!m.read(slot - distance, bytes.data(), distance + 8)) return {};
    for (U d = 0; d <= distance; d += 8) {
        auto t = resolve_vtable(m, word(bytes.data() + distance - d));
        if (!t.name.empty()) return {slot - d, t};
    }
    return {};
}
inline int cmd_graph(const Memory& m, const Args& a) {
    a.allow({"--in", "--out", "--near", "--max", "--len"}, 3);
    if (!a.has("--in") && !a.has("--out")) throw std::runtime_error("graph requires --in and/or --out");
    U obj = number(a.pos[2], 16), max = a.val("--max", 400), len = a.val("--len", 0x400);
    if (len > CHUNK || max > 100000) throw std::runtime_error("graph len/max exceeds limit");
    std::map<U, std::vector<std::string>> groups; std::map<U, U> totals; U hits = 0, failures = 0;
    if (a.has("--in")) {
        auto stats = scan(m, Near(a), [&](U base, const uint8_t* b, size_t n) {
            auto r = m.region(base);
            for (size_t i = 0; i + 8 <= n; i += 8) if (word(b + i) == obj) {
                ++totals[r->start]; ++hits;
                if (hits <= max) { Json j; j.addr("address", base + i); groups[r->start].push_back(j.done()); }
            }
        });
        require_scan(stats); failures = stats.unreadable;
    }
    std::vector<std::string> incoming, outgoing;
    for (auto& [start, total] : totals) {
        auto r = m.region(start); Json j; j.addr("region_start", start); j.addr("region_end", r->end); j.str("path", r->path);
        j.num("total", total); j.raw("references", array(groups[start])); incoming.push_back(j.done());
    }
    U out_hits = 0, unreadable = 0;
    if (a.has("--out")) for (U off = 0; off + 8 <= len; off += 8) {
        U v; if (obj > UINT64_MAX - off || !m.read(obj + off, v)) { ++unreadable; continue; }
        if (m.pointer(v)) { ++out_hits; if (outgoing.size() < max) { Json j = annotate(m, obj + off, v); j.num("offset", off); outgoing.push_back(j.done()); } }
    }
    if (a.has("--out") && len && unreadable == len / 8) throw std::runtime_error("unreadable object");
    Json j; j.str("command", "graph"); j.addr("object", obj); j.raw("inbound_regions", array(incoming)); j.raw("outbound", array(outgoing));
    j.num("inbound_total", hits); j.num("outbound_total", out_hits); j.num("unreadable_chunks", failures); j.num("unreadable_words", unreadable);
    j.str("ownership_status", "regions_only_not_proven_object_ownership"); emit(a, j, j.done()); return 0;
}
