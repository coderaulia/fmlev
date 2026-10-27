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
    auto heap = static_cast<uint8_t*>(mmap(nullptr, 0x20000, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
    auto code = mmap(nullptr, 4096, PROT_READ | PROT_EXEC, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (dll == MAP_FAILED || heap == MAP_FAILED || code == MAP_FAILED) return 3;
    if (mmap(dll + 4096, 0x3000, PROT_READ | PROT_WRITE,
             MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0) == MAP_FAILED) return 4;
    auto module = reinterpret_cast<uint64_t>(dll), root = reinterpret_cast<uint64_t>(heap);
    auto put = [](uint8_t* p, auto v) { memcpy(p, &v, sizeof(v)); };
    put(dll, uint16_t{0x5a4d}); put(dll + 0x3c, uint32_t{0x80});
    put(dll + 0x80, uint32_t{0x4550}); put(dll + 0x98, uint16_t{0x20b}); put(dll + 0xd0, uint32_t{0x4000});
    put(dll + 0x88, uint32_t{0x6a229e8a});
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
    auto finance = type(".?AVCLUB_FINANCE@db@@"), ntc = type(".?AVNATIONAL_TEAM_CONTAINER@db@@");
    auto nt = type(".?AVNATIONAL_TEAM@db@@"), actual_comp = type(".?AVACTUAL_COMP@sicomps@@");
    auto continent = type(".?AVCONTINENT@db@@");
    auto stadium = type(".?AVSTADIUM@db@@");
    auto staff = type(".?AVACTUAL_NON_PLAYER@db@@", 0x100), staff2 = type(".?AVACTUAL_NON_PLAYER@db@@", 0x128);
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
    // Guarded nonplayer editor layouts.
    put(heap + 0x2100, root + 0xe000); put(heap + 0x2150, root + 0xd000);
    put(heap + 0x2166, uint16_t{7557}); put(heap + 0x30a8, uint16_t{9098});
    put(heap + 0xe000, text("FCB")); put(heap + 0xe0d0, uint16_t{1899});
    heap[0xe118]=19;heap[0xe11e]=18;heap[0xe123]=20;heap[0xe124]=18;heap[0xe125]=17;
    put(heap + 0xd000, finance); put(heap + 0xd008, root + 0x2000);
    put(heap + 0xd014, int32_t{-1200}); heap[0xd03c] = 1; put(heap + 0xd7cc, int32_t{123456}); put(heap + 0xd7d0, int32_t{234567});
    put(heap + 0xd7d8, uint32_t{30}); put(heap + 0xd810, int32_t{100000}); put(heap + 0xd814, int32_t{10000});
    put(heap + 0xd818, int32_t{80000}); put(heap + 0xd81c, int32_t{90000}); heap[0xd8b5]=19;
    put(heap + 0x4028, text("ESP")); put(heap + 0x40f0, root + 0x19000);
    identity(0x19000, continent, 4, 4); put(heap + 0x19018, text("Europe"));
    put(heap + 0x4108, root + 0x10000); put(heap + 0x4110, root + 0x11000);
    identity(0x10000, ntc, 170, 796); identity(0x11000, ntc, 421, 2000001234);
    put(heap + 0x10170, root + 0x4000); put(heap + 0x11170, root + 0x4000);
    put(heap + 0x10864, uint16_t{145}); put(heap + 0x11864, uint16_t{160});
    put(heap + 0x10018, root + 0x1a000); put(heap + 0x10020, root + 0x1a008); put(heap + 0x10028, root + 0x1a008);
    put(heap + 0x11018, root + 0x1a010); put(heap + 0x11020, root + 0x1a018); put(heap + 0x11028, root + 0x1a018);
    put(heap + 0x1a000, root + 0x12000); put(heap + 0x1a010, root + 0x13000);
    identity(0x12000, nt, 10, 100); identity(0x13000, nt, 20, 200);
    put(heap + 0x12030, root + 0x10000); put(heap + 0x13030, root + 0x11000);
    put(heap + 0x120a8, uint16_t{9011}); put(heap + 0x130a8, uint16_t{9300});
    put(heap + 0x5048, text("First Division")); put(heap + 0x5050, text("SD1")); put(heap + 0x5058, root + 0x19000);
    put(heap + 0x5188, uint16_t{178}); put(heap + 0x50b0, root + 0x17000);
    put(heap + 0x17000, actual_comp); heap[0x17031]=0; put(heap + 0x17208, root + 0x14000);
    put(heap + 0x141e0, root + 0x15000); put(heap + 0x15060, root + 0x1a020); put(heap + 0x15068, root + 0x1a028);
    put(heap + 0x1a020, root + 0x16000); heap[0x1609a]=1;
    heap[0x41f0]=17;heap[0x41ec]=1;put(heap+0x41e4,uint32_t{49570725});
    identity(0x18000,stadium,374,1040);identity(0x1b000,stadium,375,1041);
    put(heap+0x18040,text("Camp Nou"));put(heap+0x1b040,text("Training Ground"));
    put(heap+0x1807c,uint32_t{62500});put(heap+0x1808c,uint32_t{105000});
    put(heap+0x3078,root+0x18000);put(heap+0x2140,root+0x1b000);
    // Staff record at 0xf000: sub-objects at +0x100/+0x128, identity at +0x108, pooled names at +0x150/+0x158.
    identity(0xf100, staff, 79, 1120); put(heap + 0xf128, staff2);
    put(heap + 0xb200, text("Pep")); put(heap + 0xb208, text("Guardiola"));
    put(heap + 0xf150, root + 0xb200); put(heap + 0xf158, root + 0xb208);
    for (int i = 0; i < 8; ++i) heap[0xf170 + i] = uint8_t(10 + i);
    heap[0xf032] = 89; heap[0xf041] = 1; heap[0xf031] = 16; heap[0xf01c] = 16; heap[0xf014] = 15; heap[0xf01d] = 79;
    for (int o : {0x2b,0x2c,0x2d,0x2e,0x2f,0x33,0x34,0x35,0x36,0x37,0x42,0x43}) heap[0xf000 + o] = 50;
    // A copy of only the first sub-object (no +0x128 owner) must not be discoverable.
    identity(0xf400, staff, 80, 1121); put(heap + 0xf450, root + 0xb200); put(heap + 0xf458, root + 0xb208);
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
        if (cmd == "bad-staff-attribute") heap[0xf032] = 101;
        if (cmd == "bad-staff-uid") put(heap + 0xf110, uint32_t{1});
        if (cmd == "bad-attribute") heap[0x148 + 0x1e] = 101;
        if (cmd == "good-attribute") heap[0x148 + 0x1e] = 50;
        if (cmd == "bad-date") put(heap + 0x6048, uint32_t{0});
        if (cmd == "bad-facility") heap[0xe118]=21;
        if (cmd == "bad-finance-owner") put(heap + 0xd008, root + 0x5000);
        if (cmd == "bad-container-owner") put(heap + 0x10170, root + 0x5000);
        if (cmd == "bad-national-team-owner") put(heap + 0x12030, root + 0x5000);
        if (cmd == "bad-youth") put(heap + 0x10864, uint16_t{300});
        if (cmd == "bad-comp-reputation") put(heap + 0x5188, uint16_t{10000});
        if (cmd == "bad-stage") put(heap + 0x15068, root + 0x1a000);
        if (cmd == "high-reputation") put(heap+0x30a8,uint16_t{9999});
        if (cmd == "shared-club") {
            memcpy(heap+0x1c000,heap+0x2000,0x170);
            put(heap+0x1c008,uint32_t{1099});put(heap+0x1c00c,uint32_t{1799});put(heap+0x1c010,uint32_t{1799});
        }
        if (cmd == "unknown-build") {
            mprotect(dll,4096,PROT_READ|PROT_WRITE);put(dll + 0x88, uint32_t{0});mprotect(dll,4096,PROT_READ);
        }
        std::cout << "ok" << std::endl;
    }
}
