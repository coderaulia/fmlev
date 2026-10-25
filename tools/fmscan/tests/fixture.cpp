// Synthetic target: changes its own memory only, permits sibling scanner reads.
#include <sys/mman.h>
#include <sys/prctl.h>
#include <poll.h>
#include <unistd.h>
#include <fcntl.h>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <string>

int main(int argc, char** argv) {
    if (argc != 2 || prctl(PR_SET_PTRACER, PR_SET_PTRACER_ANY) != 0) return 1;
    int fd = open(argv[1], O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (fd < 0 || ftruncate(fd, 8192)) return 2;
    auto module = static_cast<uint8_t*>(mmap(nullptr, 8192, PROT_READ | PROT_WRITE, MAP_PRIVATE, fd, 0));
    auto heap = static_cast<uint64_t*>(mmap(nullptr, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0));
    if (module == MAP_FAILED || heap == MAP_FAILED) return 3;
    // Match Proton: only the PE header retains the DLL pathname.
    if (mmap(module + 4096, 4096, PROT_READ | PROT_WRITE,
             MAP_FIXED | MAP_PRIVATE | MAP_ANONYMOUS, -1, 0) == MAP_FAILED) return 4;
    auto base = reinterpret_cast<uint64_t>(module), obj = reinterpret_cast<uint64_t>(heap);
    uint16_t mz = 0x5a4d, magic = 0x20b;
    uint32_t pe_offset = 0x80, pe = 0x4550, image_size = 8192;
    memcpy(module, &mz, 2);
    memcpy(module + 0x3c, &pe_offset, 4);
    memcpy(module + 0x80, &pe, 4);
    memcpy(module + 0x98, &magic, 2);
    memcpy(module + 0xd0, &image_size, 4);
    uint32_t col[6]{1, 0, 0, 0x1300, 0, 0x1100};
    memcpy(module + 0x1100, col, sizeof(col));
    uint64_t locator = base + 0x1100;
    memcpy(module + 0x11f8, &locator, 8);
    memcpy(module + 0x1310, ".?AVTEST_OBJECT@@", 17);
    heap[0] = base + 0x1200;
    heap[1] = obj + 0x400;
    heap[2] = 42;
    heap[3] = 0x3fc00000; // f32 1.5
    heap[4] = (2025U << 16) | 0xfe00 | 42;
    heap[7] = 1000;
    heap[0x400 / 8] = base + 0x1200;
    heap[0x408 / 8] = obj; // cycle
    heap[0x800 / 8] = obj + 0x900;
    heap[0x808 / 8] = obj + 0x910;
    heap[0x810 / 8] = obj + 0x918;
    heap[0x900 / 8] = obj;
    heap[0x908 / 8] = obj + 0x400;
    heap[0xa00 / 8] = obj + 0x900; // corrupt vector
    heap[0xa08 / 8] = obj + 0x800;
    if (mprotect(module, 8192, PROT_READ)) return 5;
    std::cout << getpid() << " " << std::hex << obj << " " << base + 0x1200 << std::endl;
    while (true) {
        pollfd p{STDIN_FILENO, POLLIN, 0};
        int ready = poll(&p, 1, 20);
        ++heap[7]; // background noise
        if (ready > 0) {
            std::string cmd; if (!std::getline(std::cin, cmd) || cmd == "quit") break;
            if (cmd == "action") ++heap[6];
            if (cmd == "tail") reinterpret_cast<uint8_t*>(heap)[0x40] ^= 1;
            std::cout << "ok" << std::endl;
        }
    }
    return 0;
}
