#pragma once
#include "inspect.hpp"

inline U snapshot(const Memory& m, const std::string& path, const Near& near) {
    std::ofstream f(path, std::ios::binary | std::ios::trunc);
    if (!f) throw std::runtime_error("cannot create snapshot: " + path);
    auto stats = scan(m, near, [&](U base, const uint8_t* b, size_t n) {
        U header[2]{base, n}; f.write(reinterpret_cast<char*>(header), sizeof(header));
        f.write(reinterpret_cast<const char*>(b), n);
        if (!f) throw std::runtime_error("snapshot write failed");
    });
    require_scan(stats); f.close(); if (!f) throw std::runtime_error("snapshot close failed");
    return stats.bytes;
}
struct Change { U address, before, after; Owner owner; unsigned score = 0; };
struct Diff { std::vector<Change> changes; U total = 0, unreadable = 0; };
inline std::set<U> load_set(const std::string& path) {
    std::ifstream f(path); if (!f) throw std::runtime_error("cannot read exclusion file: " + path);
    std::set<U> out; std::string line; while (std::getline(f, line)) if (!line.empty()) out.insert(number(line, 16)); return out;
}
inline Diff compare(const Memory& m, const std::string& path, const Args& a, const std::set<U>& exclude, bool rank = false) {
    U from = a.has("--from") ? number(a.get("--from"), 16) : 0;
    U to = a.has("--to") ? number(a.get("--to"), 16) : 0;
    U to32 = a.has("--to32") ? number(a.get("--to32"), 16) : 0;
    if (to32 > UINT32_MAX) throw std::runtime_error("to32 exceeds u32");
    std::ifstream f(path, std::ios::binary); if (!f) throw std::runtime_error("cannot open snapshot: " + path);
    std::ofstream saved;
    if (a.has("--save-changed")) { saved.open(a.get("--save-changed")); if (!saved) throw std::runtime_error("cannot create changed-address file"); }
    Diff out; Near near(a); U max = a.val("--max", 400);
    if (max > 100000) throw std::runtime_error("max must be <= 100000");
    auto better = [](const Change& x, const Change& y) { return x.score > y.score || (x.score == y.score && x.address < y.address); };
    U headers = 0, readable = 0;
    while (true) {
        U h[2]; f.read(reinterpret_cast<char*>(h), sizeof(h));
        if (f.gcount() == 0 && f.eof()) break;
        if (f.gcount() != sizeof(h) || !h[1] || h[1] > CHUNK || h[0] > UINT64_MAX - h[1]) throw std::runtime_error("invalid or truncated snapshot header");
        ++headers; std::vector<uint8_t> before(h[1]), after(h[1]);
        f.read(reinterpret_cast<char*>(before.data()), h[1]); if (static_cast<U>(f.gcount()) != h[1]) throw std::runtime_error("truncated snapshot data");
        if (!m.read(h[0], after.data(), after.size())) { ++out.unreadable; continue; }
        ++readable;
        for (size_t i = 0; i + 8 <= before.size(); i += 8) {
            U old = word(before.data() + i), cur = word(after.data() + i), addr = h[0] + i;
            if (old == cur) continue;
            if (saved.is_open()) { saved << std::hex << addr << std::endl; if (!saved) throw std::runtime_error("changed-address write failed"); }
            if (exclude.contains(addr) || !near.contains(addr)) continue;
            if (a.has("--from") && old != from) continue;
            if (a.has("--to") && cur != to) continue;
            if (a.has("--to32")) {
                if (!((uint32_t(cur) == to32 && uint32_t(old) != to32) || (uint32_t(cur >> 32) == to32 && uint32_t(old >> 32) != to32))) continue;
            }
            ++out.total; Change c{addr, old, cur, {}, 0};
            if (rank && max) {
                c.owner = nearest_owner(m, addr); c.score = (!c.owner.type.name.empty() ? 100 : 0) + (m.pointer(cur) ? 20 : 0) + ((!old && cur) ? 10 : 0);
                if (out.changes.size() < max) { out.changes.push_back(c); std::push_heap(out.changes.begin(), out.changes.end(), better); }
                else if (better(c, out.changes.front())) {
                    std::pop_heap(out.changes.begin(), out.changes.end(), better);
                    out.changes.back() = c; std::push_heap(out.changes.begin(), out.changes.end(), better);
                }
            } else if (out.changes.size() < max) out.changes.push_back(c);
        }
    }
    if (!headers || !readable) throw std::runtime_error("snapshot has no readable chunks");
    if (saved.is_open()) { saved.close(); if (!saved) throw std::runtime_error("changed-address close failed"); }
    if (rank) std::sort(out.changes.begin(), out.changes.end(), better);
    return out;
}
inline void show_diff(const Args& a, const Diff& d, const std::string& command) {
    std::vector<std::string> rows;
    for (auto& c : d.changes) {
        Json j; j.addr("address", c.address); j.addr("old", c.before); j.addr("new", c.after);
        if (command == "diffset") {
            j.num("score", c.score); j.str("owner_status", "nearest_preceding_rtti_heuristic");
            if (c.owner.addr) { j.addr("nearest_object", c.owner.addr); j.raw("rtti", type_json(c.owner.type).done()); } else j.raw("nearest_object", "null");
        }
        rows.push_back(j.done());
        if (!a.json) std::cout << hex(c.address) << ": " << hex(c.before) << " -> " << hex(c.after)
            << (command == "diffset" ? " score=" + std::to_string(c.score) + " nearest=" + hex(c.owner.addr) + " " + c.owner.type.name : "") << std::endl;
    }
    Json j; j.str("command", command); j.num("changed_words", d.total); j.num("unreadable_chunks", d.unreadable); j.raw("changes", array(rows));
    j.raw("truncated", d.total > d.changes.size() ? "true" : "false");
    emit(a, j, std::to_string(d.total) + " changed words after filters (" + std::to_string(d.changes.size()) + " shown)");
}
inline int cmd_snap(const Memory& m, const Args& a) {
    a.allow({"--near"}, 3); U bytes = snapshot(m, a.pos[2], Near(a));
    Json j; j.str("command", "snap"); j.str("file", a.pos[2]); j.num("bytes", bytes); emit(a, j, "saved " + std::to_string(bytes) + " bytes to " + a.pos[2]); return 0;
}
inline int cmd_diff(const Memory& m, const Args& a) {
    a.allow({"--save-changed", "--exclude", "--near", "--from", "--to", "--to32", "--max"}, 3);
    if (same_file(a.get("--save-changed"), a.pos[2]) || same_file(a.get("--save-changed"), a.get("--exclude"))) throw std::runtime_error("input/output files must differ");
    auto exclude = a.has("--exclude") ? load_set(a.get("--exclude")) : std::set<U>{};
    show_diff(a, compare(m, a.pos[2], a, exclude), "diff"); return 0;
}
inline int cmd_search(const Memory& m, const Args& a, bool refs) {
    a.allow({"--near", "--max"}, refs ? 3 : 4);
    U raw = 0; size_t width = 8;
    if (refs) raw = number(a.pos[2], 16);
    else {
        const auto& t = a.pos[2]; const auto& s = a.pos[3]; width = 4;
        if (t == "u64") { raw = number(s); width = 8; }
        else if (t == "u32") { raw = number(s); if (raw > UINT32_MAX) throw std::runtime_error("u32 overflow"); }
        else if (t == "i32") {
            bool neg = s.starts_with("-"); U n = number(neg ? s.substr(1) : s);
            if (n > (neg ? 2147483648ULL : 2147483647ULL)) throw std::runtime_error("i32 overflow");
            raw = uint32_t(neg ? -int64_t(n) : int64_t(n));
        } else if (t == "f32") {
            size_t end; float f = std::stof(s, &end); if (end != s.size() || !std::isfinite(f)) throw std::runtime_error("invalid f32"); raw = std::bit_cast<uint32_t>(f);
        } else throw std::runtime_error("type must be i32|u32|u64|f32");
    }
    U max = a.val("--max", 400), hits = 0; if (max > 100000) throw std::runtime_error("max must be <= 100000");
    std::vector<std::string> rows;
    auto stats = scan(m, Near(a), [&](U base, const uint8_t* b, size_t n) {
        for (size_t i = 0; i + width <= n; i += width) {
            U v = 0; memcpy(&v, b + i, width);
            if (v == raw) { ++hits; if (hits <= max) { rows.push_back(quote(hex(base + i))); if (!a.json) std::cout << hex(base + i) << std::endl; } }
        }
    });
    require_scan(stats); Json j; j.str("command", refs ? "refs" : "find"); j.num("total", hits); j.raw("addresses", array(rows)); j.num("unreadable_chunks", stats.unreadable);
    j.raw("truncated", hits > max ? "true" : "false"); emit(a, j, std::to_string(hits) + " hits"); return 0;
}
inline int cmd_dump(const Memory& m, const Args& a) {
    a.allow({}, 4); U addr = number(a.pos[2], 16), len = number(a.pos[3]); if (!len || len > 16 * CHUNK) throw std::runtime_error("len must be 1..16777216");
    std::vector<uint8_t> b(len); if (!m.read(addr, b.data(), b.size())) throw std::runtime_error("unreadable range");
    std::ostringstream bytes;
    for (size_t i = 0; i < b.size(); ++i) {
        bytes << std::hex << std::setw(2) << std::setfill('0') << unsigned(b[i]);
        if (!a.json) { if (i % 16 == 0) std::cout << hex(addr + i) << ":"; std::cout << " " << std::hex << std::setw(2) << std::setfill('0') << unsigned(b[i]); if (i % 16 == 15 || i + 1 == b.size()) std::cout << std::endl; }
    }
    Json j; j.str("command", "dump"); j.addr("address", addr); j.num("length", len); j.str("bytes", bytes.str()); if (a.json) std::cout << j.done() << std::endl; return 0;
}
inline int cmd_watch(const Memory& m, const Args& a) {
    a.allow({"--ms", "--count"}, 4); U addr = number(a.pos[2], 16), len = number(a.pos[3]), ms = a.val("--ms", 250), count = a.val("--count", 0);
    if (!len || len > 16 * CHUNK || !ms || ms > 3600000) throw std::runtime_error("watch len=1..16777216, ms=1..3600000 required");
    std::vector<uint8_t> old(len), cur(len); if (!m.read(addr, old.data(), len)) throw std::runtime_error("unreadable watch range");
    auto start = std::chrono::steady_clock::now(); Json ready; ready.str("command", "watch"); ready.str("event", "ready"); ready.addr("address", addr); ready.num("length", len); emit(a, ready, "watching " + hex(addr));
    for (U poll = 1; !stopped && (!count || poll <= count); ++poll) {
        pause_ms(ms); if (stopped) break;
        if (!m.read(addr, cur.data(), len)) throw std::runtime_error("watch range became unreadable");
        U elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now() - start).count();
        for (size_t i = 0; i < len; i += 8) {
            size_t n = std::min<size_t>(8, len - i); U before = 0, after = 0; memcpy(&before, old.data() + i, n); memcpy(&after, cur.data() + i, n);
            if (before == after) continue;
            Json j; j.str("command", "watch"); j.str("event", "change"); j.addr("address", addr + i); j.addr("old", before); j.addr("new", after); j.num("width", n); j.num("elapsed_ms", elapsed);
            emit(a, j, std::to_string(elapsed) + "ms " + hex(addr + i) + ": " + hex(before) + " -> " + hex(after));
        }
        old.swap(cur);
    }
    return 0;
}
// Unique temporary snapshot files; RAII removes them on success, exception and SIGINT.
struct Temp {
    std::string path;
    Temp() {
        path = (std::filesystem::temp_directory_path() / "fmscan-XXXXXX").string();
        int fd = mkstemp(path.data()); if (fd < 0) throw std::runtime_error("cannot create temporary snapshot"); close(fd);
    }
    ~Temp() { unlink(path.c_str()); }
};
inline int cmd_diffset(const Memory& m, const Args& a) {
    a.allow({"--near", "--noise-ms", "--action-wait", "--max", "--exclude"}, 2);
    U noise_ms = a.val("--noise-ms", 1000); if (noise_ms > 3600000) throw std::runtime_error("noise-ms exceeds one hour");
    U action_seconds = a.val("--action-wait", 0); if (action_seconds > 3600) throw std::runtime_error("action-wait exceeds one hour");
    Temp noise, baseline; Near near(a);
    std::cerr << "Sampling background noise...\n"; snapshot(m, noise.path, near); pause_ms(noise_ms);
    if (stopped) throw std::runtime_error("interrupted");
    // Noise collection needs all changed addresses, independent of output candidate limit.
    Args noise_args = a; noise_args.opts.erase("--exclude"); noise_args.opts["--max"] = "0";
    Temp noise_addresses; noise_args.opts["--save-changed"] = noise_addresses.path;
    compare(m, noise.path, noise_args, {}); auto exclude = load_set(noise_addresses.path);
    if (a.has("--exclude")) { auto extra = load_set(a.get("--exclude")); exclude.insert(extra.begin(), extra.end()); }
    snapshot(m, baseline.path, near);
    if (a.has("--action-wait")) { std::cerr << "Perform action now; waiting " << action_seconds << " seconds...\n"; pause_ms(action_seconds * 1000); }
    else {
        std::cerr << "Perform action now, then press Enter...\n";
        pollfd input{STDIN_FILENO, POLLIN, 0}; int ready = 0;
        while (!stopped && !(ready = poll(&input, 1, 50))) {}
        if (!stopped) {
            std::string line;
            if (ready < 0 || !std::getline(std::cin, line)) throw std::runtime_error("action confirmation requires stdin or --action-wait N");
        }
    }
    if (stopped) throw std::runtime_error("interrupted");
    std::cerr << "Excluded " << exclude.size() << " noise addresses; ranking candidates...\n";
    show_diff(a, compare(m, baseline.path, a, exclude, true), "diffset"); return 0;
}
