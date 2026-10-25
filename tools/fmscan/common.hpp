#pragma once
#include <sys/uio.h>
#include <unistd.h>
#include <poll.h>
#include <algorithm>
#include <array>
#include <bit>
#include <cerrno>
#include <charconv>
#include <chrono>
#include <cmath>
#include <csignal>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <filesystem>
#include <functional>
#include <iomanip>
#include <iostream>
#include <limits>
#include <map>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

using U = uint64_t;
constexpr size_t CHUNK = 1 << 20;
inline std::string hex(U v) { std::ostringstream s; s << "0x" << std::hex << v; return s.str(); }
inline U number(std::string s, int base = 10) {
    if (s.starts_with("0x") || s.starts_with("0X")) { s.erase(0, 2); base = 16; }
    U v = 0; auto [p, ec] = std::from_chars(s.data(), s.data() + s.size(), v, base);
    if (ec != std::errc{} || p != s.data() + s.size() || s.empty()) throw std::runtime_error("invalid number: " + s);
    return v;
}
inline std::string quote(const std::string& s) {
    std::ostringstream o; o << '"';
    for (unsigned char c : s) {
        if (c == '"' || c == '\\') o << '\\' << c;
        else if (c < 32) o << "\\u" << std::hex << std::setw(4) << std::setfill('0') << unsigned(c);
        else o << c;
    }
    return o.str() + "\"";
}
struct Json {
    std::string s = "{"; bool first = true;
    void raw(const std::string& k, const std::string& v) { if (!first) s += ","; first = false; s += quote(k) + ":" + v; }
    void str(const std::string& k, const std::string& v) { raw(k, quote(v)); }
    void num(const std::string& k, U v) { raw(k, std::to_string(v)); }
    void addr(const std::string& k, U v) { str(k, hex(v)); }
    std::string done() const { return s + "}"; }
};
inline std::string array(const std::vector<std::string>& rows) {
    std::string s = "["; for (auto& r : rows) { if (s.size() > 1) s += ","; s += r; } return s + "]";
}
struct Args {
    std::vector<std::string> pos;
    std::map<std::string, std::string> opts;
    bool json = false;
    Args(int argc, char** argv) {
        const std::set<std::string> flags{"--json", "--in", "--out"};
        for (int i = 1; i < argc; ++i) {
            std::string a = argv[i];
            if (!a.starts_with("--")) { pos.push_back(a); continue; }
            if (opts.contains(a)) throw std::runtime_error("duplicate option " + a);
            if (flags.contains(a)) opts[a] = "";
            else { if (i + 1 == argc || std::string(argv[i + 1]).starts_with("--")) throw std::runtime_error("missing value for " + a); opts[a] = argv[++i]; }
        }
        json = opts.contains("--json");
    }
    void allow(std::initializer_list<std::string> allowed, size_t n) const {
        if (pos.size() != n) throw std::runtime_error("wrong argument count; use --help");
        for (auto& [k, v] : opts) if (k != "--json" && std::find(allowed.begin(), allowed.end(), k) == allowed.end()) throw std::runtime_error("unknown option " + k);
    }
    bool has(const std::string& k) const { return opts.contains(k); }
    std::string get(const std::string& k, std::string fallback = "") const { auto i = opts.find(k); return i == opts.end() ? fallback : i->second; }
    U val(const std::string& k, U fallback) const { return has(k) ? number(get(k)) : fallback; }
};
struct Region { U start, end, offset; std::string perms, path; bool heap() const { return perms == "rw-p" && (path.empty() || path == "[heap]"); } };
struct Memory {
    pid_t pid; std::vector<Region> maps;
    explicit Memory(pid_t p) : pid(p) {
        std::ifstream f("/proc/" + std::to_string(pid) + "/maps");
        if (!f) throw std::runtime_error("cannot open process maps");
        std::string line;
        while (std::getline(f, line)) {
            std::istringstream s(line); std::string range, off, dev, inode, path, perms;
            s >> range >> perms >> off >> dev >> inode; std::getline(s, path);
            size_t first = path.find_first_not_of(' '); path = first == std::string::npos ? "" : path.substr(first);
            size_t dash = range.find('-');
            maps.push_back({number(range.substr(0, dash), 16), number(range.substr(dash + 1), 16), number(off, 16), perms, path});
        }
        if (maps.empty()) throw std::runtime_error("process has no readable maps");
    }
    const Region* region(U a, U len = 1) const {
        auto it = std::upper_bound(maps.begin(), maps.end(), a, [](U v, const Region& r) { return v < r.start; });
        if (it == maps.begin()) return nullptr;
        const auto& r = *std::prev(it);
        if (a >= r.start && a < r.end && len <= r.end - a) return &r;
        return nullptr;
    }
    bool read(U a, void* b, size_t n) const {
        if (n > std::numeric_limits<U>::max() - a) return false;
        iovec l{b, n}, r{reinterpret_cast<void*>(a), n};
        return process_vm_readv(pid, &l, 1, &r, 1, 0) == static_cast<ssize_t>(n);
    }
    template<class T> bool read(U a, T& v) const { return read(a, &v, sizeof(v)); }
    bool pointer(U a) const { auto r = region(a); return a && r && r->perms[0] == 'r'; }
};
struct Near {
    bool on = false; U lo = 0, hi = std::numeric_limits<U>::max();
    explicit Near(const Args& a) {
        if (!a.has("--near")) return;
        on = true; auto s = a.get("--near"); auto c = s.find(':');
        U center = number(s.substr(0, c), 16), radius = c == std::string::npos ? 0x100 : number(s.substr(c + 1), 16);
        lo = center > radius ? center - radius : 0;
        hi = center > std::numeric_limits<U>::max() - radius ? std::numeric_limits<U>::max() : center + radius;
    }
    bool contains(U a) const { return a >= lo && a <= hi; }
};
// Read only complete aligned words. Read failures are counted, never converted to zeroes.
struct ScanStats { U chunks = 0, unreadable = 0, bytes = 0; };
inline ScanStats scan(const Memory& m, const Near& near, const std::function<void(U, const uint8_t*, size_t)>& fn) {
    ScanStats stats; std::vector<uint8_t> b(CHUNK);
    for (auto& r : m.maps) {
        if (!r.heap() || r.end <= near.lo || r.start > near.hi) continue;
        U start = std::max(r.start, near.lo); if (start % 8) start += 8 - start % 8;
        U end = std::min(r.end, near.hi == UINT64_MAX ? near.hi : near.hi + 1);
        for (U a = start; a < end;) {
            size_t n = std::min<U>(CHUNK, end - a); ++stats.chunks;
            if (m.read(a, b.data(), n)) { stats.bytes += n; fn(a, b.data(), n); }
            else ++stats.unreadable;
            a += n;
        }
    }
    return stats;
}
inline void require_scan(const ScanStats& s) {
    if (!s.bytes) throw std::runtime_error("no memory could be read (check PID, permissions and --near)");
}
inline U word(const uint8_t* p) { U v; memcpy(&v, p, 8); return v; }
inline bool same_file(const std::string& x, const std::string& y) {
    if (x.empty() || y.empty()) return false;
    std::error_code ec;
    if (std::filesystem::equivalent(x, y, ec)) return true;
    return std::filesystem::absolute(x).lexically_normal() == std::filesystem::absolute(y).lexically_normal();
}
inline void emit(const Args& a, const Json& j, const std::string& text) { std::cout << (a.json ? j.done() : text) << std::endl; }
inline volatile std::sig_atomic_t stopped = 0;
inline void stop_handler(int) { stopped = 1; }
inline void pause_ms(U ms) {
    while (ms && !stopped) { U step = std::min<U>(ms, 50); std::this_thread::sleep_for(std::chrono::milliseconds(step)); ms -= step; }
}
