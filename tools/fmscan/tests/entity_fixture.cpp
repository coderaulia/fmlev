// Self-contained native DB layout; no UI objects or pre-calibrated addresses.
#include <sys/mman.h>
#include <sys/prctl.h>
#include <unistd.h>
#include <fcntl.h>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <string>

int main(int argc, char** argv) {
    if (argc != 2 || prctl(PR_SET_PTRACER, PR_SET_PTRACER_ANY)) return 1;
    int fd = open(argv[1], O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0 || ftruncate(fd, 0x4000)) return 2;
    auto dll = static_cast<uint8_t*>(mmap(nullptr, 0x4000, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0));
    auto heap = static_cast<uint8_t*>(mmap(nullptr, 0x10000, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
    auto code = mmap(nullptr, 4096, PROT_READ | PROT_EXEC, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (dll == MAP_FAILED || heap == MAP_FAILED || code == MAP_FAILED) return 3;
    if (mmap(dll + 4096, 0x3000, PROT_READ | PROT_WRITE,
             MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0) == MAP_FAILED) return 4;
    auto module = reinterpret_cast<uint64_t>(dll), root = reinterpret_cast<uint64_t>(heap);
    auto put = [](uint8_t* p, auto v) { memcpy(p, &v, sizeof(v)); };
    put(dll, uint16_t{0x5a4d}); put(dll + 0x3c, uint32_t{0x80});
    put(dll + 0x80, uint32_t{0x4550}); put(dll + 0x98, uint16_t{0x20b}); put(dll + 0xd0, uint32_t{0x4000});
    unsigned type_index = 0;
    auto type = [&](const char* name, uint32_t offset = 0) {
        uint32_t slot = 0x1000 + type_index++ * 0x100;
        uint32_t col[]{1, offset, 0, slot + 0x40, 0, slot};
        memcpy(dll + slot, col, sizeof(col)); strcpy(reinterpret_cast<char*>(dll + slot + 0x50), name);
        put(dll + slot + 0x78, module + slot); put(dll + slot + 0x80, reinterpret_cast<uint64_t>(code));
        return module + slot + 0x80;
    };
    auto player = type(".?AVACTUAL_PLAYER@db@@"), person = type(".?AVACTUAL_PLAYER@db@@", 0x288);
    auto club = type(".?AVCLUB@db@@"), team = type(".?AVTEAM@db@@"), nation = type(".?AVNATION@db@@");
    auto comp = type(".?AVCOMP@db@@"), contract = type(".?AVFULL_CONTRACT@db@@");
    uint64_t native_vt = module + 0x3e00; put(dll + 0x3e00, reinterpret_cast<uint64_t>(code));
    size_t text_offset = 0xc000;
    auto text = [&](const char* s) {
        size_t n = strlen(s), pos = text_offset; put(heap + pos, uint32_t(n)); memcpy(heap + pos + 4, s, n);
        text_offset = (pos + 4 + n + 7) & ~size_t(7); return root + pos;
    };
    auto identity = [&](size_t at, uint64_t vt, uint32_t index, uint32_t uid) {
        put(heap + at, vt); put(heap + at + 8, index); put(heap + at + 12, uid); put(heap + at + 16, uid);
    };
    put(heap, player); put(heap + 0x148, native_vt); identity(0x288, person, 40629, 2000256231);
    memset(heap + 0x150, 10, 15); heap[0x177] = 100; heap[0x178] = 50;
    memset(heap + 0x148 + 0x17, 50, 0x4c - 0x17 + 1);
    heap[0x177] = 100; heap[0x178] = 50;
    memset(heap + 0x148 + 0x1b0, 10, 8);
    put(heap + 0x148 + 0x11c, uint16_t{170}); put(heap + 0x148 + 0x11e, uint16_t{195});
    put(heap + 0xb000, text("Lamine Yamal")); put(heap + 0xb008, text("Nasraoui"));
    put(heap + 0x2d8, root + 0xb000); put(heap + 0x2e0, root + 0xb008);
    put(heap + 0x330, root + 0x6000);
    identity(0x2000, club, 1074, 1708); put(heap + 0x20c0, text("Barcelona")); put(heap + 0x20d8, root + 0x4000);
    put(heap + 0x2018, root + 0xb100); put(heap + 0x2020, root + 0xb108); put(heap + 0x2028, root + 0xb108);
    put(heap + 0xb100, root + 0x3000);
    identity(0x3000, team, 1279, 1708); put(heap + 0x3030, root + 0x2000); put(heap + 0x3050, root + 0x5000);
    identity(0x4000, nation, 170, 796); put(heap + 0x4018, text("Spain"));
    identity(0x5000, comp, 51, 67); put(heap + 0x5040, text("Spanish First Division")); put(heap + 0x5060, root + 0x4000);
    put(heap + 0x6000, contract); put(heap + 0x6008, root + 0x288); put(heap + 0x6010, root + 0x3000);
    put(heap + 0x6020, uint32_t{280867}); put(heap + 0x6048, uint32_t{(2031 << 16) | 181}); heap[0x605d] = 10;
    put(heap + 0x148 + 0x1a8, root + 0x4000);
    // A stale copy with conflicting UIDs must not be discoverable.
    memcpy(heap + 0x8000, heap, 0x400); put(heap + 0x8298, uint32_t{123});
    if (mprotect(dll, 0x4000, PROT_READ)) return 5;
    std::cout << getpid() << " " << std::hex << root << std::endl;
    std::string cmd;
    while (std::getline(std::cin, cmd) && cmd != "quit") {
        if (cmd == "bad-contract") put(heap + 0x6008, uint64_t{0});
        if (cmd == "bad-nation") put(heap + 0x20d8, root + 0x5000);
        if (cmd == "bad-vector-owner") put(heap + 0x3030, root + 0x5000);
        if (cmd == "duplicate") memcpy(heap + 0x8000, heap, 0x400);
        if (cmd == "bad-name") heap[0xc004] = 0xff;
        if (cmd == "bad-attribute") heap[0x148 + 0x1e] = 101;
        if (cmd == "good-attribute") heap[0x148 + 0x1e] = 50;
        if (cmd == "bad-date") put(heap + 0x6048, uint32_t{0});
        std::cout << "ok" << std::endl;
    }
}
