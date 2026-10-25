#pragma once
#include "inspect.hpp"

// FM26 native database types, identified through MSVC RTTI rather than fixed RVAs.
struct NativeType { TypeInfo rtti; uint32_t subobject_offset = 0; std::string kind; };
struct NativeCatalog {
    U base = 0, image_size = 0;
    std::map<U, NativeType> tables;
    explicit NativeCatalog(const Memory& m) {
        for (auto& r : m.maps) {
            if (r.offset || !r.path.ends_with("game_plugin.dll")) continue;
            uint16_t mz = 0, magic = 0; uint32_t pe = 0, signature = 0, size = 0;
            if (m.read(r.start, mz) && mz == 0x5a4d && m.read(r.start + 0x3c, pe) && pe <= CHUNK &&
                m.read(r.start + pe, signature) && signature == 0x4550 &&
                m.read(r.start + pe + 24, magic) && magic == 0x20b &&
                m.read(r.start + pe + 80, size) && size && size <= UINT64_MAX - r.start) {
                if (base) throw std::runtime_error("multiple game_plugin.dll images; native discovery is ambiguous");
                base = r.start; image_size = size;
            }
        }
        if (!base) throw std::runtime_error("valid game_plugin.dll PE32+ image not found");
        const std::map<std::string, std::string> names{
            {".?AVACTUAL_PLAYER@db@@", "player"}, {".?AVCLUB@db@@", "club"},
            {".?AVTEAM@db@@", "team"}, {".?AVNATION@db@@", "nation"},
            {".?AVCOMP@db@@", "competition"}};
        for (auto& r : m.maps) {
            if (r.start < base || r.end > base + image_size || r.perms[0] != 'r' || r.perms[1] == 'w' || r.perms[2] == 'x') continue;
            for (U a = r.start; a < r.end; a += CHUNK) {
                size_t n = std::min<U>(CHUNK, r.end - a); std::vector<uint8_t> bytes(n);
                if (!m.read(a, bytes.data(), n)) continue;
                for (size_t i = 0; i + 8 <= n; i += 8) {
                    U locator = word(bytes.data() + i);
                    if (locator < base || locator - base >= image_size) continue;
                    std::array<uint32_t, 6> col{};
                    if (!m.read(locator, col) || col[0] != 1 || col[5] != locator - base) continue;
                    auto type = resolve_vtable(m, a + i + 8);
                    auto name = names.find(type.name);
                    if (name != names.end()) tables.emplace(type.vtable, NativeType{type, col[1], name->second});
                }
            }
        }
        if (tables.empty()) throw std::runtime_error("no supported FM26 database RTTI types found");
    }
    const NativeType* at(const Memory& m, U address, const std::string& kind, bool primary = true) const {
        U vt = 0; if (!m.read(address, vt)) return nullptr;
        auto t = tables.find(vt);
        return t != tables.end() && t->second.kind == kind && (!primary || !t->second.subobject_offset) ? &t->second : nullptr;
    }
};

inline bool valid_utf8(const std::string& s) {
    for (size_t i = 0; i < s.size();) {
        unsigned char c = s[i++]; if (c < 32 || c == 127) return false;
        if (c < 128) continue;
        unsigned n = c >= 0xc2 && c <= 0xdf ? 1 : c >= 0xe0 && c <= 0xef ? 2 : c >= 0xf0 && c <= 0xf4 ? 3 : 0;
        if (!n || i + n > s.size()) return false;
        unsigned first = static_cast<unsigned char>(s[i]);
        if ((c == 0xe0 && first < 0xa0) || (c == 0xed && first >= 0xa0) ||
            (c == 0xf0 && first < 0x90) || (c == 0xf4 && first >= 0x90)) return false;
        for (unsigned k = 0; k < n; ++k) if ((static_cast<unsigned char>(s[i++]) & 0xc0) != 0x80) return false;
    }
    return true;
}
inline std::string native_string(const Memory& m, U p) {
    uint32_t n = 0;
    if (!p || !m.read(p, n) || !n || n > 512 || p > UINT64_MAX - 4 - n) return "";
    std::string s(n, ' ');
    return m.read(p + 4, s.data(), n) && valid_utf8(s) ? s : "";
}
inline std::string pooled_name(const Memory& m, U ref) {
    U p = 0; return ref && m.read(ref, p) ? native_string(m, p) : "";
}
inline std::string folded(std::string s) {
    for (char& c : s) if (c >= 'A' && c <= 'Z') c += 'a' - 'A';
    return s;
}
