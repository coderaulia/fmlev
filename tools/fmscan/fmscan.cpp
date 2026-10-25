// fmscan: read-only memory scanner for a running FM26 (Proton) process. Never writes to the game.
// Build: g++ -O2 -std=c++20 -o fmscan fmscan.cpp
//
//   fmscan snap  <pid> <file>                         save all rw-private memory to <file>
//   fmscan diff  <pid> <file> [--save-changed f] [--exclude f] [--near hex[:radius]]
//                              [--to hex] [--from hex] [--to32 hex] [--max N]
//                                                     compare <file> with live memory, 8-byte words
//   fmscan refs  <pid> <hex_ptr> [--near hex[:radius]]  slots holding a pointer value
//   fmscan find  <pid> <i32|u32|u64|f32> <value>      slots holding a value (aligned)
//   fmscan dump  <pid> <hex_addr> <len>               hex dump of live memory
//
// Differential workflow: snap (A) -> do nothing for ~10 s -> `diff A --save-changed noise`
// -> snap (A2) -> perform the in-game action -> `diff A2 --exclude noise`.
#include <sys/uio.h>
#include <algorithm>
#include <cinttypes>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

struct Region { uint64_t start, end; };
static const size_t CHUNK = 1 << 20;

static bool read_mem(pid_t pid, uint64_t addr, void* buf, size_t len) {
    iovec l{buf, len}, r{(void*)addr, len};
    return process_vm_readv(pid, &l, 1, &r, 1, 0) == (ssize_t)len;
}

// rw-p mappings that are anonymous or heap: where game objects live. File-backed and device maps skipped.
static std::vector<Region> regions(pid_t pid) {
    std::vector<Region> out;
    std::ifstream maps("/proc/" + std::to_string(pid) + "/maps");
    std::string line;
    while (std::getline(maps, line)) {
        uint64_t s, e; char perms[8]; uint64_t off; char dev[16]; uint64_t inode; char path[512] = {0};
        int n = sscanf(line.c_str(), "%" SCNx64 "-%" SCNx64 " %7s %" SCNx64 " %15s %" SCNu64 " %511[^\n]", &s, &e, perms, &off, dev, &inode, path);
        if (n < 6 || strcmp(perms, "rw-p") != 0) continue;
        std::string p = n >= 7 ? path : "";
        size_t first = p.find_first_not_of(' ');
        p = first == std::string::npos ? "" : p.substr(first);
        if (!p.empty() && p != "[heap]") continue;
        out.push_back({s, e});
    }
    return out;
}

static uint64_t hexv(const char* s) { return strtoull(s, nullptr, 16); }

struct Near { bool on = false; uint64_t center = 0, radius = 0x100; };
static Near parse_near(const char* s) {
    Near n; n.on = true;
    const char* colon = strchr(s, ':');
    n.center = hexv(s);
    if (colon) n.radius = hexv(colon + 1);
    return n;
}
static bool in_near(const Near& n, uint64_t a) { return !n.on || (a + n.radius >= n.center && a <= n.center + n.radius); }

static void load_set(const char* path, std::vector<uint64_t>& v) {
    std::ifstream f(path); std::string l;
    while (std::getline(f, l)) if (!l.empty()) v.push_back(strtoull(l.c_str(), nullptr, 16));
    std::sort(v.begin(), v.end());
}

static int cmd_snap(pid_t pid, const char* file) {
    FILE* out = fopen(file, "wb");
    if (!out) { perror("open"); return 1; }
    uint64_t total = 0; std::vector<char> buf(CHUNK);
    for (auto r : regions(pid)) {
        for (uint64_t a = r.start; a < r.end; a += CHUNK) {
            size_t len = std::min<uint64_t>(CHUNK, r.end - a);
            bool ok = read_mem(pid, a, buf.data(), len);
            if (!ok) continue;                       // unreadable chunks are simply absent
            uint64_t hdr[2] = {a, (uint64_t)len};
            fwrite(hdr, 8, 2, out); fwrite(buf.data(), 1, len, out); total += len;
        }
    }
    fclose(out);
    printf("saved %.1f MB to %s\n", total / 1048576.0, file);
    return 0;
}

static int cmd_diff(pid_t pid, int argc, char** argv) {
    const char* file = argv[0];
    const char *save = nullptr, *excl = nullptr; Near near; bool hasTo = false, hasFrom = false, hasTo32 = false; uint32_t to32 = 0;
    uint64_t to = 0, from = 0; size_t max = 400;
    for (int i = 1; i < argc; i++) {
        std::string a = argv[i];
        if (a == "--save-changed" && i + 1 < argc) save = argv[++i];
        else if (a == "--exclude" && i + 1 < argc) excl = argv[++i];
        else if (a == "--near" && i + 1 < argc) near = parse_near(argv[++i]);
        else if (a == "--to" && i + 1 < argc) { to = hexv(argv[++i]); hasTo = true; }
        else if (a == "--from" && i + 1 < argc) { from = hexv(argv[++i]); hasFrom = true; }
        else if (a == "--to32" && i + 1 < argc) { to32 = (uint32_t)hexv(argv[++i]); hasTo32 = true; }
        else if (a == "--max" && i + 1 < argc) max = strtoull(argv[++i], nullptr, 10);
    }
    std::vector<uint64_t> exclude; if (excl) load_set(excl, exclude);
    FILE* in = fopen(file, "rb");
    if (!in) { perror("open"); return 1; }
    FILE* sv = save ? fopen(save, "w") : nullptr;
    std::vector<uint64_t> old, cur; uint64_t hdr[2]; size_t shown = 0, changed = 0;
    while (fread(hdr, 8, 2, in) == 2) {
        size_t words = hdr[1] / 8; old.resize(words); cur.resize(words);
        if (fread(old.data(), 8, words, in) != words) break;
        if (!read_mem(pid, hdr[0], cur.data(), words * 8)) continue;
        for (size_t i = 0; i < words; i++) {
            if (old[i] == cur[i]) continue;
            uint64_t addr = hdr[0] + i * 8;
            if (sv) fprintf(sv, "%" PRIx64 "\n", addr);
            if (!exclude.empty() && std::binary_search(exclude.begin(), exclude.end(), addr)) continue;
            if (!in_near(near, addr)) continue;
            if (hasTo && cur[i] != to) continue;
            if (hasFrom && old[i] != from) continue;
            if (hasTo32 && !((uint32_t)cur[i] == to32 && (uint32_t)old[i] != to32) && !((uint32_t)(cur[i] >> 32) == to32 && (uint32_t)(old[i] >> 32) != to32)) continue;
            changed++;
            if (shown < max) { printf("%" PRIx64 ": %016" PRIx64 " -> %016" PRIx64 "\n", addr, old[i], cur[i]); shown++; }
        }
    }
    printf("%zu changed words after filters (%zu shown)\n", changed, shown);
    return 0;
}

static int cmd_refs(pid_t pid, int argc, char** argv) {
    uint64_t target = hexv(argv[0]); Near near;
    for (int i = 1; i + 1 < argc; i++) if (std::string(argv[i]) == "--near") near = parse_near(argv[++i]);
    std::vector<uint64_t> buf(CHUNK / 8); size_t hits = 0;
    for (auto r : regions(pid))
        for (uint64_t a = r.start; a < r.end; a += CHUNK) {
            size_t len = std::min<uint64_t>(CHUNK, r.end - a);
            if (!read_mem(pid, a, buf.data(), len)) continue;
            for (size_t i = 0; i < len / 8; i++)
                if (buf[i] == target && in_near(near, a + i * 8)) { printf("%" PRIx64 "\n", a + i * 8); hits++; }
        }
    fprintf(stderr, "%zu references\n", hits);
    return 0;
}

static int cmd_find(pid_t pid, const char* type, const char* value) {
    std::string t = type; uint64_t raw; size_t size = 4;
    if (t == "i32") raw = (uint32_t)strtol(value, nullptr, 0);
    else if (t == "u32") raw = (uint32_t)strtoul(value, nullptr, 0);
    else if (t == "u64") { raw = strtoull(value, nullptr, 0); size = 8; }
    else if (t == "f32") { float f = strtof(value, nullptr); uint32_t u; memcpy(&u, &f, 4); raw = u; }
    else { fprintf(stderr, "type must be i32|u32|u64|f32\n"); return 1; }
    std::vector<uint8_t> buf(CHUNK); size_t hits = 0;
    for (auto r : regions(pid))
        for (uint64_t a = r.start; a < r.end; a += CHUNK) {
            size_t len = std::min<uint64_t>(CHUNK, r.end - a);
            if (!read_mem(pid, a, buf.data(), len)) continue;
            for (size_t i = 0; i + size <= len; i += size) {
                uint64_t v = 0; memcpy(&v, &buf[i], size);
                if (v == raw) { printf("%" PRIx64 "\n", a + i); hits++; }
            }
        }
    fprintf(stderr, "%zu hits\n", hits);
    return 0;
}

static int cmd_dump(pid_t pid, uint64_t addr, size_t len) {
    std::vector<uint8_t> b(len);
    if (!read_mem(pid, addr, b.data(), len)) { fprintf(stderr, "unreadable\n"); return 1; }
    for (size_t i = 0; i < len; i += 16) {
        printf("%" PRIx64 ":", addr + i);
        for (size_t j = i; j < i + 16 && j < len; j++) printf(" %02x", b[j]);
        printf("\n");
    }
    return 0;
}

int main(int argc, char** argv) {
    if (argc < 4) { fprintf(stderr, "usage: see header of fmscan.cpp\n"); return 1; }
    std::string cmd = argv[1]; pid_t pid = atoi(argv[2]);
    if (cmd == "snap") return cmd_snap(pid, argv[3]);
    if (cmd == "diff") return cmd_diff(pid, argc - 3, argv + 3);
    if (cmd == "refs") return cmd_refs(pid, argc - 3, argv + 3);
    if (cmd == "find" && argc >= 5) return cmd_find(pid, argv[3], argv[4]);
    if (cmd == "dump" && argc >= 5) return cmd_dump(pid, hexv(argv[3]), strtoull(argv[4], nullptr, 0));
    fprintf(stderr, "unknown command\n"); return 1;
}
